// SPDX-License-Identifier: GPL-3.0-only
use crate::store::xdg;
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

pub const REPOSITORY: &str = "motixo/cosmic-forge-engine";
pub const ASSET: &str = "cosmic-forge-engine.json";
pub const WALLPAPER_RENDERER_REVISION: u64 = 5;
// Only offline stock recovery is embedded; engine setup uses verified releases.
const RECOVERY: &str = include_str!("../engine/recovery.py");
const LOGIN_PATH: &str = include_str!("../engine/login_path.py");

#[derive(serde::Deserialize)]
struct Bundle {
    format: u32,
    abi: u32,
    files: std::collections::BTreeMap<String, String>,
}

fn cache_files(
    files: &std::collections::BTreeMap<String, String>,
    cache: &std::path::Path,
    hash: &str,
) -> Result<PathBuf> {
    for name in files.keys() {
        ensure!(
            name.len() <= 200
                && name.split('/').all(|part| !part.is_empty()
                    && part != "."
                    && part != ".."
                    && part
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))),
            "Invalid engine bundle path: {name}"
        );
    }
    let root = cache.join(hash);
    ensure!(!root.is_symlink(), "Preserved a replaced engine cache");
    fs::create_dir_all(&root)?;
    for (name, data) in files {
        let path = root.join(name);
        let mut directory = root.clone();
        let parts: Vec<_> = name.split('/').collect();
        for part in &parts[..parts.len() - 1] {
            directory.push(part);
            ensure!(
                !directory.is_symlink(),
                "Preserved a replaced engine cache directory"
            );
            fs::create_dir_all(&directory)?;
        }
        ensure!(!path.is_symlink(), "Preserved a replaced engine cache file");
        if fs::read(&path).is_ok_and(|existing| existing == data.as_bytes()) {
            continue;
        }
        let mut file = tempfile::NamedTempFile::new_in(&directory)?;
        file.write_all(data.as_bytes())?;
        file.as_file().sync_all()?;
        file.persist(&path).map_err(|e| e.error)?;
    }
    Ok(root)
}

