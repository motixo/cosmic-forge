// SPDX-License-Identifier: GPL-3.0-only
use cosmic_forge::{
    catalog,
    packs::{self, Pack},
    runtime::RuntimeEffect,
    store::Store,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::fs;

#[test]
fn shader_repository_builds_a_pack_compatible_with_forge() {
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("official.json");
    assert!(
        std::process::Command::new("python3")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/shaders/scripts/package_effects.py"
            ))
            .args(["--revision", "1", "--output"])
            .arg(&output)
            .output()
            .unwrap()
            .status
            .success()
    );
    let root = temp.path().join("data");
    assert_eq!(packs::installed_revision(&root), None);
    packs::install(&root, &fs::read(output).unwrap(), true).unwrap();
    assert_eq!(packs::installed_revision(&root), Some(1));
    let effects = catalog::load(&root.join("effects")).unwrap();
    assert_eq!(
        effects.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
        [
            "frosted",
            "crystal",
            "mica",
            "aero",
            "liquid-glass",
            "minecraft-glass",
            "void",
            "ember",
            "snow"
        ]
    );
    assert!(effects[1..].iter().all(|e| e.shader.is_some()));
    let mica = effects.iter().find(|e| e.id == "mica").unwrap();
    assert_eq!(mica.shader.as_ref().unwrap().renderer_revision(), 5);
    assert_eq!(
        mica.shader.as_ref().unwrap().backdrop,
        cosmic_forge::runtime::Backdrop::Wallpaper
    );
    let liquid = effects.iter().find(|e| e.id == "liquid-glass").unwrap();
    assert_eq!(liquid.shader.as_ref().unwrap().base, "glass");
    assert_eq!(liquid.shader.as_ref().unwrap().renderer_revision(), 2);
    let mut store = Store::new(temp.path().join("config"), temp.path().join("state"));
    store.select_effect(liquid).unwrap();
    let runtime: RuntimeEffect =
        ron::from_str(&fs::read_to_string(store.comp.join("forge_runtime")).unwrap()).unwrap();
    assert!(runtime.valid());
    assert_eq!(runtime.glass_blur(), 12);
    assert_eq!(
        runtime
            .parameters
            .iter()
            .find(|p| p.name == "forge_liquid_refraction")
            .unwrap()
            .value,
        80.0
    );
    assert!(
        !liquid
            .controls
            .iter()
            .any(|c| c.key == "forge_liquid_background_blend")
    );
    let lensing = liquid
        .controls
        .iter()
        .find(|c| c.key == "forge_liquid_refraction")
        .unwrap();
    assert_eq!(lensing.label, "Lensing");
    let clear_center = liquid
        .controls
        .iter()
        .find(|c| c.key == "forge_liquid_clear_center")
        .unwrap();
    assert_eq!(store.value(clear_center), json!(40));
    let dispersion = liquid
        .controls
        .iter()
        .find(|c| c.key == "forge_liquid_dispersion")
        .unwrap();
    assert_eq!(store.value(dispersion), json!(5));
    store.set_control(dispersion, &json!(8)).unwrap();
    store.set_control(lensing, &json!(35)).unwrap();
    store.set_control(clear_center, &json!(60)).unwrap();
    store.select_effect(&effects[2]).unwrap();
    store.select_effect(liquid).unwrap();
    let runtime: RuntimeEffect =
        ron::from_str(&fs::read_to_string(store.comp.join("forge_runtime")).unwrap()).unwrap();
    assert_eq!(
        runtime
            .parameters
            .iter()
            .find(|p| p.name == "forge_liquid_refraction")
            .unwrap()
            .value,
        35.0
    );
    assert_eq!(
        runtime
            .parameters
            .iter()
            .find(|p| p.name == "forge_liquid_clear_center")
            .unwrap()
            .value,
        60.0
    );
    assert_eq!(
        runtime
            .parameters
            .iter()
            .find(|p| p.name == "forge_liquid_dispersion")
            .unwrap()
            .value,
        8.0
    );
    for (id, key, amount) in [
        ("crystal", "forge_crystal_color", 61),
        ("aero", "aero_color", 27),
        ("liquid-glass", "forge_liquid_color", 44),
    ] {
        let effect = effects.iter().find(|e| e.id == id).unwrap();
        let color = effect.controls.iter().find(|c| c.key == key).unwrap();
        assert_eq!(color.label, "Theme color");
        store.select_effect(effect).unwrap();
        store.set_control(color, &json!(amount)).unwrap();
    }
    for (id, amount) in [("crystal", 61), ("aero", 27), ("liquid-glass", 44)] {
        let effect = effects.iter().find(|e| e.id == id).unwrap();
        store.select_effect(effect).unwrap();
        let color = effect
            .controls
            .iter()
            .find(|c| c.label == "Theme color")
            .unwrap();
        assert_eq!(store.value(color), json!(amount));
    }
    store.select_effect(&effects[1]).unwrap();
    let runtime: RuntimeEffect =
        ron::from_str(&fs::read_to_string(store.comp.join("forge_runtime")).unwrap()).unwrap();
    assert_eq!(runtime.parameters[0].name, "forge_crystal_color");
    assert_eq!(runtime.parameters[0].value, 61.0);
    let crystal_dispersion = effects[1]
        .controls
        .iter()
        .find(|c| c.key == "forge_crystal_dispersion")
        .unwrap();
    assert_eq!(store.value(crystal_dispersion), json!(1));
    store.set_control(crystal_dispersion, &json!(7)).unwrap();
    store.select_effect(liquid).unwrap();
    assert_eq!(store.value(dispersion), json!(8));
    store.select_effect(&effects[1]).unwrap();
    assert_eq!(store.value(crystal_dispersion), json!(7));
    let runtime: RuntimeEffect =
        ron::from_str(&fs::read_to_string(store.comp.join("forge_runtime")).unwrap()).unwrap();
    assert_eq!(
        runtime
            .parameters
            .iter()
            .find(|p| p.name == "forge_crystal_dispersion")
            .unwrap()
            .value,
        7.0
    );
    assert_eq!(store.value(effects[1].controls.last().unwrap()), json!(50));
}

