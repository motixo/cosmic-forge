// SPDX-License-Identifier: GPL-3.0-only
use anyhow::{Context, Result, ensure};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{fs, io::Write, path::Path};

fn installed_hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn valid_tag(tag: &str) -> bool {
    !tag.is_empty() && tag.len() <= 256 && !tag.chars().any(char::is_control)
}

pub(crate) fn save_release_tag(path: &Path, tag: &str, installed: &[u8]) -> Result<()> {
    ensure!(valid_tag(tag), "Invalid release tag");
    ensure!(!path.is_symlink(), "Preserved a replaced release record");
    let bytes =
        serde_json::to_vec(&serde_json::json!({"tag":tag, "sha256":installed_hash(installed)}))?;
    if fs::read(path).is_ok_and(|old| old == bytes) {
        return Ok(());
    }
    let parent = path.parent().context("Missing release record directory")?;
    fs::create_dir_all(parent)?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(&bytes)?;
    file.as_file().sync_all()?;
    file.persist(path).map_err(|e| e.error)?;
    Ok(())
}

pub(crate) fn release_tag(path: &Path, installed: &[u8]) -> Option<String> {
    let record: Value = serde_json::from_slice(&fs::read(path).ok()?).ok()?;
    let tag = record["tag"].as_str()?;
    (valid_tag(tag) && record["sha256"].as_str()? == installed_hash(installed))
        .then(|| tag.to_owned())
}
