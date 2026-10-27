// SPDX-License-Identifier: GPL-3.0-only
use cosmic_forge::{panel, store::Store};
use std::fs;

fn fixture() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().join("config"), dir.path().join("state"));
    (dir, store)
}
fn adjust(store: &mut Store, name: &str, key: &str, value: serde_json::Value) {
    let catalog = cosmic_forge::mods::load(&store.root.join("unused-mod-cache")).unwrap();
    panel::apply_controls(
        store,
        name,
        &catalog.mods[0].controls,
        &std::collections::BTreeMap::from([(key.into(), value)]),
    )
    .unwrap();
}
#[test]
fn colors_preserve_other_mode_profiles_and_native_background() {
    let (_tmp, mut store) = fixture();
    let path = store.root.join("cosmic/com.system76.CosmicPanel.Panel/v1");
    fs::create_dir_all(&path).unwrap();
    fs::write(path.join("background"), "Color((0.1,0.2,0.3))").unwrap();
    panel::set_color(&mut store, "Panel", false, Some([1., 0., 0.])).unwrap();
    panel::set_color(&mut store, "Panel", true, Some([0., 0., 1.])).unwrap();
    panel::set_color(&mut store, "Dock", false, Some([0., 1., 0.])).unwrap();
    panel::set_color(&mut store, "Panel", false, None).unwrap();
    let colors = panel::load(&store, "Panel").unwrap().colors.unwrap();
    assert_eq!(colors.light, None);
    assert_eq!(colors.dark, Some([0., 0., 1.]));
    assert_eq!(
        panel::load(&store, "Dock").unwrap().colors.unwrap().light,
        Some([0., 1., 0.])
    );
    assert_eq!(
        fs::read_to_string(path.join("background")).unwrap(),
        "Color((0.1,0.2,0.3))"
    );
    assert!(store.backup.exists());
}
#[test]
fn penumbra_and_colors_are_independent() {
    let (_tmp, mut store) = fixture();
    panel::set_color(&mut store, "Dock", true, Some([0.2, 0.4, 0.8])).unwrap();
    adjust(
        &mut store,
        "Dock",
        "penumbra_enabled",
        serde_json::json!(true),
    );
    adjust(
        &mut store,
        "Dock",
        "gradient_height",
        serde_json::json!(144),
    );
    adjust(&mut store, "Dock", "panel_opacity", serde_json::json!(65));
    adjust(
        &mut store,
        "Dock",
        "penumbra_enabled",
        serde_json::json!(false),
    );
    let profile = panel::load(&store, "Dock").unwrap();
    assert!(!profile.penumbra.as_ref().unwrap().enabled);
    assert_eq!(profile.penumbra.unwrap().height, 144);
    assert_eq!(profile.opacity, 0.65);
    assert!(!profile.gap);
    assert_eq!(profile.colors.unwrap().dark, Some([0.2, 0.4, 0.8]));
}
#[test]
fn invalid_configs_and_colors_are_preserved() {
    let (_tmp, mut store) = fixture();
    for name in ["../bad", ".", "does-not-exist"] {
        assert!(panel::set_color(&mut store, name, false, None).is_err());
    }
    assert!(panel::set_color(&mut store, "Panel", false, Some([f32::NAN, 0., 0.])).is_err());
    assert!(panel::set_color(&mut store, "Panel", false, Some([2., 0., 0.])).is_err());
    let dir = store.root.join("cosmic/com.system76.CosmicPanel.Panel/v1");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("forge_colors"), "user malformed config").unwrap();
    assert!(panel::set_color(&mut store, "Panel", false, None).is_err());
    assert_eq!(
        fs::read_to_string(dir.join("forge_colors")).unwrap(),
        "user malformed config"
    );
}

#[test]
fn adjusting_a_disabled_gradient_does_not_enable_it() {
    let (_tmp, mut store) = fixture();
    adjust(
        &mut store,
        "Panel",
        "gradient_height",
        serde_json::json!(144),
    );
    let gradient = panel::load(&store, "Panel").unwrap().penumbra.unwrap();
    assert_eq!(gradient.height, 144);
    assert!(!gradient.enabled);
}
#[test]
fn narrower_catalog_ranges_can_be_adjusted_without_rewriting_on_read() {
    let (_tmp, mut store) = fixture();
    let mut catalog = cosmic_forge::mods::load(&store.root.join("unused")).unwrap();
    let control = &mut catalog.mods[0].controls[1];
    control.min = 128;
    control.max = 200;
    assert_eq!(
        panel::control_value(&store, "Panel", control).unwrap(),
        serde_json::json!(128)
    );
    assert!(panel::load(&store, "Panel").unwrap().penumbra.is_none());
    panel::apply_controls(
        &mut store,
        "Panel",
        std::slice::from_ref(control),
        &std::collections::BTreeMap::from([("gradient_height".into(), serde_json::json!(129))]),
    )
    .unwrap();
    assert_eq!(
        panel::load(&store, "Panel")
            .unwrap()
            .penumbra
            .unwrap()
            .height,
        129
    );
}