fn sample() -> Vec<u8> {
    include_bytes!("../shaders/examples/prism.json").to_vec()
}

fn pair(directory: &std::path::Path) -> Vec<std::path::PathBuf> {
    fs::create_dir_all(directory).unwrap();
    let metadata = directory.join("settings.json");
    let shader = directory.join("glass.frag");
    fs::write(
        &metadata,
        include_bytes!("../shaders/examples/prism-files/prism.json"),
    )
    .unwrap();
    fs::write(
        &shader,
        include_bytes!("../shaders/examples/prism-files/prism.frag"),
    )
    .unwrap();
    vec![metadata, shader]
}

#[test]
fn generic_glass_imports_and_keeps_each_effects_blur_independent() {
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("data");
    let files = [
        std::path::PathBuf::from(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/shaders/examples/plain-glass/plain-glass.json"
        )),
        std::path::PathBuf::from(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/shaders/examples/plain-glass/plain-glass.frag"
        )),
    ];
    packs::import_files(&data, &files).unwrap();
    let effects = catalog::load(&data.join("effects")).unwrap();
    let first = &effects[3];
    assert_eq!(first.shader.as_ref().unwrap().renderer_revision(), 2);
    let mut second = first.clone();
    second.id = "another-glass".into();
    second.controls[0].key = "forge_another_blur".into();
    second
        .shader
        .as_mut()
        .unwrap()
        .uniforms
        .insert("forge_blur".into(), "forge_another_blur".into());
    let mut store = Store::new(temp.path().join("config"), temp.path().join("state"));
    fs::create_dir_all(&store.comp).unwrap();
    fs::write(store.comp.join("aero_blur_amount"), "79\n").unwrap();
    fs::write(store.comp.join("crystal_blur_amount"), "83\n").unwrap();
    store.select_effect(first).unwrap();
    store.set_control(&first.controls[0], &json!(17)).unwrap();
    store.select_effect(&second).unwrap();
    store.set_control(&second.controls[0], &json!(92)).unwrap();
    let runtime = || -> RuntimeEffect {
        ron::from_str(&fs::read_to_string(store.comp.join("forge_runtime")).unwrap()).unwrap()
    };
    assert_eq!(runtime().glass_blur(), 92);
    store.select_effect(first).unwrap();
    let current: RuntimeEffect =
        ron::from_str(&fs::read_to_string(store.comp.join("forge_runtime")).unwrap()).unwrap();
    assert!(current.valid());
    assert_eq!(current.base, "glass");
    assert_eq!(current.glass_blur(), 17);
    assert_eq!(
        fs::read_to_string(store.comp.join("aero_blur_amount")).unwrap(),
        "79\n"
    );
    assert_eq!(
        fs::read_to_string(store.comp.join("crystal_blur_amount")).unwrap(),
        "83\n"
    );
    assert_eq!(
        fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/engine/glass.frag")).unwrap(),
        first.shader.as_ref().unwrap().source
    );
}

