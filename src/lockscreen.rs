// SPDX-License-Identifier: GPL-3.0-only
//! Independent Veil preferences; never change desktop wallpaper or authentication.
use crate::{
    mods,
    store::{Changes, Store, xdg},
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    io::{BufReader, Read},
    path::{Path, PathBuf},
};

#[derive(Debug, Default, PartialEq, Eq, Deserialize)]
pub struct Config {
    pub wallpaper: Option<PathBuf>,
    pub media_enabled: bool,
}
fn directory(store: &Store) -> PathBuf {
    store.root.join("cosmic/com.system76.CosmicLockscreen/v1")
}
fn read<T: serde::de::DeserializeOwned>(path: &Path, fallback: T) -> Result<T> {
    match fs::read_to_string(path) {
        Ok(s) => ron::from_str(&s).with_context(|| format!("Could not read {}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(fallback),
        Err(e) => Err(e.into()),
    }
}
pub fn load(store: &Store) -> Result<Config> {
    let dir = directory(store);
    Ok(Config {
        wallpaper: read(&dir.join("wallpaper"), None)?,
        media_enabled: read(&dir.join("media_enabled"), false)?,
    })
}
fn put(changes: &mut Changes, store: &Store, key: &str, value: &impl Serialize) -> Result<()> {
    let dir = directory(store);
    let file = dir.join(key);
    ensure!(
        !dir.is_symlink() && !dir.parent().unwrap().is_symlink() && !file.is_symlink(),
        "Preserved a replaced lock screen preference"
    );
    changes.insert(
        file,
        Some(format!("{}\n", ron::to_string(value)?).into_bytes()),
    );
    Ok(())
}
pub fn set_wallpaper(store: &mut Store, path: Option<PathBuf>) -> Result<bool> {
    ensure!(
        path.as_ref().is_none_or(|p| p.is_absolute() && p.is_file()),
        "Choose an existing image"
    );
    let mut changes = Changes::new();
    put(&mut changes, store, "wallpaper", &path)?;
    store.commit(changes, "Lock screen background")
}
pub fn control_value(store: &Store, control: &mods::Control) -> Result<Value> {
    if control.key == "media_enabled" {
        return Ok(json!(load(store)?.media_enabled));
    }
    ensure!(
        control.key.starts_with("forge_"),
        "Unsupported lock screen setting"
    );
    read(
        &directory(store).join(&control.key),
        control.default.clone(),
    )
}
pub fn apply_controls(
    store: &mut Store,
    controls: &[mods::Control],
    values: &BTreeMap<String, Value>,
) -> Result<bool> {
    let mut changes = Changes::new();
    for (key, value) in values {
        let c = controls
            .iter()
            .find(|c| c.key == *key)
            .context("Unknown lock screen setting")?;
        ensure!(
            (key == "media_enabled" || key.starts_with("forge_")) && c.validate_value(value),
            "Invalid lock screen setting"
        );
        match c.kind {
            mods::Kind::Boolean => put(&mut changes, store, key, &value.as_bool().unwrap())?,
            mods::Kind::Integer => put(&mut changes, store, key, &value.as_i64().unwrap())?,
            _ => anyhow::bail!("Unsupported lock screen setting"),
        }
    }
    store.commit(changes, "Lock screen controls")
}
pub fn import_image(source: &Path) -> Result<PathBuf> {
    import_into(
        source,
        &xdg("XDG_DATA_HOME", ".local/share").join("cosmic-lockscreen"),
    )
}
pub fn import_into(source: &Path, directory: &Path) -> Result<PathBuf> {
    ensure!(source.is_file(), "Choose a regular image file");
    let file = fs::File::open(source)?;
    let meta = file.metadata()?;
    ensure!(
        meta.is_file() && meta.len() <= 64 * 1024 * 1024,
        "Choose an image smaller than 64 MiB"
    );
    let mut reader = image::ImageReader::new(BufReader::new(file.take(64 * 1024 * 1024 + 1)))
        .with_guessed_format()?;
    ensure!(
        matches!(
            reader.format(),
            Some(image::ImageFormat::Png | image::ImageFormat::Jpeg | image::ImageFormat::WebP)
        ),
        "Choose a PNG, JPEG or WebP image"
    );
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(16384);
    limits.max_image_height = Some(16384);
    limits.max_alloc = Some(256 * 1024 * 1024);
    reader.limits(limits);
    let image = reader.decode().context("Could not read this image")?;
    ensure!(
        !directory.is_symlink(),
        "Preserved a replaced image directory"
    );
    fs::create_dir_all(directory)?;
    let mut file = tempfile::Builder::new()
        .prefix("wallpaper-")
        .suffix(".png")
        .tempfile_in(directory)?;
    image.write_to(file.as_file_mut(), image::ImageFormat::Png)?;
    ensure!(
        file.as_file().metadata()?.len() <= 64 * 1024 * 1024,
        "Imported image is too large; choose a smaller image"
    );
    file.as_file().sync_all()?;
    let (_, path) = file.keep().map_err(|e| e.error)?;
    Ok(path)
}
