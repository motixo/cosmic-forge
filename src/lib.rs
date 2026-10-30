// SPDX-License-Identifier: GPL-3.0-only
pub mod catalog;
pub mod engine;
mod material;
pub mod packs;
#[path = "../engine/runtime.rs"]
pub mod runtime;
pub mod store;
pub mod updates;

pub const APP_ID: &str = "io.github.motixo.CosmicForge";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