#[test]
fn generic_glass_validates_blur_bounds_and_allows_custom_parameters() {
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("glass.json");
    let result = std::process::Command::new("python3")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/shaders/scripts/package_effects.py"
        ))
        .args([
            "--catalog",
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/shaders/examples/plain-glass/plain-glass.json"
            ),
            "--shader-dir",
            concat!(env!("CARGO_MANIFEST_DIR"), "/shaders/examples/plain-glass"),
            "--pack-id",
            "plain-glass",
            "--revision",
            "1",
            "--output",
        ])
        .arg(&output)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let good: serde_json::Value = serde_json::from_slice(&fs::read(output).unwrap()).unwrap();
    packs::parse(&serde_json::to_vec(&good).unwrap()).unwrap();
    for (key, value) in [
        ("min", json!(-1)),
        ("max", json!(101)),
        ("type", json!("boolean")),
    ] {
        let mut bad = good.clone();
        bad["effects"][0]["controls"][0][key] = value;
        assert!(packs::parse(&serde_json::to_vec(&bad).unwrap()).is_err());
    }
    let mut no_blur = good;
    no_blur["effects"][0]["shader"]["uniforms"] = json!({});
    let pack = packs::parse(&serde_json::to_vec(&no_blur).unwrap()).unwrap();
    let runtime = RuntimeEffect {
        abi: 1,
        id: pack.effects[0].id.clone(),
        base: "glass".into(),
        source: pack.effects[0].shader.as_ref().unwrap().source.clone(),
        animated: false,
        backdrop: Default::default(),
        parameters: vec![],
    };
    assert!(runtime.valid());
    assert_eq!(runtime.glass_blur(), 45);
}

#[test]
fn separate_frag_and_json_are_imported_as_a_self_contained_pack() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("data");
    let mut files = pair(&temp.path().join("input"));
    files.reverse();
    packs::import_files(&root, &files).unwrap();
    for path in files {
        fs::remove_file(path).unwrap();
    }
    let effects = catalog::load(&root.join("effects")).unwrap();
    assert_eq!(effects[3].pack_id.as_deref(), Some("prism-demo"));
    assert_eq!(
        effects[3].shader.as_ref().unwrap().source,
        include_str!("../shaders/examples/prism-files/prism.frag")
    );
    assert_eq!(
        effects[3].shader.as_ref().unwrap().uniforms["forge_prism"],
        "forge_prism_amount"
    );
    assert!(effects[..3].iter().all(|e| e.pack_id.is_none()));
}

#[test]
fn a_single_effect_definition_can_supply_json_settings() {
    let temp = tempfile::tempdir().unwrap();
    let files = pair(&temp.path().join("input"));
    let mut definition: serde_json::Value =
        serde_json::from_slice(&fs::read(&files[0]).unwrap()).unwrap();
    definition = definition["effects"][0].take();
    definition["revision"] = json!(3);
    fs::write(&files[0], serde_json::to_vec(&definition).unwrap()).unwrap();
    let root = temp.path().join("data");
    packs::import_files(&root, &files).unwrap();
    let installed = packs::parse(&fs::read(root.join("packs/prism.json")).unwrap()).unwrap();
    assert_eq!(installed.revision, 3);
    assert_eq!(installed.effects[0].id, "prism");
}

