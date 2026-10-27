// SPDX-License-Identifier: GPL-3.0-only
use super::{blur_effect, forge_runtime::RuntimeEffect};
use crate::backend::render::element::AsGlowRenderer;
use smithay::backend::renderer::gles::{GlesRenderer, GlesTexProgram, UniformName, UniformType};
use std::{
    borrow::{Borrow, BorrowMut},
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    sync::{Arc, Mutex},
};

static CURRENT: Mutex<(u64, Option<Arc<RuntimeEffect>>)> = Mutex::new((0, None));

pub fn configure(config: &cosmic_config::Config) {
    use cosmic_config::ConfigGet;
    static ANNOUNCE: std::sync::Once = std::sync::Once::new();
    ANNOUNCE.call_once(|| {
        if let Some(directory) = std::env::var_os("XDG_RUNTIME_DIR") {
            let path = std::path::Path::new(&directory).join("cosmic-forge-engine-v1");
            let _ = std::fs::write(path, std::process::id().to_string());
        }
    });
    // Values and shader source arrive in one config key, so render threads see a complete effect.
    let selected = config.get::<String>("glass_effect").unwrap_or_default();
    let runtime = config
        .get::<RuntimeEffect>("forge_runtime")
        .ok()
        .filter(|effect| effect.valid() && effect.id == selected)
        .map(Arc::new);
    if let Some(effect) = &runtime {
        blur_effect::set_glass_effect(if effect.base == "aero" { 2 } else { 1 });
    }
    let mut current = CURRENT.lock().unwrap();
    if current.1 != runtime {
        current.0 = current.0.wrapping_add(1);
        current.1 = runtime;
    }
}

pub fn current() -> (u64, Option<Arc<RuntimeEffect>>) {
    CURRENT.lock().unwrap().clone()
}

#[derive(Default)]
struct Cache {
    key: Option<u64>,
    program: Option<GlesTexProgram>,
}

pub fn program<R: AsGlowRenderer>(
    renderer: &mut R,
    effect: &RuntimeEffect,
) -> Option<GlesTexProgram> {
    let mut hash = DefaultHasher::new();
    effect.source.hash(&mut hash);
    effect.base.hash(&mut hash);
    for parameter in &effect.parameters {
        parameter.name.hash(&mut hash);
    }
    let key = hash.finish();
    let cache = {
        let data = Borrow::<GlesRenderer>::borrow(renderer.glow_renderer())
            .egl_context()
            .user_data();
        data.insert_if_missing_threadsafe(|| Arc::new(Mutex::new(Cache::default())));
        data.get::<Arc<Mutex<Cache>>>().unwrap().clone()
    };
    let mut cache = cache.lock().unwrap();
    if cache.key == Some(key) {
        return cache.program.clone();
    }
    let mut uniforms = vec![
        UniformName::new("geo_size", UniformType::_2f),
        UniformName::new("corner_radius", UniformType::_4f),
        UniformName::new("input_to_geo", UniformType::Matrix3x3),
        UniformName::new("sharp_tex", UniformType::_1i),
        UniformName::new("blur_amount", UniformType::_1f),
        UniformName::new("theme_tint", UniformType::_3f),
        UniformName::new("theme_accent", UniformType::_3f),
        UniformName::new("uv_per_pixel", UniformType::_2f),
        UniformName::new("refraction_strength", UniformType::_1f),
        UniformName::new("facet_density", UniformType::_1f),
        UniformName::new("crystal_thickness", UniformType::_1f),
        UniformName::new("everyday_mode", UniformType::_1f),
        UniformName::new("aero_color", UniformType::_1f),
        UniformName::new("aero_reflections", UniformType::_1f),
    ];
    uniforms.extend(
        effect
            .parameters
            .iter()
            .map(|p| UniformName::new(p.name.clone(), UniformType::_1f)),
    );
    let result = BorrowMut::<GlesRenderer>::borrow_mut(renderer.glow_renderer_mut())
        .compile_custom_texture_shader(&effect.source, &uniforms);
    cache.program = match result {
        Ok(program) => Some(program),
        Err(error) => {
            tracing::warn!(
                effect = effect.id,
                ?error,
                "Forge shader rejected; using built-in glass"
            );
            None
        }
    };
    cache.key = Some(key);
    cache.program.clone()
}
