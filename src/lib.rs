// SPDX-License-Identifier: GPL-3.0-only
pub mod catalog;
pub mod engine;
pub mod lockscreen;
mod material;
pub mod mods;
pub mod packs;
pub mod panel;
#[path = "../engine/runtime.rs"]
pub mod runtime;
pub mod store;

pub const APP_ID: &str = "io.github.motixo.CosmicForge";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