#[test]
fn animated_shaders_import_and_preserve_legacy_static_runtime() {
    let temp = tempfile::tempdir().unwrap();
    let files = pair(&temp.path().join("input"));
    let mut definition: serde_json::Value =
        serde_json::from_slice(&fs::read(&files[0]).unwrap()).unwrap();
    definition["effects"][0]["shader"]["animated"] = json!(true);
    fs::write(&files[0], serde_json::to_vec(&definition).unwrap()).unwrap();
    let data = temp.path().join("data");
    packs::import_files(&data, &files).unwrap();
    let effects = catalog::load(&data.join("effects")).unwrap();
    let effect = &effects[3];
    assert_eq!(effect.shader.as_ref().unwrap().renderer_revision(), 3);
    let mut store = Store::new(temp.path().join("config"), temp.path().join("state"));
    store.select_effect(effect).unwrap();
    store.set_control(&effect.controls[0], &json!(72)).unwrap();
    let encoded = fs::read_to_string(store.comp.join("forge_runtime")).unwrap();
    let runtime: RuntimeEffect = ron::from_str(&encoded).unwrap();
    assert!(runtime.animated && runtime.valid());
    assert_eq!(runtime.parameters[0].value, 72.0);
    let mut static_effect = effect.clone();
    let mut accumulating = effect.clone();
    let shader = accumulating.shader.as_mut().unwrap();
    shader.source.push_str("\nuniform float window_age;\n");
    assert_eq!(shader.renderer_revision(), 6);
    shader.backdrop = cosmic_forge::runtime::Backdrop::Wallpaper;
    assert_eq!(shader.renderer_revision(), 6);
    shader.source.push_str("\nuniform vec2 output_size;\n");
    shader.animated = false;
    assert_eq!(shader.renderer_revision(), 7);
    let mut positioned = effect.clone();
    let shader = positioned.shader.as_mut().unwrap();
    shader.source.push_str("\nuniform vec2 surface_position;\n");
    shader.animated = false;
    assert_eq!(shader.renderer_revision(), 7);
    static_effect.shader.as_mut().unwrap().animated = false;
    assert_eq!(
        static_effect.shader.as_ref().unwrap().renderer_revision(),
        1
    );
    store.select_effect(&static_effect).unwrap();
    let encoded = fs::read_to_string(store.comp.join("forge_runtime")).unwrap();
    assert!(!encoded.contains("animated"));
    assert!(!ron::from_str::<RuntimeEffect>(&encoded).unwrap().animated);
    assert!(
        serde_json::to_value(&static_effect).unwrap()["shader"]
            .get("animated")
            .is_none()
    );
    definition["effects"][0]["shader"]["animated"] = json!("true");
    assert!(packs::parse(&serde_json::to_vec(&definition).unwrap()).is_err());
}

#[test]
fn animation_switch_persists_without_affecting_static_effects_or_other_controls() {
    let temp = tempfile::tempdir().unwrap();
    let files = pair(&temp.path().join("input"));
    let mut definition: serde_json::Value =
        serde_json::from_slice(&fs::read(&files[0]).unwrap()).unwrap();
    definition["effects"][0]["shader"]["animated"] = json!(true);
    fs::write(&files[0], serde_json::to_vec(&definition).unwrap()).unwrap();
    let data = temp.path().join("data");
    packs::import_files(&data, &files).unwrap();
    let effects = catalog::load(&data.join("effects")).unwrap();
    let effect = &effects[3];
    let mut store = Store::new(temp.path().join("config"), temp.path().join("state"));
    let runtime = |store: &Store| -> RuntimeEffect {
        ron::from_str(&fs::read_to_string(store.comp.join("forge_runtime")).unwrap()).unwrap()
    };
    assert!(store.animated(effect));
    store.select_effect(effect).unwrap();
    store.set_control(&effect.controls[0], &json!(72)).unwrap();
    store.set_animation(effect, false).unwrap();
    assert!(!store.animated(effect) && !runtime(&store).animated);
    assert_eq!(runtime(&store).parameters[0].value, 72.0);
    store.set_control(&effect.controls[0], &json!(64)).unwrap();
    assert!(!runtime(&store).animated);
    store.select_effect(&effects[2]).unwrap();
    let before = fs::read(store.comp.join("forge_runtime")).ok();
    assert!(!store.animated(&effects[2]));
    assert!(store.set_animation(&effects[2], true).is_err());
    store.set_animation(effect, true).unwrap();
    assert_eq!(fs::read(store.comp.join("forge_runtime")).ok(), before);
    store.undo().unwrap();
    store.select_effect(effect).unwrap();
    assert!(!runtime(&store).animated);
    store.reset(effect).unwrap();
    assert!(runtime(&store).animated && store.animated(effect));
    store.undo().unwrap();
    assert!(!runtime(&store).animated && !store.animated(effect));
    store.set_animation(effect, true).unwrap();
    assert!(runtime(&store).animated);
    let saved = Store::new(store.root.clone(), temp.path().join("state"));
    assert!(saved.animated(effect));
    fs::write(
        store.root.join("cosmic-forge/animations/prism.json"),
        "broken",
    )
    .unwrap();
    assert!(store.animated(effect));
}

