// SPDX-License-Identifier: GPL-3.0-only
//! Use COSMIC's own alpha scale and theme builder for derived glass colors.
use crate::store::{BLUR_NAMES, Changes, Store};
use anyhow::{Context, Result};
use cosmic_config::{Config, CosmicConfigEntry};
use cosmic_theme::{AlphaMap, BlurStrength, ThemeBuilder};
use serde::Serialize;

fn encode(value: &impl Serialize) -> Result<Option<Vec<u8>>> {
    Ok(Some(
        format!(
            "{}\n",
            ron::ser::to_string_pretty(value, ron::ser::PrettyConfig::default())?
        )
        .into_bytes(),
    ))
}

pub fn opacity(store: &Store) -> i32 {
    let map: AlphaMap =
        ron::from_str(&Store::text(&store.theme_path("alpha_map"), "")).unwrap_or_default();
    (map.guess_offset().clamp(0., 1.) * 100.).round() as i32
}

pub fn strength(store: &Store) -> i32 {
    let name = Store::text(&store.theme_path("frosted"), "Medium");
    BLUR_NAMES.iter().position(|v| *v == name).unwrap_or(6) as i32
}

pub fn changes(store: &Store, thickness: Option<i32>, opacity: Option<i32>) -> Result<Changes> {
    let map = opacity.map(|value| {
        let default = AlphaMap::default();
        let amount = value.clamp(0, 100) as f32 / 100.;
        let scale = if amount < 0.5 {
            default.extremely_high_2
        } else {
            1. - default.extremely_low
        };
        default.offset((amount - 0.5) / 0.5 * scale)
    });
    let mut changes = Changes::new();
    for mode in ["Dark", "Light"] {
        let theme_dir = store
            .root
            .join(format!("cosmic/com.system76.CosmicTheme.{mode}/v2"));
        let builder_dir = store
            .root
            .join(format!("cosmic/com.system76.CosmicTheme.{mode}.Builder/v2"));
        // Read through a private snapshot, avoiding directory creation or config
        // migration in the user's tree while preparing an atomic transaction.
        let temporary = tempfile::tempdir()?;
        let name = format!("com.system76.CosmicTheme.{mode}.Builder");
        let snapshot_dir = temporary.path().join("cosmic").join(&name).join("v2");
        std::fs::create_dir_all(&snapshot_dir)?;
        if builder_dir.is_dir() {
            for entry in std::fs::read_dir(&builder_dir)? {
                let path = entry?.path();
                if path.is_file() {
                    std::fs::copy(&path, snapshot_dir.join(path.file_name().unwrap()))?;
                }
            }
        }
        let helper = Config::with_custom_path(&name, 2, temporary.path().to_owned())?;
        let mut builder = ThemeBuilder::get_entry(&helper).map_err(|(errors, _)| {
            anyhow::anyhow!(
                "Could not read the {mode} theme builder: {}",
                errors
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("; ")
            )
        })?;
        if !builder_dir.join("palette").exists() && mode == "Light" {
            builder.palette = ThemeBuilder::light().palette;
        }
        let read = |key: &str| Store::text(&theme_dir.join(key), "");
        builder.frosted = if let Some(value) = thickness {
            BlurStrength::try_from(value.clamp(0, 13) as u8).unwrap()
        } else {
            ron::from_str(&read("frosted")).unwrap_or(builder.frosted)
        };
        builder.alpha_map =
            map.unwrap_or_else(|| ron::from_str(&read("alpha_map")).unwrap_or(builder.alpha_map));
        for (key, field) in [
            ("frosted_windows", &mut builder.frosted_windows),
            ("frosted_panel", &mut builder.frosted_panel),
            ("frosted_applets", &mut builder.frosted_applets),
            (
                "frosted_system_interface",
                &mut builder.frosted_system_interface,
            ),
        ] {
            if let Ok(value) = ron::from_str(&read(key)) {
                *field = value;
            }
        }
        if thickness.is_some() {
            for dir in [&theme_dir, &builder_dir] {
                changes.insert(dir.join("frosted"), encode(&builder.frosted)?);
            }
        }
        if map.is_some() {
            for dir in [&theme_dir, &builder_dir] {
                changes.insert(dir.join("alpha_map"), encode(&builder.alpha_map)?);
            }
        }
        let theme = builder.build();
        // Update the same derived glass surfaces that COSMIC Settings rebuilds.
        // Palette, opaque surfaces and other effect settings remain untouched.
        for (key, value) in [
            ("transparent_background", theme.background(true)),
            ("transparent_primary", theme.primary(true)),
            ("transparent_secondary", theme.secondary(true)),
        ] {
            changes.insert(
                theme_dir.join(key),
                encode(value).context("Could not serialize a glass surface")?,
            );
        }
    }
    Ok(changes)
}
