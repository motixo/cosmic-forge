// SPDX-License-Identifier: GPL-3.0-only
//! Verified declarative mod catalogs; downloaded JSON never runs code.
use crate::{catalog::valid_identifier, packs, store::xdg};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::HashSet,
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
pub const REPOSITORY: &str = "motixo/cosmic-forge-mods";
pub const ASSET: &str = "cosmic-forge-mods.json";
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Catalog {
    pub format: u32,
    pub revision: u64,
    pub mods: Vec<Mod>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mod {
    pub id: String,
    pub section: String,
    pub name: String,
    pub description: String,
    pub engine_revision: u64,
    pub controls: Vec<Control>,
    #[serde(default)]
    pub preset: std::collections::BTreeMap<String, Value>,
}
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Boolean,
    Integer,
    ThemeColor,
    Image,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Control {
    pub key: String,
    pub kind: Kind,
    pub label: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub min: i32,
    #[serde(default)]
    pub max: i32,
    #[serde(default)]
    pub unit: String,
    #[serde(default)]
    pub default: Value,
}
impl Control {
    pub fn validate_value(&self, value: &Value) -> bool {
        match self.kind {
            Kind::Boolean => value.is_boolean(),
            Kind::Integer => value
                .as_i64()
                .is_some_and(|v| v >= i64::from(self.min) && v <= i64::from(self.max)),
            Kind::ThemeColor | Kind::Image => false,
        }
    }
}
pub fn parse(bytes: &[u8]) -> Result<Catalog> {
    ensure!(
        bytes.len() <= packs::MAX_PACK as usize,
        "Mod catalog is too large"
    );
    let catalog: Catalog = serde_json::from_slice(bytes)?;
    ensure!(
        catalog.format == 1 && catalog.revision > 0 && catalog.mods.len() <= 128,
        "Unsupported mod catalog"
    );
    let mut ids = HashSet::new();
    for m in &catalog.mods {
        ensure!(
            valid_identifier(&m.id)
                && ids.insert(&m.id)
                && matches!(m.section.as_str(), "panel" | "lock_screen")
                && !m.name.is_empty()
                && m.name.len() <= 100
                && m.description.len() <= 2000
                && m.engine_revision > 0
                && !m.controls.is_empty()
                && m.controls.len() <= 32,
            "Invalid mod definition"
        );
        let mut keys = HashSet::new();
        for c in &m.controls {
            ensure!(
                valid_identifier(&c.key)
                    && keys.insert(&c.key)
                    && !c.label.is_empty()
                    && c.label.len() <= 100
                    && c.description.len() <= 1000
                    && c.unit.len() <= 16,
                "Invalid mod control"
            );
            let (kind, range) = match (m.section.as_str(), c.key.as_str()) {
                ("lock_screen", "wallpaper") => (Kind::Image, None),
                ("lock_screen", "media_enabled") => (Kind::Boolean, None),
                ("panel", "penumbra_enabled") => (Kind::Boolean, None),
                ("panel", "gradient_height") => (Kind::Integer, Some((16, 512))),
                ("panel", "panel_opacity" | "maximized_opacity") => (Kind::Integer, Some((0, 100))),
                ("panel", "forge_colors") => (Kind::ThemeColor, None),
                (_, key) => {
                    ensure!(
                        key.starts_with("forge_")
                            && key.len() <= 100
                            && !matches!(c.kind, Kind::ThemeColor | Kind::Image),
                        "Unsupported mod setting"
                    );
                    (c.kind, Some((-100000, 100000)))
                }
            };
            ensure!(c.kind == kind, "Mod setting has the wrong control type");
            if c.kind == Kind::Integer {
                let (min, max) = range.context("Missing integer bounds")?;
                ensure!(
                    c.min >= min && c.max <= max && c.min < c.max,
                    "Invalid mod control range"
                );
            }
            ensure!(
                c.default.is_null() || c.validate_value(&c.default),
                "Invalid mod control default"
            );
        }
        for (key, value) in &m.preset {
            ensure!(
                m.controls
                    .iter()
                    .any(|c| c.key == *key && c.validate_value(value)),
                "Preset needs a compatible declared control"
            );
        }
    }
    Ok(catalog)
}
pub fn root() -> PathBuf {
    xdg("XDG_DATA_HOME", ".local/share").join("cosmic-forge/mods")
}
pub fn load(directory: &Path) -> Result<Catalog> {
    match fs::read(directory.join(ASSET)) {
        Ok(bytes) => {
            let cached = parse(&bytes)?;
            let builtin = parse(include_bytes!("../mods/catalog.json"))?;
            Ok(if cached.revision < builtin.revision {
                builtin
            } else {
                cached
            })
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            parse(include_bytes!("../mods/catalog.json"))
        }
        Err(e) => Err(e.into()),
    }
}
fn atomic(directory: &Path, name: &str, bytes: &[u8]) -> Result<()> {
    ensure!(!directory.is_symlink(), "Preserved a replaced mod cache");
    fs::create_dir_all(directory)?;
    let path = directory.join(name);
    ensure!(!path.is_symlink(), "Preserved a replaced mod catalog");
    let mut temp = tempfile::NamedTempFile::new_in(directory)?;
    temp.write_all(bytes)?;
    temp.as_file().sync_all()?;
    temp.persist(path).map_err(|e| e.error)?;
    Ok(())
}
pub fn update_from_release(
    directory: &Path,
    metadata: &[u8],
    fetch: impl FnOnce(&str) -> Result<Vec<u8>>,
) -> Result<String> {
    let release: Value = serde_json::from_slice(metadata)?;
    let asset = release["assets"]
        .as_array()
        .and_then(|a| a.iter().find(|a| a["name"] == ASSET))
        .context("No mod catalog published yet")?;
    let url = asset["browser_download_url"]
        .as_str()
        .context("Missing mod catalog URL")?;
    ensure!(
        url.starts_with(&format!(
            "https://github.com/{REPOSITORY}/releases/download/"
        )),
        "Unexpected mod catalog repository"
    );
    let size = asset["size"].as_u64().context("Missing mod catalog size")?;
    ensure!(
        size > 0 && size <= packs::MAX_PACK,
        "Invalid mod catalog size"
    );
    let bytes = fetch(url)?;
    ensure!(bytes.len() as u64 == size, "Incomplete mod catalog");
    packs::verify_checksum(
        &bytes,
        asset["digest"]
            .as_str()
            .context("Missing mod catalog checksum")?,
    )?;
    let catalog = parse(&bytes)?;
    let current = load(directory)?;
    ensure!(
        catalog.revision >= current.revision,
        "Preserved newer installed mod catalog"
    );
    if catalog == current {
        return Ok("Mod list is up to date".into());
    }
    ensure!(
        catalog.revision > current.revision,
        "Changed mod catalogs need a higher revision"
    );
    atomic(directory, ASSET, &bytes)?;
    Ok(format!("Updated mod list · {} mods", catalog.mods.len()))
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
pub fn due(directory: &Path) -> bool {
    now().saturating_sub(
        fs::read_to_string(directory.join("last-check"))
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0),
    ) >= packs::CHECK_INTERVAL
}
pub fn update(directory: &Path) -> Result<String> {
    let (status, metadata) = packs::download(&format!(
        "https://api.github.com/repos/{REPOSITORY}/releases/latest"
    ))?;
    ensure!(
        status == 200,
        "No mod catalog release has been published yet"
    );
    let note = update_from_release(directory, &metadata, |url| {
        let (status, bytes) = packs::download(url)?;
        ensure!(status == 200, "Mod catalog is unavailable");
        Ok(bytes)
    })?;
    atomic(directory, "last-check", now().to_string().as_bytes())?;
    Ok(note)
}
