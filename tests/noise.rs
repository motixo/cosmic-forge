// SPDX-License-Identifier: GPL-3.0-only
use cosmic_forge::{
    catalog, noise,
    runtime::{Backdrop, MAX_SHADER, Parameter, RuntimeEffect},
    store::Store,
};
use serde_json::json;
use std::fs;

fn shader() -> RuntimeEffect {
    RuntimeEffect {
        abi: 1,
        id: "custom".into(),
        base: "glass".into(),
        source: "// header\n#version 100\nprecision highp float;\nvoid main(void) { gl_FragColor = vec4(0.4); }".into(),
        animated: false,
        backdrop: Backdrop::Scene,
        parameters: vec![],
    }
}

#[test]
fn noise_wraps_the_original_entry_point_without_changing_animation_or_backdrop() {
    let mut runtime = shader();
    runtime.backdrop = Backdrop::Wallpaper;
    runtime.animated = true;
    noise::apply(&mut runtime, 15.0).unwrap();
    assert!(runtime.valid());
    assert!(runtime.source.starts_with("// header\n#version 100\n"));
    assert!(
        runtime
            .source
            .contains("#define main forge_glass_original_main")
    );
    assert!(runtime.source.contains("void main(void)"));
    assert!(runtime.source.contains("#undef main"));
    assert!(runtime.source.contains("forge_glass_original_main();"));
    assert_eq!(runtime.parameters.last().unwrap().key, noise::KEY);
    assert_eq!(runtime.parameters.last().unwrap().value, 15.0);
    assert!(runtime.animated);
    assert_eq!(runtime.backdrop, Backdrop::Wallpaper);
}

#[test]
fn noise_limits_and_failures_preserve_the_original_runtime() {
    let mut runtime = shader();
    runtime
        .source
        .push_str(&" ".repeat(MAX_SHADER - runtime.source.len()));
    let original = runtime.clone();
    assert!(noise::apply(&mut runtime, 20.0).is_err());
    assert_eq!(runtime, original);
    let mut runtime = shader();
    runtime.parameters = (0..32)
        .map(|i| Parameter {
            key: format!("forge_value_{i}"),
            name: format!("forge_value_{i}"),
            value: 1.0,
        })
        .collect();
    let original = runtime.clone();
    assert!(noise::apply(&mut runtime, 20.0).is_err());
    assert_eq!(runtime, original);
    for value in [-1.0, 101.0, f32::NAN] {
        let mut runtime = shader();
        let original = runtime.clone();
        assert!(noise::apply(&mut runtime, value).is_err());
        assert_eq!(runtime, original);
    }
}

#[test]
fn noise_is_shared_persistent_and_updates_the_active_shader_without_recompilation() {
    let temp = tempfile::tempdir().unwrap();
    let effects = catalog::load(&temp.path().join("effects")).unwrap();
    assert!(
        effects
            .iter()
            .all(|e| e.controls.iter().filter(|c| c.key == noise::KEY).count() == 1)
    );
    let mut effect = effects[1].clone();
    let source = shader();
    effect.shader = Some(catalog::Shader {
        base: "glass".into(),
        source: source.source,
        animated: false,
        backdrop: Backdrop::Scene,
        uniforms: Default::default(),
    });
    let mut store = Store::new(temp.path().join("config"), temp.path().join("state"));
    store.select_effect(&effect).unwrap();
    let read = || -> RuntimeEffect {
        ron::from_str(
            &fs::read_to_string(
                temp.path()
                    .join("config/cosmic/com.system76.CosmicComp/v1/forge_runtime"),
            )
            .unwrap(),
        )
        .unwrap()
    };
    let original_source = read().source;
    let control = effect
        .controls
        .iter()
        .find(|c| c.key == noise::KEY)
        .unwrap();
    store.set_control(control, &json!(0)).unwrap();
    assert_eq!(read().source, original_source);
    assert_eq!(read().parameters.last().unwrap().value, 0.0);
    store.set_control(control, &json!(80)).unwrap();
    assert_eq!(read().source, original_source);
    assert_eq!(read().parameters.last().unwrap().value, 80.0);
    let reopened = Store::new(store.root.clone(), temp.path().join("state"));
    assert!(effects.iter().all(|e| {
        reopened.value(e.controls.iter().find(|c| c.key == noise::KEY).unwrap()) == json!(80)
    }));
}

#[test]
fn native_glass_restores_the_stock_renderer_at_zero_and_tracks_frost_strength() {
    for id in ["frosted", "crystal", "aero"] {
        assert!(
            noise::native_runtime(id, 0.0, 6, Backdrop::Scene)
                .unwrap()
                .is_none()
        );
        let runtime = noise::native_runtime(id, 15.0, 6, Backdrop::Scene)
            .unwrap()
            .unwrap();
        assert!(runtime.valid());
        assert!(!runtime.animated);
        assert!(runtime.source.contains(noise::NATIVE));
    }
    for strength in 0..=13 {
        let runtime = noise::native_runtime("frosted", 15.0, strength, Backdrop::Wallpaper)
            .unwrap()
            .unwrap();
        assert_eq!(runtime.base, "glass");
        assert_eq!(runtime.backdrop, Backdrop::Wallpaper);
        let position = f64::from(runtime.glass_blur()) * 13.0 / 100.0;
        let native_step = (strength + 1).min(13);
        assert_eq!(position.floor() as i32, native_step);
        assert!(position - f64::from(native_step) <= 0.131);
        let extra = runtime
            .parameters
            .iter()
            .find(|p| p.name == "forge_frosted_extra_blur")
            .unwrap()
            .value;
        assert_eq!(extra > 0.0, strength == 13);
    }
}
