// SPDX-License-Identifier: GPL-3.0-only
use crate::{
    catalog::{self, Effect},
    store::xdg,
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::hash::{Hash, Hasher};
use std::{
    collections::HashSet,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

pub fn fingerprint(data_root: &Path) -> u64 {
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    for name in ["effects", "packs"] {
        if let Ok(entries) = fs::read_dir(data_root.join(name)) {
            let mut paths: Vec<_> = entries.flatten().map(|e| e.path()).collect();
            paths.sort();
            for path in paths {
                if path.extension().is_some_and(|x| x == "json") {
                    path.hash(&mut hash);
                    if let Ok(m) = fs::metadata(path) {
                        m.len().hash(&mut hash);
                        m.modified().ok().hash(&mut hash);
                    }
                }
            }
        }
    }
    hash.finish()
}

pub fn engine_active() -> bool {
    let path = xdg("XDG_RUNTIME_DIR", ".cache").join("cosmic-forge-engine-v1");
    let Ok(text) = fs::read_to_string(path) else {
        return false;
    };
    let Ok(pid) = text.trim().parse::<u32>() else {
        return false;
    };
    fs::read_to_string(format!("/proc/{pid}/comm")).is_ok_and(|s| s.trim() == "cosmic-comp")
}

pub const REPOSITORY: &str = "motixo/cosmic-forge-shaders";
pub const ASSET: &str = "cosmic-forge-glass.json";
pub const MAX_PACK: u64 = 2 * 1024 * 1024;
pub const CHECK_INTERVAL: u64 = 6 * 60 * 60;
const OFFICIAL: &str = "cosmic-glass";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pack {
    pub format: u32,
    pub id: String,
    pub revision: u64,
    pub effects: Vec<Effect>,
}

pub fn root() -> PathBuf {
    xdg("XDG_DATA_HOME", ".local/share").join("cosmic-forge")
}

pub fn installed_revision(data_root: &Path) -> Option<u64> {
    let pack =
        parse(&read(&data_root.join("packs").join(format!("{OFFICIAL}.json"))).ok()?).ok()?;
    (pack.id == OFFICIAL).then_some(pack.revision)
}

pub fn installed_release_tag(data_root: &Path) -> Option<String> {
    let bytes = read(&data_root.join("packs").join(format!("{OFFICIAL}.json"))).ok()?;
    crate::updates::release_tag(&data_root.join("official-release.json"), &bytes)
}

impl Pack {
    pub fn validate(&self) -> Result<()> {
        ensure!(self.format == 1, "Unsupported effect pack format");
        ensure!(
            catalog::valid_identifier(&self.id) && self.revision > 0,
            "Invalid pack identifier or revision"
        );
        ensure!(
            !self.effects.is_empty() && self.effects.len() <= 64,
            "A pack needs 1–64 glass effects"
        );
        catalog::validate_catalog(&self.effects)?;
        for effect in &self.effects {
            ensure!(
                effect.shader.is_some() || (self.id == OFFICIAL && effect.id == "frosted"),
                "An effect needs a glass shader"
            );
            ensure!(
                self.id == OFFICIAL
                    || !matches!(effect.id.as_str(), "frosted" | "crystal" | "aero"),
                "Community packs cannot replace built-in effects"
            );
            ensure!(effect.controls.len() <= 32, "Too many effect controls");
            for control in &effect.controls {
                if control.target == "comp" {
                    ensure!(
                        (control.key.starts_with("forge_") && control.key != "forge_runtime")
                            || matches!(
                                control.key.as_str(),
                                "crystal_everyday"
                                    | "crystal_blur_amount"
                                    | "crystal_thickness"
                                    | "crystal_refraction"
                                    | "crystal_density"
                                    | "aero_blur_amount"
                                    | "aero_color"
                                    | "aero_reflections"
                            ),
                        "Unsupported glass control: {}",
                        control.key
                    );
                    ensure!(
                        control.legacy.is_none()
                            || control.legacy.as_deref() == Some("crystal_blur"),
                        "Unsupported legacy control"
                    );
                }
            }
        }
        Ok(())
    }
}

pub fn parse(bytes: &[u8]) -> Result<Pack> {
    ensure!(bytes.len() as u64 <= MAX_PACK, "Effect pack exceeds 2 MiB");
    let pack: Pack = serde_json::from_slice(bytes).context("Invalid effect pack JSON")?;
    pack.validate()?;
    Ok(pack)
}

pub fn read(path: &Path) -> Result<Vec<u8>> {
    let mut data = Vec::new();
    fs::File::open(path)?
        .take(MAX_PACK + 1)
        .read_to_end(&mut data)?;
    ensure!(data.len() as u64 <= MAX_PACK, "Effect pack exceeds 2 MiB");
    Ok(data)
}

pub fn import_files(data_root: &Path, paths: &[PathBuf]) -> Result<String> {
    let extension = |path: &Path, expected: &str| {
        path.extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case(expected))
    };
    if let [path] = paths {
        ensure!(
            extension(path, "json"),
            "Select a JSON pack, or both a .frag file and its JSON settings"
        );
        return install(data_root, &read(path)?, false);
    }
    ensure!(
        paths.len() == 2,
        "Select one JSON settings file and one .frag shader"
    );
    let metadata = paths
        .iter()
        .find(|p| extension(p, "json"))
        .context("Select a JSON settings file with the .frag shader")?;
    let shader = paths
        .iter()
        .find(|p| extension(p, "frag"))
        .context("Select a .frag shader with the JSON settings")?;
    let mut source = Vec::new();
    fs::File::open(shader)
        .with_context(|| format!("Could not read {}", shader.display()))?
        .take(crate::runtime::MAX_SHADER as u64 + 1)
        .read_to_end(&mut source)?;
    ensure!(
        source.len() <= crate::runtime::MAX_SHADER,
        "Shader exceeds 64 KiB"
    );
    let source = String::from_utf8(source).context("Shader must be UTF-8 text")?;
    let mut document: serde_json::Value =
        serde_json::from_slice(&read(metadata)?).context("Invalid JSON settings")?;
    let effect = if document.get("effects").is_some() {
        let effects = document["effects"]
            .as_array_mut()
            .context("Expected an effects array")?;
        ensure!(
            effects.len() == 1,
            "A .frag + JSON import must describe exactly one effect"
        );
        &mut effects[0]
    } else {
        &mut document
    };
    effect
        .get_mut("shader")
        .and_then(serde_json::Value::as_object_mut)
        .context("JSON settings need shader.base and optional shader.uniforms")?
        .insert("source".into(), source.into());
    if document.get("effects").is_none() {
        let revision = document
            .as_object_mut()
            .context("Expected an effect object")?
            .remove("revision")
            .unwrap_or(serde_json::json!(1));
        document = serde_json::json!({"format": 1, "id": document["id"], "revision": revision, "effects": [document]});
    }
    install(data_root, &serde_json::to_vec(&document)?, false)
}

pub fn merge(
    effects: &mut Vec<Effect>,
    directory: &Path,
    replacement: Option<&Pack>,
) -> Result<()> {
    let mut packs = Vec::new();
    if directory.exists() {
        let mut paths = fs::read_dir(directory)?
            .map(|e| e.map(|e| e.path()))
            .collect::<std::io::Result<Vec<_>>>()?;
        paths.sort();
        for path in paths
            .into_iter()
            .filter(|p| p.extension().is_some_and(|x| x == "json"))
        {
            if replacement.is_some_and(|p| path.file_stem().is_some_and(|s| s == p.id.as_str())) {
                continue;
            }
            let pack =
                parse(&read(&path)?).with_context(|| format!("Invalid pack {}", path.display()))?;
            ensure!(
                path.file_stem().is_some_and(|s| s == pack.id.as_str()),
                "Pack filename does not match its identifier"
            );
            packs.push(pack);
        }
    }
    if let Some(pack) = replacement {
        packs.push(pack.clone());
    }
    let mut ids = HashSet::new();
    let mut official_order = Vec::new();
    for pack in packs {
        if pack.id == OFFICIAL {
            official_order = pack
                .effects
                .iter()
                .map(|effect| effect.id.clone())
                .collect();
        }
        for mut effect in pack.effects {
            ensure!(
                ids.insert(effect.id.clone()),
                "Effect appears in multiple packs: {}",
                effect.id
            );
            if pack.id == OFFICIAL && matches!(effect.id.as_str(), "frosted" | "crystal" | "aero") {
                let index = effects
                    .iter()
                    .position(|e| e.id == effect.id)
                    .context("Missing built-in effect")?;
                effects[index] = effect;
            } else {
                if pack.id != OFFICIAL {
                    effect.pack_id = Some(pack.id.clone());
                }
                effects.push(effect);
            }
        }
    }
    if !official_order.is_empty() {
        effects.sort_by_key(|effect| {
            official_order
                .iter()
                .position(|id| id == &effect.id)
                .unwrap_or(official_order.len())
        });
    }
    Ok(())
}

fn atomic(path: &Path, data: &[u8]) -> Result<()> {
    let parent = path.parent().context("Missing pack directory")?;
    fs::create_dir_all(parent)?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(data)?;
    file.as_file().sync_all()?;
    file.persist(path).map_err(|e| e.error)?;
    Ok(())
}

pub fn install(data_root: &Path, bytes: &[u8], official: bool) -> Result<String> {
    let pack = parse(bytes)?;
    ensure!(
        official || pack.id != OFFICIAL,
        "This identifier is reserved for the official GitHub feed"
    );
    fs::create_dir_all(data_root)?;
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(data_root.join(".packs.lock"))?;
    lock.lock()?;
    catalog::load_with_pack(&data_root.join("effects"), Some(&pack))?;
    let path = data_root.join("packs").join(format!("{}.json", pack.id));
    if path.exists() {
        let old = read(&path)?;
        let previous = parse(&old)?;
        ensure!(
            previous
                .effects
                .iter()
                .all(|old| pack.effects.iter().any(|e| e.id == old.id)),
            "An update must retain existing effect identifiers"
        );
        ensure!(
            pack.revision >= previous.revision,
            "Pack revision cannot decrease"
        );
        if previous.revision == pack.revision {
            if serde_json::to_vec(&previous)? == serde_json::to_vec(&pack)? {
                return Ok("effects are up to date".into());
            }
            ensure!(!official, "Official pack revision must increase");
        }
        atomic(&path.with_extension("previous"), &old)?;
    }
    atomic(&path, bytes)?;
    Ok(format!(
        "Installed {} · revision {}",
        pack.id, pack.revision
    ))
}

pub fn remove_effect(data_root: &Path, pack_id: &str, effect_id: &str) -> Result<()> {
    ensure!(
        catalog::valid_identifier(pack_id)
            && catalog::valid_identifier(effect_id)
            && pack_id != OFFICIAL
            && !matches!(effect_id, "frosted" | "crystal" | "aero"),
        "Only user-imported effects can be removed"
    );
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(data_root.join(".packs.lock"))?;
    lock.lock()?;
    let path = data_root.join("packs").join(format!("{pack_id}.json"));
    let original = read(&path)?;
    let mut pack = parse(&original)?;
    ensure!(pack.id == pack_id, "Pack identifier mismatch");
    ensure!(
        pack.effects.iter().any(|e| e.id == effect_id),
        "Effect is not in this pack"
    );
    pack.effects.retain(|e| e.id != effect_id);
    if !pack.effects.is_empty() {
        pack.validate()?;
    }
    atomic(&path.with_extension("previous"), &original)?;
    if pack.effects.is_empty() {
        fs::remove_file(path)?;
    } else {
        atomic(&path, &serde_json::to_vec_pretty(&pack)?)?;
    }
    Ok(())
}

pub(crate) fn download(url: &str) -> Result<(u16, Vec<u8>)> {
    let file = tempfile::NamedTempFile::new()?;
    let result = Command::new("curl")
        .args([
            "--disable",
            "--silent",
            "--show-error",
            "--location",
            "--proto",
            "=https",
            "--proto-redir",
            "=https",
            "--connect-timeout",
            "10",
            "--max-time",
            "45",
            "--max-filesize",
            "2097152",
            "--user-agent",
            "COSMIC-Forge/0.4",
            "--write-out",
            "%{http_code}",
            "--output",
        ])
        .arg(file.path())
        .arg(url)
        .output()
        .context("GitHub downloads require curl")?;
    ensure!(
        result.status.success(),
        "GitHub download failed: {}",
        String::from_utf8_lossy(&result.stderr).trim()
    );
    let status = String::from_utf8(result.stdout)?
        .parse::<u16>()
        .context("Invalid GitHub HTTP response")?;
    ensure!(
        matches!(status, 200 | 404),
        "GitHub request returned HTTP {status}"
    );
    Ok((status, read(file.path())?))
}

pub(crate) fn verify_checksum(bytes: &[u8], digest: &str) -> Result<()> {
    let expected = digest
        .strip_prefix("sha256:")
        .context("Release asset needs a SHA-256 digest")?;
    ensure!(
        expected.len() == 64 && expected.bytes().all(|b| b.is_ascii_hexdigit()),
        "Invalid asset digest"
    );
    let actual: String = Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    ensure!(
        actual.eq_ignore_ascii_case(expected),
        "Release asset checksum mismatch"
    );
    Ok(())
}

pub fn verified_asset(bytes: &[u8], digest: &str) -> Result<Pack> {
    verify_checksum(bytes, digest)?;
    let pack = parse(bytes)?;
    ensure!(pack.id == OFFICIAL, "Unexpected official pack identifier");
    Ok(pack)
}

pub fn update_from_release(
    data_root: &Path,
    release: &[u8],
    fetch: impl FnOnce(&str) -> Result<Vec<u8>>,
) -> Result<String> {
    let release: serde_json::Value = serde_json::from_slice(release)?;
    let Some(asset) = release["assets"]
        .as_array()
        .and_then(|a| a.iter().find(|a| a["name"] == ASSET))
    else {
        return Ok("No effect pack published yet".into());
    };
    let url = asset["browser_download_url"]
        .as_str()
        .context("Missing release download URL")?;
    ensure!(
        url.starts_with(&format!(
            "https://github.com/{REPOSITORY}/releases/download/"
        )),
        "Unexpected release asset host or repository"
    );
    ensure!(
        asset["size"]
            .as_u64()
            .is_some_and(|size| size > 0 && size <= MAX_PACK),
        "Invalid release asset size"
    );
    let bytes = fetch(url)?;
    ensure!(
        Some(bytes.len() as u64) == asset["size"].as_u64(),
        "Incomplete release asset"
    );
    verified_asset(
        &bytes,
        asset["digest"]
            .as_str()
            .context("Missing release asset digest")?,
    )?;
    let note = install(data_root, &bytes, true)?;
    if let Some(tag) = release["tag_name"].as_str() {
        crate::updates::save_release_tag(&data_root.join("official-release.json"), tag, &bytes)?;
    }
    Ok(note)
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
pub fn due(data_root: &Path) -> bool {
    let checked = fs::read_to_string(data_root.join("last-check"))
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);
    now().saturating_sub(checked) >= CHECK_INTERVAL
}
pub fn update(data_root: &Path) -> Result<String> {
    let (status, release) = download(&format!(
        "https://api.github.com/repos/{REPOSITORY}/releases/latest"
    ))?;
    let result = if status == 404 {
        "No effect pack published yet".into()
    } else {
        update_from_release(data_root, &release, |url| {
            let (status, bytes) = download(url)?;
            ensure!(status == 200, "The release asset is no longer available");
            Ok(bytes)
        })?
    };
    atomic(&data_root.join("last-check"), now().to_string().as_bytes())?;
    Ok(result)
}