#[test]
fn backdrop_defaults_overrides_and_renderer_requirements_cover_every_base() {
    use cosmic_forge::runtime::Backdrop;
    let temp = tempfile::tempdir().unwrap();
    let files = pair(&temp.path().join("input"));
    let mut definition: serde_json::Value =
        serde_json::from_slice(&fs::read(&files[0]).unwrap()).unwrap();
    definition["effects"][0]["shader"]["backdrop"] = json!("wallpaper");
    fs::write(&files[0], serde_json::to_vec(&definition).unwrap()).unwrap();
    let data = temp.path().join("data");
    packs::import_files(&data, &files).unwrap();
    let effects = catalog::load(&data.join("effects")).unwrap();
    let mut effect = effects[3].clone();
    let config = temp.path().join("config");
    let state = temp.path().join("state");
    let mut store = Store::new(config.clone(), state.clone());
    for base in ["glass", "aero", "crystal"] {
        effect.shader.as_mut().unwrap().base = base.into();
        effect.validate().unwrap();
        assert_eq!(effect.shader.as_ref().unwrap().renderer_revision(), 5);
        store.select_effect(&effect).unwrap();
        let runtime: RuntimeEffect =
            ron::from_str(&fs::read_to_string(store.comp.join("forge_runtime")).unwrap()).unwrap();
        assert!(runtime.valid());
        assert_eq!(runtime.backdrop, Backdrop::Wallpaper);
    }
    store.set_backdrop(&effect, Backdrop::Scene).unwrap();
    store.set_control(&effect.controls[0], &json!(72)).unwrap();
    store.select_effect(&effects[2]).unwrap();
    store
        .set_backdrop(&effects[2], Backdrop::Wallpaper)
        .unwrap();
    assert_eq!(
        Store::text(&store.comp.join("forge_backdrop"), ""),
        "wallpaper"
    );
    assert!(!store.comp.join("forge_runtime").exists());
    let mut store = Store::new(config, state);
    assert_eq!(store.backdrop(&effect), Backdrop::Scene);
    assert_eq!(store.backdrop(&effects[2]), Backdrop::Wallpaper);
    store.select_effect(&effect).unwrap();
    let encoded = fs::read_to_string(store.comp.join("forge_runtime")).unwrap();
    assert!(!encoded.contains(",backdrop:"));
    assert_eq!(
        ron::from_str::<RuntimeEffect>(&encoded).unwrap().parameters[0].value,
        72.0
    );
    let mut updated = effect.clone();
    updated.shader.as_mut().unwrap().backdrop = Backdrop::Wallpaper;
    store.select_effect(&updated).unwrap();
    assert_eq!(store.backdrop(&updated), Backdrop::Scene);
    store.reset(&updated).unwrap();
    assert_eq!(store.backdrop(&updated), Backdrop::Wallpaper);
    let encoded = fs::read_to_string(store.comp.join("forge_runtime")).unwrap();
    assert_eq!(
        ron::from_str::<RuntimeEffect>(&encoded).unwrap().backdrop,
        Backdrop::Wallpaper
    );
    store.undo().unwrap();
    assert_eq!(store.backdrop(&updated), Backdrop::Scene);
    for input in ["window_active", "is_dark"] {
        updated.shader.as_mut().unwrap().backdrop = Backdrop::Scene;
        updated
            .shader
            .as_mut()
            .unwrap()
            .source
            .push_str(&format!("\nuniform float {input};"));
        assert_eq!(updated.shader.as_ref().unwrap().renderer_revision(), 4);
    }
    definition["effects"][0]["shader"]["backdrop"] = json!("desktop");
    assert!(packs::parse(&serde_json::to_vec(&definition).unwrap()).is_err());
    let invalid = encoded.replace("backdrop:wallpaper", "backdrop:desktop");
    assert!(ron::from_str::<RuntimeEffect>(&invalid).is_err());
}

