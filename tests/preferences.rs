// SPDX-License-Identifier: GPL-3.0-only
use cosmic_forge::{
    catalog::{self, Effect},
    store::Store,
};
use serde_json::json;
use std::{collections::BTreeMap, fs, os::unix::fs::PermissionsExt};

struct Fixture {
    _temp: tempfile::TempDir,
    store: Store,
    effects: Vec<Effect>,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::new(temp.path().join("config"), temp.path().join("state"));
        let effects = catalog::load(&temp.path().join("extra")).unwrap();
        Self {
            _temp: temp,
            store,
            effects,
        }
    }
    fn write(&self, key: &str, value: &str) {
        fs::create_dir_all(&self.store.comp).unwrap();
        fs::write(self.store.comp.join(key), value).unwrap();
    }
    fn theme_paths(&self, key: &str) -> Vec<std::path::PathBuf> {
        ["Dark", "Light"]
            .into_iter()
            .flat_map(|mode| {
                ["", ".Builder"].map(move |suffix| {
                    self.store.root.join(format!(
                        "cosmic/com.system76.CosmicTheme.{mode}{suffix}/v2/{key}"
                    ))
                })
            })
            .collect()
    }
}
#[test]
fn opening_and_reading_is_read_only() {
    let f = Fixture::new();
    assert_eq!(f.store.selected(), "frosted");
    f.store.fingerprint(&f.effects);
    assert!(!f.store.root.exists());
    assert!(!f._temp.path().join("state").exists());
}
#[test]
fn selection_preserves_crystal_and_undo_restores_absence() {
    let mut f = Fixture::new();
    f.write("crystal_enabled", "true");
    f.write("crystal_thickness", "95");
    assert_eq!(f.store.selected(), "crystal");
    f.store.select_effect(&f.effects[2]).unwrap();
    assert_eq!(f.store.selected(), "aero");
    assert_eq!(
        Store::text(&f.store.comp.join("crystal_thickness"), ""),
        "95"
    );
    f.store.undo().unwrap();
    assert_eq!(f.store.selected(), "crystal");
    assert!(!f.store.comp.join("glass_effect").exists());
}
#[test]
fn controls_are_independent_and_bounded() {
    let mut f = Fixture::new();
    let c = f.effects[2].controls[0].clone();
    f.write("custom_setting", "Some(42)");
    f.store.set_control(&c, &json!(999)).unwrap();
    assert_eq!(f.store.value(&c), 100);
    f.store.set_control(&c, &json!(-3)).unwrap();
    assert_eq!(f.store.value(&c), 0);
    assert!(f.store.set_control(&c, &json!(true)).is_err());
    assert_eq!(
        Store::text(&f.store.comp.join("custom_setting"), ""),
        "Some(42)"
    );
    assert!(!f.store.comp.join("crystal_thickness").exists());
}
#[test]
fn legacy_blur_and_explicit_percentage() {
    let f = Fixture::new();
    let c = &f.effects[1].controls[1];
    f.write("crystal_blur", "7");
    assert_eq!(f.store.value(c), 54);
    f.write("crystal_blur_amount", "32");
    assert_eq!(f.store.value(c), 32);
}
#[test]
fn malformed_values_fall_back() {
    let f = Fixture::new();
    let c = &f.effects[2].controls[0];
    for value in ["true", "Some(broken)", "1.2", "null"] {
        f.write(&c.key, value);
        assert_eq!(f.store.value(c), c.default);
    }
}
#[test]
fn frosted_uses_ron_enums_and_is_undoable() {
    let mut f = Fixture::new();
    let c = f.effects[0].controls[0].clone();
    f.store.set_control(&c, &json!(13)).unwrap();
    for path in f.theme_paths("frosted") {
        assert_eq!(Store::text(&path, ""), "ExtremelyHigh2");
    }
    f.store.undo().unwrap();
    for path in f.theme_paths("frosted") {
        assert!(!path.exists());
    }
}
#[test]
fn reset_is_one_undo_and_preserves_other_effects_and_selection() {
    let mut f = Fixture::new();
    let e = f.effects[2].clone();
    f.write("crystal_thickness", "80");
    f.store.select_effect(&f.effects[2]).unwrap();
    for c in &e.controls {
        f.store.set_control(c, &json!(9)).unwrap();
    }
    f.store.reset(&e).unwrap();
    for c in &e.controls {
        assert_eq!(f.store.value(c), c.default);
    }
    f.store.undo().unwrap();
    for c in &e.controls {
        assert_eq!(f.store.value(c), 9);
    }
    assert_eq!(f.store.selected(), "aero");
    assert_eq!(
        Store::text(&f.store.comp.join("crystal_thickness"), ""),
        "80"
    );
}
#[test]
fn undo_refuses_external_edits() {
    let mut f = Fixture::new();
    f.store.select_effect(&f.effects[2]).unwrap();
    f.write("glass_effect", "\"crystal\"");
    assert!(f.store.undo().is_err());
    assert_eq!(f.store.selected(), "crystal");
}
#[test]
fn backup_keeps_original_bytes_and_file_permissions() {
    use base64::Engine;
    let mut f = Fixture::new();
    f.write("glass_effect", "\"frosted\"\n");
    let path = f.store.comp.join("glass_effect");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
    f.store.select_effect(&f.effects[2]).unwrap();
    f.store.select_effect(&f.effects[1]).unwrap();
    let backup: serde_json::Value =
        serde_json::from_slice(&fs::read(&f.store.backup).unwrap()).unwrap();
    let data = backup[path.to_str().unwrap()]["data"].as_str().unwrap();
    assert_eq!(
        base64::engine::general_purpose::STANDARD
            .decode(data)
            .unwrap(),
        b"\"frosted\"\n"
    );
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o640
    );
    f.store.undo().unwrap();
    f.store.undo().unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"\"frosted\"\n");
}
#[test]
fn incomplete_transaction_rolls_back() {
    let mut f = Fixture::new();
    f.write("a_good", "original");
    // Backup creation makes this formerly absent path a directory, causing the
    // second atomic replacement to fail after the first replacement succeeded.
    f.store.backup = f.store.comp.join("z_blocker/backup.json");
    let changes = BTreeMap::from([
        (f.store.comp.join("a_good"), Some(b"changed".to_vec())),
        (f.store.comp.join("z_blocker"), Some(b"failure".to_vec())),
    ]);
    assert!(f.store.commit(changes).is_err());
    assert_eq!(Store::text(&f.store.comp.join("a_good"), ""), "original");
    assert!(!f.store.undo().unwrap());
}
#[test]
fn no_op_does_not_create_history() {
    let mut f = Fixture::new();
    f.write("glass_effect", "\"aero\"\n");
    assert!(!f.store.select_effect(&f.effects[2]).unwrap());
    assert!(!f.store.undo().unwrap());
    assert!(!f.store.backup.exists());
}
#[test]
fn future_catalog_accepts_object_and_rejects_duplicate_and_path_keys() {
    let f = Fixture::new();
    let folder = f._temp.path().join("extra");
    fs::create_dir_all(&folder).unwrap();
    let mut effect = f.effects[2].clone();
    effect.id = "prism".into();
    let path = folder.join("prism.json");
    fs::write(&path, serde_json::to_vec(&effect).unwrap()).unwrap();
    assert_eq!(catalog::load(&folder).unwrap().len(), 4);
    effect.id = "aero".into();
    fs::write(&path, serde_json::to_vec(&effect).unwrap()).unwrap();
    assert!(catalog::load(&folder).is_err());
    effect.id = "prism".into();
    effect.controls[0].key = "../outside".into();
    fs::write(&path, serde_json::to_vec(&effect).unwrap()).unwrap();
    assert!(catalog::load(&folder).is_err());
}
#[test]
fn fingerprints_follow_external_light_theme_changes() {
    let f = Fixture::new();
    let before = f.store.fingerprint(&f.effects);
    let mode = f
        .store
        .root
        .join("cosmic/com.system76.CosmicTheme.Mode/v1/is_dark");
    fs::create_dir_all(mode.parent().unwrap()).unwrap();
    fs::write(mode, "false").unwrap();
    let accent = f.store.theme_path("accent");
    fs::create_dir_all(accent.parent().unwrap()).unwrap();
    fs::write(accent, "(base: \"#aabbccff\")").unwrap();
    let after = f.store.fingerprint(&f.effects);
    assert_ne!(before, after);
    assert!(!after.dark);
    assert_eq!(after.accent, "#aabbcc");
}
#[test]
fn booleans_and_identifier_validation() {
    let mut f = Fixture::new();
    let c = f.effects[1].controls[0].clone();
    f.store.set_control(&c, &json!(false)).unwrap();
    assert_eq!(f.store.value(&c), false);
    assert!(f.store.set_control(&c, &json!(1)).is_err());
    let mut invalid = f.effects[2].clone();
    invalid.id = "../../escape".into();
    assert!(f.store.select_effect(&invalid).is_err());
    let mut c = f.effects[0].controls[0].clone();
    c.kind = catalog::ControlKind::Boolean;
    c.default = json!(false);
    assert!(c.validate().is_err());
}

