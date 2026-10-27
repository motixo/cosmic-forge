// SPDX-License-Identifier: GPL-3.0-only
use cosmic_forge::{lockscreen, mods, store::Store};
use serde_json::json;
use std::{collections::BTreeMap, fs};

#[test]
fn wallpaper_and_media_are_independent_and_preserve_other_preferences() {
    let tmp = tempfile::tempdir().unwrap();
    let mut store = Store::new(tmp.path().join("config"), tmp.path().join("state"));
    let desktop = store
        .root
        .join("cosmic/com.system76.CosmicBackground/v1/all");
    fs::create_dir_all(desktop.parent().unwrap()).unwrap();
    fs::write(&desktop, "original desktop").unwrap();
    assert_eq!(
        lockscreen::load(&store).unwrap(),
        lockscreen::Config::default()
    );
    let image = tmp.path().join("تصویر.png");
    image::RgbaImage::from_pixel(4, 3, image::Rgba([1, 2, 3, 255]))
        .save(&image)
        .unwrap();
    let copied = lockscreen::import_into(&image, &tmp.path().join("images")).unwrap();
    fs::remove_file(image).unwrap();
    lockscreen::set_wallpaper(&mut store, Some(copied.clone())).unwrap();
    let catalog = mods::load(&tmp.path().join("empty")).unwrap();
    let media = catalog
        .mods
        .iter()
        .find(|m| m.id == "lockscreen-media")
        .unwrap();
    lockscreen::apply_controls(
        &mut store,
        &media.controls,
        &BTreeMap::from([("media_enabled".into(), json!(true))]),
    )
    .unwrap();
    assert_eq!(
        lockscreen::load(&store).unwrap(),
        lockscreen::Config {
            wallpaper: Some(copied.clone()),
            media_enabled: true
        }
    );
    lockscreen::set_wallpaper(&mut store, None).unwrap();
    assert!(lockscreen::load(&store).unwrap().media_enabled);
    assert!(copied.exists());
    assert_eq!(fs::read_to_string(desktop).unwrap(), "original desktop");
    assert!(!store.comp.join("glass_effect").exists());
    assert!(store.backup.exists());
}
#[test]
fn invalid_images_and_replaced_preferences_are_preserved() {
    let tmp = tempfile::tempdir().unwrap();
    let bad = tmp.path().join("bad.png");
    fs::write(&bad, "not an image").unwrap();
    let copies = tmp.path().join("images");
    assert!(lockscreen::import_into(&bad, &copies).is_err());
    assert!(!copies.exists());
    let mut store = Store::new(tmp.path().join("config"), tmp.path().join("state"));
    let dir = store.root.join("cosmic/com.system76.CosmicLockscreen/v1");
    fs::create_dir_all(&dir).unwrap();
    std::os::unix::fs::symlink(&bad, dir.join("wallpaper")).unwrap();
    assert!(lockscreen::set_wallpaper(&mut store, None).is_err());
    assert_eq!(fs::read_to_string(bad).unwrap(), "not an image");
}
#[test]
fn catalog_cannot_target_authentication_or_cross_component_settings() {
    let baseline: serde_json::Value =
        serde_json::from_slice(include_bytes!("../mods/catalog.json")).unwrap();
    for (index, key, kind) in [
        (2, "../../pam", "image"),
        (2, "wallpaper", "boolean"),
        (3, "penumbra_enabled", "boolean"),
        (0, "media_enabled", "boolean"),
    ] {
        let mut c = baseline.clone();
        c["mods"][index]["controls"][0]["key"] = json!(key);
        c["mods"][index]["controls"][0]["kind"] = json!(kind);
        assert!(mods::parse(&serde_json::to_vec(&c).unwrap()).is_err());
    }
    let mut c = baseline;
    c["mods"][2]["preset"] = json!({"wallpaper":"/tmp/image.png"});
    assert!(mods::parse(&serde_json::to_vec(&c).unwrap()).is_err());
}
#[test]
fn old_cached_catalog_does_not_hide_new_builtin_mods() {
    let tmp = tempfile::tempdir().unwrap();
    let mut old: serde_json::Value =
        serde_json::from_slice(include_bytes!("../mods/catalog.json")).unwrap();
    old["revision"] = json!(2);
    old["mods"].as_array_mut().unwrap().truncate(2);
    fs::write(
        tmp.path().join(mods::ASSET),
        serde_json::to_vec(&old).unwrap(),
    )
    .unwrap();
    assert_eq!(mods::load(tmp.path()).unwrap().mods.len(), 4);
}