#[test]
fn invalid_file_pairs_preserve_the_working_effect() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("data");
    packs::install(&root, &sample(), false).unwrap();
    let files = pair(&temp.path().join("input"));
    let before = fs::read(root.join("packs/prism-demo.json")).unwrap();
    for selection in [
        vec![],
        vec![files[1].clone()],
        vec![files[0].clone(), files[0].clone()],
        vec![files[0].clone(), files[1].clone(), files[1].clone()],
    ] {
        assert!(packs::import_files(&root, &selection).is_err());
    }
    for source in [
        vec![0xff],
        vec![b' '; cosmic_forge::runtime::MAX_SHADER + 1],
        vec![],
        vec![0],
    ] {
        fs::write(&files[1], source).unwrap();
        assert!(packs::import_files(&root, &files).is_err());
        assert_eq!(
            fs::read(root.join("packs/prism-demo.json")).unwrap(),
            before
        );
    }
    fs::write(
        &files[1],
        include_bytes!("../shaders/examples/prism-files/prism.frag"),
    )
    .unwrap();
    let mut metadata: serde_json::Value =
        serde_json::from_slice(&fs::read(&files[0]).unwrap()).unwrap();
    let extra = metadata["effects"][0].clone();
    metadata["effects"].as_array_mut().unwrap().push(extra);
    fs::write(&files[0], serde_json::to_vec(&metadata).unwrap()).unwrap();
    assert!(packs::import_files(&root, &files).is_err());
}

#[test]
fn removing_one_community_effect_keeps_its_pack_siblings_and_can_be_restored() {
    let temp = tempfile::tempdir().unwrap();
    let mut pack: Pack = serde_json::from_slice(&sample()).unwrap();
    let mut sibling = pack.effects[0].clone();
    sibling.id = "another-prism".into();
    pack.effects.push(sibling);
    let original = serde_json::to_vec(&pack).unwrap();
    packs::install(temp.path(), &original, false).unwrap();
    packs::remove_effect(temp.path(), "prism-demo", "prism").unwrap();
    let effects = catalog::load(&temp.path().join("effects")).unwrap();
    assert_eq!(effects.len(), 4);
    assert_eq!(effects[3].id, "another-prism");
    assert_eq!(
        fs::read(temp.path().join("packs/prism-demo.previous")).unwrap(),
        original
    );
    packs::import_files(
        temp.path(),
        &[{
            let path = temp.path().join("restore.json");
            fs::write(&path, &original).unwrap();
            path
        }],
    )
    .unwrap();
    assert_eq!(
        catalog::load(&temp.path().join("effects")).unwrap().len(),
        5
    );
    packs::remove_effect(temp.path(), "prism-demo", "prism").unwrap();
    packs::remove_effect(temp.path(), "prism-demo", "another-prism").unwrap();
    assert!(!temp.path().join("packs/prism-demo.json").exists());
    assert_eq!(
        catalog::load(&temp.path().join("effects")).unwrap().len(),
        3
    );
}

#[test]
fn protected_effects_and_official_revisions_cannot_be_overwritten_or_removed() {
    let temp = tempfile::tempdir().unwrap();
    let mut pack: Pack = serde_json::from_slice(&sample()).unwrap();
    pack.id = "cosmic-glass".into();
    let original = serde_json::to_vec(&pack).unwrap();
    packs::install(temp.path(), &original, true).unwrap();
    for (pack_id, effect_id) in [
        ("cosmic-glass", "prism"),
        ("prism-demo", "aero"),
        ("../escape", "prism"),
    ] {
        assert!(packs::remove_effect(temp.path(), pack_id, effect_id).is_err());
    }
    pack.effects[0].description.push_str(" Changed.");
    assert!(packs::install(temp.path(), &serde_json::to_vec(&pack).unwrap(), true).is_err());
    assert_eq!(
        fs::read(temp.path().join("packs/cosmic-glass.json")).unwrap(),
        original
    );
    let effects = catalog::load(&temp.path().join("effects")).unwrap();
    assert!(effects.iter().all(|e| e.pack_id.is_none()));
}

#[test]
fn import_is_atomic_and_preserves_existing_controls_and_undo() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("data");
    packs::install(&root, &sample(), false).unwrap();
    let effects = catalog::load(&root.join("effects")).unwrap();
    assert_eq!(effects.len(), 4);
    let effect = &effects[3];
    let mut store = Store::new(temp.path().join("config"), temp.path().join("state"));
    store.select_effect(effect).unwrap();
    let runtime_path = store.comp.join("forge_runtime");
    let original = fs::read(&runtime_path).unwrap();
    let control = effect
        .controls
        .iter()
        .find(|c| c.key == "forge_prism_amount")
        .unwrap();
    store.set_control(control, &json!(80)).unwrap();
    let runtime: RuntimeEffect =
        ron::from_str(&fs::read_to_string(&runtime_path).unwrap()).unwrap();
    assert!(runtime.valid());
    assert_eq!(runtime.parameters[0].value, 80.0);
    store.undo().unwrap();
    assert_eq!(fs::read(&runtime_path).unwrap(), original);
    store.undo().unwrap();
    assert!(!runtime_path.exists());
    assert_eq!(store.selected(), "frosted");
}