#[test]
fn opacity_is_shared_across_effects_and_updates_native_glass_surfaces() {
    let mut f = Fixture::new();
    let control = f.effects[2].controls.last().unwrap().clone();
    f.write("crystal_thickness", "88");
    let accent = f.store.theme_path("accent");
    fs::create_dir_all(accent.parent().unwrap()).unwrap();
    fs::write(&accent, "(base: \"#123456FF\")").unwrap();
    f.store.set_control(&control, &json!(25)).unwrap();
    for effect in &f.effects {
        assert_eq!(f.store.value(effect.controls.last().unwrap()), 25);
    }
    for path in f.theme_paths("alpha_map") {
        let map: cosmic_theme::AlphaMap =
            ron::from_str(&fs::read_to_string(path).unwrap()).unwrap();
        assert!((map.guess_offset() - 0.25).abs() < 0.00001);
    }
    for mode in ["Dark", "Light"] {
        let theme = f
            .store
            .root
            .join(format!("cosmic/com.system76.CosmicTheme.{mode}/v2"));
        for name in [
            "transparent_background",
            "transparent_primary",
            "transparent_secondary",
        ] {
            let surface: cosmic_theme::Container =
                ron::from_str(&fs::read_to_string(theme.join(name)).unwrap()).unwrap();
            assert!(surface.base.alpha.is_finite());
        }
    }
    assert_eq!(Store::text(&accent, ""), "(base: \"#123456FF\")");
    assert_eq!(
        Store::text(&f.store.comp.join("crystal_thickness"), ""),
        "88"
    );
    assert!(!f.store.comp.join("alpha_map").exists());
}

