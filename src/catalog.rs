// SPDX-License-Identifier: GPL-3.0-only
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, HashSet},
    fs,
    path::Path,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Shader {
    pub base: String,
    pub source: String,
    #[serde(default, skip_serializing_if = "crate::runtime::is_false")]
    pub animated: bool,
    #[serde(default, skip_serializing_if = "crate::runtime::Backdrop::is_scene")]
    pub backdrop: crate::runtime::Backdrop,
    #[serde(default)]
    pub uniforms: BTreeMap<String, String>,
}

impl Shader {
    pub fn renderer_revision(&self) -> u64 {
        if self.backdrop == crate::runtime::Backdrop::Wallpaper
            || self
                .source
                .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                .any(|word| matches!(word, "window_active" | "is_dark"))
        {
            4
        } else if self.animated {
            3
        } else if self.base == "glass" {
            2
        } else {
            1
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ControlKind {
    Integer,
    Boolean,
}

fn comp_target() -> String {
    "comp".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Control {
    pub key: String,
    pub label: String,
    #[serde(default)]
    pub description: String,
    #[serde(rename = "type")]
    pub kind: ControlKind,
    #[serde(default)]
    pub min: i32,
    #[serde(default)]
    pub max: i32,
    pub default: Value,
    #[serde(default)]
    pub unit: String,
    #[serde(default = "comp_target")]
    pub target: String,
    #[serde(default)]
    pub legacy: Option<String>,
}

impl Control {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            valid_identifier(&self.key),
            "Invalid control key: {}",
            self.key
        );
        ensure!(
            !matches!(self.key.as_str(), "forge_runtime" | "forge_backdrop"),
            "Reserved engine setting"
        );
        ensure!(!self.label.trim().is_empty(), "A control needs a label");
        ensure!(
            self.target == "comp"
                || (self.target == "theme" && matches!(self.key.as_str(), "frosted" | "alpha_map")),
            "Invalid control target"
        );
        match self.kind {
            ControlKind::Boolean => {
                ensure!(self.target == "comp", "Theme strength must be an integer");
                ensure!(self.default.is_boolean(), "Invalid boolean default");
            }
            ControlKind::Integer => {
                let value = self.default.as_i64().context("Invalid integer default")?;
                ensure!(
                    i64::from(self.min) <= value && value <= i64::from(self.max),
                    "Default outside control bounds"
                );
                if self.target == "theme" {
                    ensure!(
                        self.min == 0 && self.max == if self.key == "frosted" { 13 } else { 100 },
                        "Invalid theme control bounds"
                    );
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Effect {
    pub id: String,
    pub name: String,
    pub tagline: String,
    pub description: String,
    pub controls: Vec<Control>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shader: Option<Shader>,
    #[serde(skip)]
    pub pack_id: Option<String>,
}

impl Effect {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            valid_identifier(&self.id),
            "Invalid effect identifier: {}",
            self.id
        );
        ensure!(
            !self.name.trim().is_empty()
                && !self.tagline.trim().is_empty()
                && !self.description.trim().is_empty(),
            "Effect {} needs a name and description",
            self.id
        );
        let mut keys = HashSet::new();
        for control in &self.controls {
            control
                .validate()
                .with_context(|| format!("Invalid control in {}", self.id))?;
            ensure!(
                keys.insert(&control.key),
                "Duplicate control: {}",
                control.key
            );
        }
        if let Some(shader) = &self.shader {
            let runtime = crate::runtime::RuntimeEffect {
                abi: crate::runtime::ABI,
                id: self.id.clone(),
                base: shader.base.clone(),
                source: shader.source.clone(),
                animated: shader.animated,
                backdrop: shader.backdrop,
                parameters: shader
                    .uniforms
                    .iter()
                    .map(|(name, key)| crate::runtime::Parameter {
                        key: key.clone(),
                        name: name.clone(),
                        value: 0.0,
                    })
                    .collect(),
            };
            ensure!(runtime.valid(), "Invalid glass shader ABI");
            if shader.base == "glass"
                && let Some(key) = shader.uniforms.get("forge_blur")
            {
                let control = self
                    .controls
                    .iter()
                    .find(|c| &c.key == key)
                    .context("Glass blur references a missing control")?;
                ensure!(
                    control.kind == ControlKind::Integer && control.min >= 0 && control.max <= 100,
                    "Generic glass blur must be an integer in 0–100"
                );
            }
            for key in shader.uniforms.values() {
                let control = self
                    .controls
                    .iter()
                    .find(|c| &c.key == key)
                    .context("Shader uniform references a missing control")?;
                ensure!(
                    control.target == "comp" && control.min >= -10000 && control.max <= 10000,
                    "Invalid shader control bounds"
                );
            }
        }
        Ok(())
    }
}

pub fn valid_identifier(value: &str) -> bool {
    let bytes = value.as_bytes();
    (1..=64).contains(&bytes.len())
        && bytes[0].is_ascii_lowercase()
        && bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_' || *b == b'-')
}

pub fn load(extra: &Path) -> Result<Vec<Effect>> {
    load_with_pack(extra, None)
}

pub fn load_with_pack(
    extra: &Path,
    replacement: Option<&crate::packs::Pack>,
) -> Result<Vec<Effect>> {
    let mut effects: Vec<Effect> = serde_json::from_str(include_str!("../shaders/effects.json"))?;
    if extra.exists() {
        let mut paths = fs::read_dir(extra)?
            .map(|e| e.map(|e| e.path()))
            .collect::<std::io::Result<Vec<_>>>()?;
        paths.sort();
        for path in paths
            .into_iter()
            .filter(|p| p.extension().is_some_and(|e| e == "json"))
        {
            let data: Value = serde_json::from_slice(&fs::read(&path)?)
                .with_context(|| format!("Could not read {}", path.display()))?;
            let parsed: Vec<Effect> = if data.is_array() {
                serde_json::from_value(data)?
            } else {
                vec![serde_json::from_value(data)?]
            };
            effects.extend(parsed);
        }
    }
    crate::packs::merge(&mut effects, &extra.with_file_name("packs"), replacement)?;
    validate_catalog(&effects)?;
    Ok(effects)
}

pub fn validate_catalog(effects: &[Effect]) -> Result<()> {
    let mut identifiers = HashSet::new();
    for effect in effects {
        effect.validate()?;
        if !identifiers.insert(&effect.id) {
            bail!("Duplicate effect identifier: {}", effect.id);
        }
    }
    Ok(())
}