#[test]
fn invalid_updates_leave_the_working_pack_intact() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    packs::install(root, &sample(), false).unwrap();
    let file = root.join("packs/prism-demo.json");
    let before = fs::read(&file).unwrap();
    let mut invalid: Pack = serde_json::from_slice(&sample()).unwrap();
    invalid.revision += 1;
    invalid.effects[0].controls[0].key = "forge_runtime".into();
    assert!(packs::install(root, &serde_json::to_vec(&invalid).unwrap(), false).is_err());
    assert_eq!(fs::read(&file).unwrap(), before);
    assert!(!file.with_extension("previous").exists());
    invalid = serde_json::from_slice(&sample()).unwrap();
    invalid.revision += 1;
    invalid.effects[0].description.push_str(" Updated.");
    packs::install(root, &serde_json::to_vec(&invalid).unwrap(), false).unwrap();
    assert_eq!(fs::read(file.with_extension("previous")).unwrap(), before);
    assert!(packs::install(root, &before, false).is_err());
}

#[test]
fn rejects_conflicts_reserved_ids_unknown_abi_and_invalid_uniforms() {
    let temp = tempfile::tempdir().unwrap();
    packs::install(temp.path(), &sample(), false).unwrap();
    let mut p: Pack = serde_json::from_slice(&sample()).unwrap();
    p.id = "another-pack".into();
    assert!(packs::install(temp.path(), &serde_json::to_vec(&p).unwrap(), false).is_err());
    p.effects[0].id = "aero".into();
    assert!(p.validate().is_err());
    p.effects[0].id = "prism".into();
    p.effects[0]
        .shader
        .as_mut()
        .unwrap()
        .uniforms
        .insert("alpha".into(), "forge_prism_amount".into());
    assert!(p.validate().is_err());
    p.format = 2;
    assert!(p.validate().is_err());
    p.id = "../escape".into();
    assert!(p.validate().is_err());
    assert!(packs::parse(&vec![b' '; packs::MAX_PACK as usize + 1]).is_err());
}

#[test]
fn github_download_checks_digest_and_keeps_community_effects() {
    let temp = tempfile::tempdir().unwrap();
    packs::install(temp.path(), &sample(), false).unwrap();
    let mut official: Pack = serde_json::from_slice(&sample()).unwrap();
    official.id = "cosmic-glass".into();
    official.effects[0].id = "aero".into();
    let bytes = serde_json::to_vec(&official).unwrap();
    let digest: String = Sha256::digest(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let mut release = json!({"assets":[{"name":packs::ASSET,"size":bytes.len(),
        "browser_download_url":"https://github.com/motixo/cosmic-forge-shaders/releases/download/test/cosmic-forge-glass.json",
        "digest":format!("sha256:{digest}")}]});
    packs::update_from_release(temp.path(), &serde_json::to_vec(&release).unwrap(), |_| {
        Ok(bytes.clone())
    })
    .unwrap();
    assert_eq!(
        catalog::load(&temp.path().join("effects")).unwrap().len(),
        4
    );
    release["assets"][0]["digest"] = json!(format!("sha256:{}", "0".repeat(64)));
    assert!(
        packs::update_from_release(temp.path(), &serde_json::to_vec(&release).unwrap(), |_| Ok(
            bytes.clone()
        ))
        .is_err()
    );
    release["assets"][0]["browser_download_url"] = json!("https://example.org/evil");
    assert!(
        packs::update_from_release(
            temp.path(),
            &serde_json::to_vec(&release).unwrap(),
            |_| panic!("must not fetch")
        )
        .is_err()
    );
    release["assets"][0]["browser_download_url"] = json!(
        "https://github.com/motixo/cosmic-crystal/releases/download/test/cosmic-forge-glass.json"
    );
    assert!(
        packs::update_from_release(
            temp.path(),
            &serde_json::to_vec(&release).unwrap(),
            |_| panic!("must not fetch from the old repository")
        )
        .is_err()
    );
    assert_eq!(
        packs::update_from_release(temp.path(), br#"{"assets":[]}"#, |_| panic!(
            "must not fetch"
        ))
        .unwrap(),
        "No effect pack published yet"
    );
}
