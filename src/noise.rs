// SPDX-License-Identifier: GPL-3.0-only
use crate::{
    catalog::{Control, ControlKind, Effect},
    runtime::{Backdrop, MAX_SHADER, Parameter, RuntimeEffect},
};
use anyhow::{Result, ensure};
use serde_json::json;

pub const KEY: &str = "forge_glass_noise";
const UNIFORM: &str = "forge_surface_noise";
pub const NATIVE: &str = "// Forge native glass grain\n";

pub fn control() -> Control {
    Control {
        key: KEY.into(),
        label: "Noise".into(),
        description: "Fine surface grain, shared across glass effects.".into(),
        kind: ControlKind::Integer,
        min: 0,
        max: 100,
        default: json!(15),
        unit: "%".into(),
        target: "comp".into(),
        legacy: None,
    }
}

pub fn add_control(effect: &mut Effect) {
    if !effect.controls.iter().any(|c| c.key == KEY) {
        effect.controls.push(control());
    }
}

pub fn apply(runtime: &mut RuntimeEffect, amount: f32) -> Result<()> {
    ensure!(runtime.valid(), "Invalid glass shader before adding noise");
    ensure!(
        amount.is_finite() && (0.0..=100.0).contains(&amount),
        "Noise must be between 0 and 100"
    );
    ensure!(
        runtime.parameters.len() < 32 && !runtime.parameters.iter().any(|p| p.name == UNIFORM),
        "This shader has no free parameter for surface noise"
    );
    let mut insertion = 0;
    let mut offset = 0;
    for line in runtime.source.split_inclusive('\n') {
        offset += line.len();
        if line.trim_start().starts_with("#version ") {
            insertion = offset;
            break;
        }
    }
    let mut source = runtime.source.clone();
    source.insert_str(insertion, "\n#define main forge_glass_original_main\n");
    source.push('\n');
    source.push_str(include_str!("noise.frag"));
    ensure!(
        source.len() <= MAX_SHADER,
        "Shader exceeds 64 KiB after adding noise"
    );
    runtime.source = source;
    runtime.parameters.push(Parameter {
        key: KEY.into(),
        name: UNIFORM.into(),
        value: amount,
    });
    Ok(())
}

pub fn native_runtime(
    id: &str,
    amount: f32,
    thickness: i32,
    backdrop: Backdrop,
) -> Result<Option<RuntimeEffect>> {
    if amount <= 0.0 {
        return Ok(None);
    }
    let (base, source) = match id {
        "frosted" => ("glass", include_str!("frosted.frag")),
        "crystal" => ("crystal", include_str!("../shaders/shaders/crystal.frag")),
        "aero" => ("aero", include_str!("../shaders/shaders/aero.frag")),
        _ => return Ok(None),
    };
    let mut runtime = RuntimeEffect {
        abi: crate::runtime::ABI,
        id: id.into(),
        base: base.into(),
        source: format!("{NATIVE}{source}"),
        animated: false,
        backdrop,
        parameters: Vec::new(),
    };
    if id == "frosted" {
        // COSMIC adds one to the theme's 0–13 strength before selecting a blur kernel.
        let blur = (((thickness.clamp(0, 13) + 1) * 100 + 12) / 13).min(100);
        runtime.parameters.push(Parameter {
            key: "forge_frosted_noise_blur".into(),
            name: "forge_blur".into(),
            value: blur as f32,
        });
        runtime.parameters.push(Parameter {
            key: "forge_frosted_extra_blur".into(),
            name: "forge_frosted_extra_blur".into(),
            // Match the variance between native kernels 13 and 14 across the blur pyramid.
            value: if thickness >= 13 { 79.16 } else { 0.0 },
        });
    }
    apply(&mut runtime, amount)?;
    Ok(Some(runtime))
}
