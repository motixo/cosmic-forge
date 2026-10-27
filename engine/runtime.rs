// SPDX-License-Identifier: GPL-3.0-only
// Shared wire format: copied into the compositor by engine/build.py.
use serde::{Deserialize, Serialize};

pub const ABI: u32 = 1;
pub const MAX_SHADER: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Parameter {
    pub key: String,
    pub name: String,
    pub value: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeEffect {
    pub abi: u32,
    pub id: String,
    pub base: String,
    pub source: String,
    pub parameters: Vec<Parameter>,
}

pub fn identifier(value: &str) -> bool {
    (1..=64).contains(&value.len())
        && value.as_bytes()[0].is_ascii_lowercase()
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

impl RuntimeEffect {
    pub fn valid(&self) -> bool {
        self.abi == ABI
            && !self.id.is_empty()
            && self.id.len() <= 64
            && self
                .id
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
            && matches!(self.base.as_str(), "crystal" | "aero")
            && !self.source.trim().is_empty()
            && self.source.len() <= MAX_SHADER
            && !self.source.contains('\0')
            && self.parameters.len() <= 32
            && self.parameters.iter().enumerate().all(|(i, p)| {
                identifier(&p.key)
                    && identifier(&p.name)
                    && p.name.starts_with("forge_")
                    && p.value.is_finite()
                    && p.value.abs() <= 10000.0
                    && self.parameters[..i]
                        .iter()
                        .all(|other| other.name != p.name)
            })
    }
}
