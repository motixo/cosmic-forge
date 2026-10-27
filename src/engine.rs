// SPDX-License-Identifier: GPL-3.0-only
use crate::store::xdg;
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    path::PathBuf,
    process::{Command, Stdio},
};

pub const REPOSITORY: &str = "motixo/cosmic-forge-engine";
pub const ASSET: &str = "cosmic-forge-engine.json";
// Only offline stock recovery is embedded; engine setup uses verified releases.
const RECOVERY: &str = include_str!("../engine/recovery.py");

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Progress {
    Checking,
    Downloading,
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
            Self::Preparing => "Downloading and patching compositor source…",
            Self::Building => "Compiling the glass renderer…",
            Self::Installing => "Installing the glass renderer…",
            Self::Restoring => "Restoring the original COSMIC compositor…",
            Self::Complete => "Finished · sign out and back in to activate",
            Self::AlreadyInstalled => "This renderer version is already installed",
        }
    }
    pub fn finished(self) -> bool {
        matches!(self, Self::Complete | Self::AlreadyInstalled)
    }
}

pub fn setup(restore: bool, mut progress: impl FnMut(Progress)) -> Result<String> {
    progress(Progress::Checking);
    let cache = xdg("XDG_CACHE_HOME", ".cache").join("cosmic-forge/renderer");
    let root = if restore {
        let hash: String = Sha256::digest(RECOVERY.as_bytes())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        cache_files(
            &std::collections::BTreeMap::from([("recovery.py".into(), RECOVERY.into())]),
            &cache,
            &hash,
        )?
    } else {
        progress(Progress::Downloading);
        let (status, release) = crate::packs::download(&format!(
            "https://api.github.com/repos/{REPOSITORY}/releases/latest"
        ))?;
        ensure!(
            status == 200,
            "No compositor engine release has been published yet"
        );
        prepare_from_release(&cache, &release, |url| {
            let (status, bytes) = crate::packs::download(url)?;
            ensure!(status == 200, "The engine release asset is unavailable");
            Ok(bytes)
        })?
    };
    let log_path: PathBuf =
        xdg("XDG_STATE_HOME", ".local/state").join("cosmic-forge/engine-build.log");
    fs::create_dir_all(log_path.parent().unwrap())?;
    let mut log = fs::File::create(&log_path)?;
    let mut command = Command::new("python3");
    command
        .arg(root.join(if restore { "recovery.py" } else { "install.py" }))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(log.try_clone()?);
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
    Ok(if last == Progress::AlreadyInstalled {
        "The matching Forge glass renderer is already installed. No rebuild or reinstall needed."
            .into()
    } else if restore {
        "Original COSMIC compositor restored. Sign out and back in to activate it.".into()
    } else {
        "Glass renderer installed. Sign out and back in to activate it.".into()
    })
}
