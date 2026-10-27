// SPDX-License-Identifier: GPL-3.0-only
use cosmic_forge::{
    mods::{self, ASSET, REPOSITORY},
    panel,
    store::Store,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::fs;
fn builtin() -> Value {
    serde_json::from_slice(include_bytes!("../mods/catalog.json")).unwrap()
}
fn release(bytes: &[u8]) -> Vec<u8> {
    let hash: String = Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    serde_json::to_vec(&json!({"assets":[{"name":ASSET,"size":bytes.len(),"digest":format!("sha256:{hash}"),"browser_download_url":format!("https://github.com/{REPOSITORY}/releases/download/mods-v2/{ASSET}")}]})).unwrap()
}
#[test]
fn new_mod_controls_and_presets_load_without_app_changes() {
    let temp = tempfile::tempdir().unwrap();
    let mut c = builtin();
    c["revision"] = json!(c["revision"].as_u64().unwrap() + 1);
    c["mods"].as_array_mut().unwrap().push(json!({"id":"custom-panel","section":"panel","name":"Custom panel","description":"New preset","engine_revision":1,"controls":[{"key":"penumbra_enabled","kind":"boolean","label":"Gradient"},{"key":"gradient_height","kind":"integer","label":"Extent","min":16,"max":512},{"key":"forge_new_amount","kind":"integer","label":"New engine control","min":0,"max":100,"default":30}],"preset":{"penumbra_enabled":true,"gradient_height":128,"forge_new_amount":75}}));
    let bytes = serde_json::to_vec(&c).unwrap();
    mods::update_from_release(temp.path(), &release(&bytes), |_| Ok(bytes.clone())).unwrap();
    let catalog = mods::load(temp.path()).unwrap();
    assert_eq!(catalog.mods.len(), 3);
    let m = &catalog.mods[2];
    let mut store = Store::new(temp.path().join("config"), temp.path().join("state"));
    panel::apply_controls(&mut store, "Panel", &m.controls, &m.preset).unwrap();
    assert_eq!(
        panel::load(&store, "Panel")
            .unwrap()
            .penumbra
            .unwrap()
            .height,
        128
    );
    assert_eq!(
        panel::control_value(&store, "Panel", &m.controls[2]).unwrap(),
        json!(75)
    );
}
#[test]
fn catalog_updates_are_verified_atomic_and_never_write_preferences() {
    let temp = tempfile::tempdir().unwrap();
    let mut c = builtin();
    c["revision"] = json!(c["revision"].as_u64().unwrap() + 1);
    c["mods"][0]["name"] = json!("Updated title");
    let bytes = serde_json::to_vec(&c).unwrap();
    mods::update_from_release(temp.path(), &release(&bytes), |_| Ok(bytes.clone())).unwrap();
    let before = fs::read(temp.path().join(ASSET)).unwrap();
    let mut bad = release(&bytes);
    let mut metadata: Value = serde_json::from_slice(&bad).unwrap();
    metadata["assets"][0]["browser_download_url"] = json!("https://example.com/code");
    bad = serde_json::to_vec(&metadata).unwrap();
    assert!(mods::update_from_release(temp.path(), &bad, |_| panic!("must not download")).is_err());
    assert!(
        mods::update_from_release(temp.path(), &release(&bytes), |_| Ok(vec![0; bytes.len()]))
            .is_err()
    );
    let old = serde_json::to_vec(&builtin()).unwrap();
    assert!(mods::update_from_release(temp.path(), &release(&old), |_| Ok(old.clone())).is_err());
    assert_eq!(fs::read(temp.path().join(ASSET)).unwrap(), before);
    assert!(!temp.path().join("config").exists());
}
#[test]
fn unsafe_paths_types_and_unsupported_sections_are_rejected() {
    for (key, value) in [
        ("key", json!("../../settings")),
        ("kind", json!("shell")),
        ("max", json!(99999)),
    ] {
        let mut c = builtin();
        c["mods"][0]["controls"][1][key] = value;
        assert!(mods::parse(&serde_json::to_vec(&c).unwrap()).is_err());
    }
    let mut c = builtin();
    c["mods"][0]["section"] = json!("lock-screen");
    assert!(mods::parse(&serde_json::to_vec(&c).unwrap()).is_err());
    let mut c = builtin();
    c["mods"][0]["preset"] = json!({"panel_opacity":300});
    assert!(mods::parse(&serde_json::to_vec(&c).unwrap()).is_err());
}