#[test]
fn opacity_undo_restores_original_maps_and_removes_new_derived_files() {
    let mut f = Fixture::new();
    let control = f.effects[2].controls.last().unwrap().clone();
    let path = f.store.theme_path("alpha_map");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let original = ron::ser::to_string(&cosmic_theme::AlphaMap::default()).unwrap();
    fs::write(&path, &original).unwrap();
    f.store.set_control(&control, &json!(10)).unwrap();
    f.store.undo().unwrap();
    assert_eq!(fs::read_to_string(path).unwrap(), original);
    for mode in ["Dark", "Light"] {
        let dir = f
            .store
            .root
            .join(format!("cosmic/com.system76.CosmicTheme.{mode}/v2"));
        assert!(!dir.join("transparent_background").exists());
    }
}

#[test]
fn grouped_thickness_and_opacity_keep_derived_colors_consistent() {
    let mut f = Fixture::new();
    let thickness = f.effects[0].controls[0].clone();
    let opacity = f.effects[0].controls[1].clone();
    let (strength, amount) = (json!(13), json!(80));
    let changes = f
        .store
        .controls_changes([(&opacity, &amount), (&thickness, &strength)])
        .unwrap();
    f.store.commit(changes).unwrap();
    for mode in ["Dark", "Light"] {
        let dir = f
            .store
            .root
            .join(format!("cosmic/com.system76.CosmicTheme.{mode}/v2"));
        let map: cosmic_theme::AlphaMap =
            ron::from_str(&fs::read_to_string(dir.join("alpha_map")).unwrap()).unwrap();
        let surface: cosmic_theme::Container =
            ron::from_str(&fs::read_to_string(dir.join("transparent_background")).unwrap())
                .unwrap();
        assert!(
            (surface.base.alpha
                - (map.blurred_alpha(cosmic_theme::BlurStrength::ExtremelyHigh2) * 255.).round()
                    / 255.)
                .abs()
                < 0.00001
        );
    }
    f.store.reset(&f.effects[0].clone()).unwrap();
    assert_eq!(f.store.value(&thickness), 6);
    assert_eq!(f.store.value(&opacity), 50);
    let surface: cosmic_theme::Container =
        ron::from_str(&fs::read_to_string(f.store.theme_path("transparent_background")).unwrap())
            .unwrap();
    assert!(
        (surface.base.alpha - (cosmic_theme::AlphaMap::default().medium * 255.).round() / 255.)
            .abs()
            < 0.00001
    );
}

#[test]
fn opacity_scale_matches_settings_at_ends_and_center() {
    let mut f = Fixture::new();
    let control = f.effects[2].controls.last().unwrap().clone();
    for amount in [0, 50, 100] {
        f.store.set_control(&control, &json!(amount)).unwrap();
        assert_eq!(f.store.value(&control), amount);
        let map: cosmic_theme::AlphaMap =
            ron::from_str(&fs::read_to_string(f.store.theme_path("alpha_map")).unwrap()).unwrap();
        let default = cosmic_theme::AlphaMap::default();
        let offset = match amount {
            0 => -default.extremely_high_2,
            100 => 1. - default.extremely_low,
            _ => 0.,
        };
        assert!((map.medium - (default.medium + offset)).abs() < 0.00001);
    }
}

#[test]
fn malformed_builder_does_not_reset_glass_colors_to_defaults() {
    let mut f = Fixture::new();
    let control = f.effects[2].controls.last().unwrap().clone();
    let builder = f
        .store
        .root
        .join("cosmic/com.system76.CosmicTheme.Light.Builder/v2");
    fs::create_dir_all(&builder).unwrap();
    fs::write(builder.join("palette"), "NotAValidPalette(").unwrap();
    assert!(f.store.set_control(&control, &json!(40)).is_err());
    assert!(!f.store.theme_path("alpha_map").exists());
    assert!(!f.store.theme_path("transparent_background").exists());
    assert!(!f.store.undo().unwrap());
}
