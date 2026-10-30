// SPDX-License-Identifier: GPL-3.0-only
mod ui;

use cosmic_forge::{
    APP_ID, VERSION, catalog, packs,
    store::{Store, xdg},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    match std::env::args().nth(1).as_deref() {
        Some("--version") => {
            println!("COSMIC Forge {VERSION} (Rust / libcosmic)");
            return Ok(());
        }
        Some("--help" | "-h") => {
            println!(
                "COSMIC Forge — glass shader manager for COSMIC\n\nUsage: cosmic-forge [--version | --status | --update-effects | --import-pack PACK.json | --import-pack SETTINGS.json SHADER.frag | --help]"
            );
            return Ok(());
        }
        Some("--update-effects") => {
            println!("{}", packs::update(&packs::root())?);
            return Ok(());
        }
        Some("--import-pack") => {
            let paths: Vec<_> = std::env::args_os()
                .skip(2)
                .map(std::path::PathBuf::from)
                .collect();
            println!("{}", packs::import_files(&packs::root(), &paths)?);
            return Ok(());
        }
        Some("--status") => {
            let store = Store::default();
            let effects =
                catalog::load(&xdg("XDG_DATA_HOME", ".local/share").join("cosmic-forge/effects"))?;
            let state = store.fingerprint(&effects);
            let engine = cosmic_forge::store::engine_status();
            println!(
                "{}",
                serde_json::to_string_pretty(
                    &serde_json::json!({"application": APP_ID, "version": VERSION, "effect": state.selected, "dark": state.dark, "accent": state.accent, "controls": effects.iter().flat_map(|e| e.controls.iter()).map(|c| (c.key.clone(),store.value(c))).collect::<serde_json::Map<_,_>>(), "surfaces": state.categories, "engine": {"state": engine.state, "status": engine.label(), "message": engine.note(), "installed_version": cosmic_forge::store::installed_compositor_version(), "installed_executable": engine.installed_executable, "running": engine.running, "marker_pid": engine.marker_pid}})
                )?
            );
            return Ok(());
        }
        Some(argument) => return Err(format!("Unknown argument: {argument}").into()),
        None => {}
    }
    let settings = cosmic::app::Settings::default()
        .size(cosmic::iced::Size::new(1040., 880.))
        .size_limits(cosmic::iced::Limits::NONE.min_width(680.).min_height(500.));
    cosmic::app::run_single_instance::<ui::Forge>(settings, ui::Flags)?;
    Ok(())
}
