// SPDX-License-Identifier: GPL-3.0-only
use crate::store::{Changes, Store, xdg};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::ErrorKind,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Colors {
    pub light: Option<[f32; 3]>,
    pub dark: Option<[f32; 3]>,
}
impl Colors {
    pub fn get(self, dark: bool) -> Option<[f32; 3]> {
        if dark { self.dark } else { self.light }
    }
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Penumbra {
    pub enabled: bool,
    pub height: u32,
    pub opacity: f32,
    pub maximized_color: [u8; 3],
    pub maximized_opacity: f32,
}
impl Default for Penumbra {
    fn default() -> Self {
        Self {
            enabled: true,
            height: 96,
            opacity: 0.4,
            maximized_color: [36, 38, 42],
            maximized_opacity: 1.0,
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct Profile {
    pub name: String,
    pub colors: Option<Colors>,
    pub penumbra: Option<Penumbra>,
    pub opacity: f32,
    pub gap: bool,
}
fn read<T: DeserializeOwned>(path: &Path, default: T) -> Result<T> {
    match fs::read_to_string(path) {
        Ok(data) => {
            ron::from_str(&data).with_context(|| format!("Could not read {}", path.display()))
        }
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(default),
        Err(e) => Err(e.into()),
    }
}
fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && name != "."
        && name != ".."
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._- ".contains(&b))
}
fn rgb_valid(c: &[f32; 3]) -> bool {
    c.iter().all(|v| v.is_finite() && (0.0..=1.0).contains(v))
}
pub fn profiles(store: &Store) -> Result<Vec<String>> {
    let names: Vec<String> = read(
        &store
            .root
            .join("cosmic/com.system76.CosmicPanel/v1/entries"),
        vec!["Panel".into(), "Dock".into()],
    )?;
    ensure!(
        names.len() <= 64 && names.iter().all(|n| valid_name(n)),
        "Invalid COSMIC panel profile list"
    );
    Ok(names)
}
fn directory(store: &Store, name: &str) -> Result<PathBuf> {
    ensure!(
        valid_name(name) && profiles(store)?.iter().any(|n| n == name),
        "Select an existing COSMIC panel profile"
    );
    let path = store
        .root
        .join(format!("cosmic/com.system76.CosmicPanel.{name}/v1"));
    ensure!(
        !path.is_symlink() && !path.parent().unwrap().is_symlink(),
        "Preserved a replaced panel profile"
    );
    Ok(path)
}
pub fn load(store: &Store, name: &str) -> Result<Profile> {
    let dir = directory(store, name)?;
    let colors: Option<Colors> = read(&dir.join("forge_colors"), None)?;
    if let Some(c) = colors {
        ensure!(
            c.light.as_ref().is_none_or(rgb_valid) && c.dark.as_ref().is_none_or(rgb_valid),
            "Invalid panel RGB color"
        );
    }
    let penumbra: Option<Penumbra> = read(&dir.join("top_edge_gradient"), None)?;
    let opacity: f32 = read(
        &dir.join("opacity"),
        if store
            .root
            .join("cosmic/com.system76.CosmicPanel/v1/entries")
            .exists()
        {
            0.8
        } else {
            1.0
        },
    )?;
    ensure!(
        opacity.is_finite() && (0.0..=1.0).contains(&opacity),
        "Invalid panel opacity"
    );
    if let Some(p) = &penumbra {
        ensure!(
            (16..=512).contains(&p.height)
                && p.maximized_opacity.is_finite()
                && (0.0..=1.0).contains(&p.maximized_opacity),
            "Invalid Penumbra preferences"
        );
    }
    Ok(Profile {
        name: name.into(),
        colors,
        penumbra,
        opacity,
        gap: read(&dir.join("anchor_gap"), false)?,
    })
}
fn put(changes: &mut Changes, dir: &Path, key: &str, value: &impl Serialize) -> Result<()> {
    let path = dir.join(key);
    ensure!(!path.is_symlink(), "Preserved a replaced panel preference");
    changes.insert(
        path,
        Some(format!("{}\n", ron::to_string(value)?).into_bytes()),
    );
    Ok(())
}
pub fn set_color(
    store: &mut Store,
    name: &str,
    dark: bool,
    color: Option<[f32; 3]>,
) -> Result<bool> {
    ensure!(
        color.as_ref().is_none_or(rgb_valid),
        "RGB components must be between 0 and 1"
    );
    let mut colors = load(store, name)?.colors.unwrap_or_default();
    *if dark {
        &mut colors.dark
    } else {
        &mut colors.light
    } = color;
    let mut changes = Changes::new();
    put(
        &mut changes,
        &directory(store, name)?,
        "forge_colors",
        &Some(colors),
    )?;
    store.commit(changes, "Panel color")
}
pub fn theme_color(store: &Store, dark: bool) -> [f32; 3] {
    let mode = if dark { "Dark" } else { "Light" };
    let file = store.root.join(format!(
        "cosmic/com.system76.CosmicTheme.{mode}/v2/background"
    ));
    let color = fs::read_to_string(file)
        .ok()
        .and_then(|s| ron::from_str::<cosmic_theme::Container>(&s).ok())
        .map(|c| c.base)
        .unwrap_or_else(|| {
            if dark {
                cosmic_theme::Theme::dark_default()
            } else {
                cosmic_theme::Theme::light_default()
            }
            .bg_color()
        });
    [color.red, color.green, color.blue]
}
pub fn installed_revision() -> Option<u64> {
    let check = || -> Option<u64> {
        let data = xdg("XDG_DATA_HOME", ".local/share").join("cosmic-forge/panel/versions");
        let launcher = PathBuf::from(std::env::var_os("HOME")?).join(".local/bin/cosmic-panel");
        let record: serde_json::Value = serde_json::from_slice(
            &fs::read(
                xdg("XDG_STATE_HOME", ".local/state").join("cosmic-forge/panel-installation.json"),
            )
            .ok()?,
        )
        .ok()?;
        if record["abi"] != 1
            || record["revision"].as_u64()? < 1
            || record["launcher"].as_str()? != launcher.to_str()?
            || launcher.is_symlink()
        {
            return None;
        }
        let destination = PathBuf::from(record["destination"].as_str()?);
        if destination.parent()? != data || destination.file_name()?.len() != 16 {
            return None;
        }
        let binary = destination.join("cosmic-panel");
        if destination.is_symlink()
            || binary.is_symlink()
            || fs::metadata(&binary).ok()?.permissions().mode() & 0o111 == 0
        {
            return None;
        }
        let manager = record["package_manager"].as_str()?;
        let args: &[&str] = match manager {
            "pacman" => &["-Q", "cosmic-panel"],
            "rpm" => &[
                "-q",
                "--qf",
                "%{NAME} %{EPOCHNUM}:%{VERSION}-%{RELEASE}",
                "cosmic-panel",
            ],
            "dpkg-query" => &[
                "-W",
                "-f=${Package} ${Version} ${db:Status-Status}",
                "cosmic-panel",
            ],
            _ => return None,
        };
        let output = Command::new(manager).args(args).output().ok()?;
        if !output.status.success()
            || String::from_utf8(output.stdout).ok()?.trim()
                != record["packages"]["cosmic-panel"].as_str()?
        {
            return None;
        }
        let hash = |p: &Path| {
            fs::read(p).ok().map(|b| {
                Sha256::digest(b)
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>()
            })
        };
        (hash(&launcher)?.as_str() == record["launcher_sha256"].as_str()?
            && hash(&binary)?.as_str() == record["binary_sha256"].as_str()?)
        .then_some(record["revision"].as_u64()?)
    };
    check()
}

pub fn control_value(
    store: &Store,
    name: &str,
    control: &crate::mods::Control,
) -> Result<serde_json::Value> {
    use serde_json::json;
    let p = load(store, name)?;
    let g = p.penumbra.clone().unwrap_or_default();
    let value = match control.key.as_str() {
        "penumbra_enabled" => json!(p.penumbra.is_some_and(|p| p.enabled)),
        "gradient_height" => json!(g.height),
        "panel_opacity" => json!((p.opacity * 100.0).round() as i32),
        "maximized_opacity" => json!((g.maximized_opacity * 100.0).round() as i32),
        "forge_colors" => serde_json::Value::Null,
        key => read(
            &directory(store, name)?.join(key),
            if control.default.is_null() {
                if control.kind == crate::mods::Kind::Boolean {
                    json!(false)
                } else {
                    json!(control.min)
                }
            } else {
                control.default.clone()
            },
        )?,
    };
    match control.kind {
        crate::mods::Kind::Boolean => ensure!(
            value.is_boolean(),
            "Invalid panel mod setting: {}",
            control.key
        ),
        crate::mods::Kind::Integer => {
            let number = value
                .as_i64()
                .with_context(|| format!("Invalid panel mod setting: {}", control.key))?;
            return Ok(json!(
                number.clamp(i64::from(control.min), i64::from(control.max))
            ));
        }
        crate::mods::Kind::ThemeColor => {}
    }
    Ok(value)
}
pub fn apply_controls(
    store: &mut Store,
    name: &str,
    controls: &[crate::mods::Control],
    values: &std::collections::BTreeMap<String, serde_json::Value>,
) -> Result<bool> {
    let profile = load(store, name)?;
    let dir = directory(store, name)?;
    let mut gradient = profile.penumbra.unwrap_or_else(|| Penumbra {
        enabled: false,
        ..Penumbra::default()
    });
    let mut changed_gradient = false;
    let mut changes = Changes::new();
    for (key, value) in values {
        let control = controls
            .iter()
            .find(|c| c.key == *key)
            .context("Unknown panel mod control")?;
        ensure!(control.validate_value(value), "Invalid panel mod value");
        control_value(store, name, control)?;
        match key.as_str() {
            "penumbra_enabled" => {
                gradient.enabled = value.as_bool().unwrap();
                changed_gradient = true;
                if gradient.enabled && profile.gap {
                    put(&mut changes, &dir, "anchor_gap", &false)?;
                }
            }
            "gradient_height" => {
                gradient.height = value.as_u64().unwrap() as u32;
                changed_gradient = true;
            }
            "panel_opacity" => put(
                &mut changes,
                &dir,
                "opacity",
                &(value.as_f64().unwrap() as f32 / 100.0),
            )?,
            "maximized_opacity" => {
                gradient.maximized_opacity = value.as_f64().unwrap() as f32 / 100.0;
                changed_gradient = true;
            }
            key => {
                ensure!(
                    key.starts_with("forge_")
                        && crate::catalog::valid_identifier(key)
                        && key != "forge_colors",
                    "Unsupported panel mod key"
                );
                put(&mut changes, &dir, key, value)?;
            }
        }
    }
    if changed_gradient {
        put(&mut changes, &dir, "top_edge_gradient", &Some(gradient))?;
    }
    store.commit(changes, "Panel mod")
}