pub fn prepare_from_release(
    cache: &std::path::Path,
    release: &[u8],
    fetch: impl FnOnce(&str) -> Result<Vec<u8>>,
) -> Result<PathBuf> {
    let release: serde_json::Value = serde_json::from_slice(release)?;
    let asset = release["assets"]
        .as_array()
        .and_then(|assets| assets.iter().find(|a| a["name"] == ASSET))
        .context("No compositor engine package has been published yet")?;
    let url = asset["browser_download_url"]
        .as_str()
        .context("Missing engine download URL")?;
    ensure!(
        url.starts_with(&format!(
            "https://github.com/{REPOSITORY}/releases/download/"
        )),
        "Unexpected engine asset repository"
    );
    let size = asset["size"]
        .as_u64()
        .context("Missing engine package size")?;
    ensure!(
        size > 0 && size <= crate::packs::MAX_PACK,
        "Invalid engine package size"
    );
    let bytes = fetch(url)?;
    ensure!(bytes.len() as u64 == size, "Incomplete engine package");
    crate::packs::verify_checksum(
        &bytes,
        asset["digest"]
            .as_str()
            .context("Missing engine package digest")?,
    )?;
    let bundle: Bundle = serde_json::from_slice(&bytes)?;
    ensure!(
        bundle.format == 1 && bundle.abi == crate::runtime::ABI,
        "This engine requires a newer Forge app"
    );
    ensure!(
        !bundle.files.is_empty() && bundle.files.len() <= 128,
        "Invalid engine file count"
    );
    for name in ["install.py", "build.py", "recovery.py", "release.json"] {
        ensure!(
            bundle.files.contains_key(name),
            "Missing engine file: {name}"
        );
    }
    let config: serde_json::Value = serde_json::from_str(&bundle.files["release.json"])?;
    ensure!(
        config["abi"] == bundle.abi && config["renderer_revision"].as_u64().is_some_and(|r| r > 0),
        "Invalid engine release metadata"
    );
    let hash: String = Sha256::digest(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    cache_files(&bundle.files, cache, &hash)
}

fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn receipt(bundle: &str, record: &[u8]) -> serde_json::Value {
    serde_json::json!({"component":"compositor","bundle_sha256":bundle,"installation_sha256":hash(record)})
}
fn update_needed(state: &Path, bundle: &str) -> Result<bool> {
    let record = state.join("engine-installation.json");
    if !record.exists() {
        return Ok(false);
    }
    let bytes = fs::read(record)?;
    let saved: Option<serde_json::Value> = fs::read(state.join("engine-compositor-receipt.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok());
    Ok(saved.as_ref() != Some(&receipt(bundle, &bytes)))
}

fn remember_release(state: &Path, bundle: &str, tag: &str) -> Result<()> {
    let Ok(record) = fs::read(state.join("engine-installation.json")) else {
        return Ok(());
    };
    let saved: Option<serde_json::Value> = fs::read(state.join("engine-compositor-receipt.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok());
    if saved.as_ref() == Some(&receipt(bundle, &record)) {
        crate::updates::save_release_tag(&state.join("engine-release.json"), tag, &record)?;
    }
    Ok(())
}
pub fn update_available() -> Result<bool> {
    let (status, metadata) = crate::packs::download(&format!(
        "https://api.github.com/repos/{REPOSITORY}/releases/latest"
    ))?;
    ensure!(status == 200, "No engine release has been published yet");
    update_available_from_release(&metadata)
}
pub fn update_available_from_release(metadata: &[u8]) -> Result<bool> {
    let cache = xdg("XDG_CACHE_HOME", ".cache").join("cosmic-forge/renderer");
    let root = prepare_from_release(&cache, metadata, |url| {
        let (status, bytes) = crate::packs::download(url)?;
        ensure!(status == 200, "The engine package is unavailable");
        Ok(bytes)
    })?;
    let state = xdg("XDG_STATE_HOME", ".local/state").join("cosmic-forge");
    let bundle = root
        .file_name()
        .and_then(|v| v.to_str())
        .context("Missing engine package hash")?;
    let available = update_needed(&state, bundle)?;
    if !available {
        let metadata: serde_json::Value = serde_json::from_slice(metadata)?;
        if let Some(tag) = metadata["tag_name"].as_str() {
            remember_release(&state, bundle, tag)?;
        }
    }
    Ok(available)
}
fn save_receipt(root: &std::path::Path) -> Result<()> {
    let state = xdg("XDG_STATE_HOME", ".local/state").join("cosmic-forge");
    let record = state.join("engine-installation.json");
    if !record.exists() {
        return Ok(());
    }
    let value = receipt(
        root.file_name()
            .and_then(|v| v.to_str())
            .context("Missing engine package hash")?,
        &fs::read(record)?,
    );
    let path = state.join("engine-compositor-receipt.json");
    ensure!(!path.is_symlink(), "Preserved a replaced engine receipt");
    let mut file = tempfile::NamedTempFile::new_in(state)?;
    file.write_all(&serde_json::to_vec(&value)?)?;
    file.as_file().sync_all()?;
    file.persist(path).map_err(|e| e.error)?;
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Install,
    Restore,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Progress {
    Checking,
    Downloading,
    DownloadingBinary,
    Preparing,
    Building,
    Installing,
    Restoring,
    Complete,
    AlreadyInstalled,
}

impl Progress {
    fn from_line(line: &str) -> Option<Self> {
        Some(match line.strip_prefix("FORGE_PROGRESS:")? {
            "checking" => Self::Checking,
            "downloading" => Self::Downloading,
            "downloading_binary" => Self::DownloadingBinary,
            "preparing" => Self::Preparing,
            "building" => Self::Building,
            "installing" => Self::Installing,
            "restoring" => Self::Restoring,
            "complete" => Self::Complete,
            "already_installed" => Self::AlreadyInstalled,
            _ => return None,
        })
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Checking => "Checking installed renderer…",
            Self::Downloading => "Downloading the compositor engine package…",
            Self::DownloadingBinary => "Downloading the prebuilt compositor…",
            Self::Preparing => "Downloading and patching compositor source…",
            Self::Building => "Compiling the glass renderer…",
            Self::Installing => "Installing the glass renderer…",
            Self::Restoring => "Restoring the original COSMIC compositor…",
            Self::Complete => "Finished · log out and back in to activate",
            Self::AlreadyInstalled => "This renderer version is already installed",
        }
    }
    pub fn finished(self) -> bool {
        matches!(self, Self::Complete | Self::AlreadyInstalled)
    }
}

pub fn setup(action: Action, mut progress: impl FnMut(Progress)) -> Result<String> {
    let restore = action == Action::Restore;
    let mut release_tag = None;
    progress(Progress::Checking);
    let cache = xdg("XDG_CACHE_HOME", ".cache").join("cosmic-forge/renderer");
    let root = if restore {
        let files = std::collections::BTreeMap::from([
            ("recovery.py".into(), RECOVERY.into()),
            ("login_path.py".into(), LOGIN_PATH.into()),
        ]);
        let hash = Sha256::digest(serde_json::to_vec(&files)?)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        cache_files(&files, &cache, &hash)?
    } else {
        progress(Progress::Downloading);
        let (status, release) = crate::packs::download(&format!(
            "https://api.github.com/repos/{REPOSITORY}/releases/latest"
        ))?;
        ensure!(
            status == 200,
            "No compositor engine release has been published yet"
        );
        let metadata: serde_json::Value = serde_json::from_slice(&release)?;
        release_tag = metadata["tag_name"].as_str().map(str::to_owned);
        prepare_from_release(&cache, &release, |url| {
            let (status, bytes) = crate::packs::download(url)?;
            ensure!(status == 200, "The engine release asset is unavailable");
            Ok(bytes)
        })?
    };
    let log_path = xdg("XDG_STATE_HOME", ".local/state").join("cosmic-forge/engine-build.log");
    fs::create_dir_all(log_path.parent().unwrap())?;
    let mut log = fs::File::create(&log_path)?;
    let mut command = Command::new("python3");
    command
        .arg(root.join(if restore { "recovery.py" } else { "install.py" }))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(log.try_clone()?);
    if let Some(tag) = release_tag.as_deref() {
        command.args(["--release-tag", tag]);
    }
    let mut child = command
        .spawn()
        .context("The engine installer requires Python 3")?;
    let mut last = Progress::Checking;
    let output = BufReader::new(child.stdout.take().context("Missing installer output")?);
    let read_result = (|| -> Result<()> {
        let mut log_error = None;
        for line in output.lines() {
            let line = line?;
            if let Err(error) = writeln!(log, "{line}") {
                log_error = Some(error);
            }
            if let Some(stage) = Progress::from_line(&line) {
                last = stage;
                progress(stage);
            }
        }
        if let Some(error) = log_error {
            return Err(error.into());
        }
        Ok(())
    })();
    let result = child.wait()?;
    read_result?;
    let output = fs::read_to_string(&log_path).unwrap_or_default();
    let tail = output
        .lines()
        .rev()
        .take(8)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("\n");
    ensure!(
        result.success(),
        "Renderer setup failed: {tail}\nFull log: {}",
        log_path.display()
    );
    if !restore {
        save_receipt(&root)?;
        if let Some(tag) = release_tag.as_deref() {
            let state = xdg("XDG_STATE_HOME", ".local/state").join("cosmic-forge");
            remember_release(
                &state,
                root.file_name()
                    .and_then(|v| v.to_str())
                    .context("Missing engine package hash")?,
                tag,
            )?;
        }
    }
    Ok(if last == Progress::AlreadyInstalled {
        "The matching Forge glass renderer is already installed. No rebuild or reinstall needed."
            .into()
    } else if restore {
        "Original COSMIC compositor restored. Log out and back in to activate it.".into()
    } else {
        "Glass renderer installed. Log out and back in to activate it.".into()
    })
}

#[cfg(test)]
mod update_tests {
    use super::*;

    #[test]
    fn compositor_updates_preserve_and_ignore_legacy_mod_installations() {
        let tmp = tempfile::tempdir().unwrap();
        let state = tmp.path();
        let legacy = [
            "panel-installation.json",
            "greeter-installation.json",
            "engine-panel-receipt.json",
            "engine-greeter-receipt.json",
        ];
        for name in legacy {
            fs::write(state.join(name), "preserved legacy data").unwrap();
        }
        assert!(!update_needed(state, "bundle-a").unwrap());
        let record = b"installed compositor";
        fs::write(state.join("engine-installation.json"), record).unwrap();
        assert!(update_needed(state, "bundle-a").unwrap());
        fs::write(
            state.join("engine-compositor-receipt.json"),
            serde_json::to_vec(&receipt("bundle-a", record)).unwrap(),
        )
        .unwrap();
        assert!(!update_needed(state, "bundle-a").unwrap());
        assert!(update_needed(state, "bundle-b").unwrap());
        let tag_path = state.join("engine-release.json");
        remember_release(state, "bundle-b", "engine-v0.7.0").unwrap();
        assert!(crate::updates::release_tag(&tag_path, record).is_none());
        remember_release(state, "bundle-a", "engine-v0.6.0").unwrap();
        assert_eq!(
            crate::updates::release_tag(&tag_path, record).as_deref(),
            Some("engine-v0.6.0")
        );
        fs::write(
            state.join("engine-installation.json"),
            b"changed installation",
        )
        .unwrap();
        assert!(update_needed(state, "bundle-a").unwrap());
        fs::remove_file(state.join("engine-installation.json")).unwrap();
        assert!(!update_needed(state, "bundle-b").unwrap());
        for name in legacy {
            assert_eq!(
                fs::read_to_string(state.join(name)).unwrap(),
                "preserved legacy data"
            );
        }
    }
}
