// SPDX-License-Identifier: GPL-3.0-only
//! Use COSMIC's own alpha scale and theme builder for derived glass colors.
use crate::store::{BLUR_NAMES, Changes, Store};
use anyhow::{Context, Result};
use cosmic_config::{Config, ConfigGet, CosmicConfigEntry};
use cosmic_theme::{AlphaMap, BlurStrength, CosmicPalette, ThemeBuilder};
use serde::Serialize;
use std::{fs, path::Path};

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

fn read_builder(
    name: &str,
    builder_root: &Path,
    system: &Config,
    light: bool,
) -> Result<ThemeBuilder> {
    // Keep the valid system defaults even if the distribution ships an older schema.
    let mut builder = ThemeBuilder::get_entry(system).unwrap_or_else(|(_, builder)| builder);
    if light && system.get::<CosmicPalette>("palette").is_err() {
        builder.palette = ThemeBuilder::light().palette;
    }

    // The snapshot lets COSMIC resolve v2 over v1 without writing to the user's config.
    let version = ThemeBuilder::VERSION;
    let temporary = tempfile::tempdir()?;
    let user = Config::with_custom_path(name, version, temporary.path().to_owned())?;
    let snapshot = temporary.path().join("cosmic").join(name);
    let mut keys = Vec::new();
    for version in [version - 1, version] {
        let schema = format!("v{version}");
        let source = builder_root.join(&schema);
        if !source.is_dir() {
            continue;
        }
        let snapshot_dir = snapshot.join(schema);
        for entry in fs::read_dir(source)? {
            let entry = entry?;
            if entry.path().is_file()
                && let Ok(key) = entry.file_name().into_string()
            {
                fs::copy(entry.path(), snapshot_dir.join(&key))?;
                keys.push(key);
            }
        }
    }
    keys.sort_unstable();
    keys.dedup();
    let (errors, _) = builder.update_keys(&user, &keys);
    if !errors.is_empty() {
        anyhow::bail!(
            "Could not read {name}: {}",
            errors
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("; ")
        );
    }
    Ok(builder)
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
        let name = format!("com.system76.CosmicTheme.{mode}.Builder");
        let builder_root = store.root.join("cosmic").join(&name);
        let version = ThemeBuilder::VERSION;
        let builder_dir = builder_root.join(format!("v{version}"));
        let system = Config::system(&name, version)?;
        let mut builder = read_builder(&name, &builder_root, &system, mode == "Light")?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_theme_keys_override_system_defaults_in_version_order() {
        let root = tempfile::tempdir().unwrap();
        let name = "com.system76.CosmicTheme.Light.Builder";
        let system = Config::with_custom_path(name, 2, root.path().join("system")).unwrap();
        let mut defaults = ThemeBuilder::light();
        defaults.bg_color = Some((0.2, 0.3, 0.4, 1.0).into());
        defaults.primary_container_bg = defaults.bg_color;
        defaults.secondary_container_bg = defaults.bg_color;
        defaults.write_entry(&system).unwrap();

        let user = root.path().join("user");
        let legacy = Config::with_custom_path(name, 1, user.clone()).unwrap();
        let current = Config::with_custom_path(name, 2, user.clone()).unwrap();
        let mut overrides = ThemeBuilder::light();
        let purple = Some((0.5, 0.4, 0.7, 1.0).into());
        let green = Some((0.1, 0.7, 0.4, 1.0).into());
        overrides.set_bg_color(&legacy, purple).unwrap();
        overrides.set_primary_container_bg(&legacy, green).unwrap();
        overrides.set_bg_color(&current, None).unwrap();

        let builder = read_builder(name, &user.join("cosmic").join(name), &system, true).unwrap();
        assert_eq!(builder.bg_color, None);
        assert_eq!(builder.primary_container_bg, green);
        let defaults = ThemeBuilder::get_entry(&system).unwrap();
        assert_eq!(
            builder.secondary_container_bg,
            defaults.secondary_container_bg
        );
    }
}
