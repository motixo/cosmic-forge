// SPDX-License-Identifier: GPL-3.0-only
use cosmic::{
    Core,
    app::{CosmicFlags, Task},
    iced::{self, Length, Subscription},
    prelude::*,
    widget::{self, button, nav_bar, settings, text},
};
use cosmic_forge::{
    APP_ID, VERSION,
    catalog::{self, Control, ControlKind, Effect},
    engine, mods, packs, panel,
    store::{self, Fingerprint, Store, xdg},
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};

pub struct Flags;
impl CosmicFlags for Flags {
    type SubCommand = String;
    type Args = Vec<String>;
}

#[derive(Clone, Copy, Debug)]
enum Page {
    Effect(usize),
    Panel(usize),
    LockScreen,
    Settings,
}
#[derive(Clone, Debug)]
pub enum Message {
    Value(usize, Value),
    Flush,
    Panel(nav_bar::Id),
    PanelProfile(usize),
    PanelMode(usize),
    PanelColor(widget::color_picker::ColorPickerUpdate),
    MatchPanelTheme,
    ModValue(usize, Value),
    ModPreset,
    Engine(engine::Component, engine::Action),
    LockScreen,
    Settings,
    Reset,
    About(bool),
    Repository,
    DismissEngine,
    DismissEngineUpdate,
    EngineSettings,
    Folder,
    Import,
    Imported(Option<Result<Vec<std::path::PathBuf>, String>>),
    RemoveEffect,
    CheckUpdates,
    Tick,
    Exit,
}

enum TransferEvent {
    Progress(engine::Progress),
    EngineUpdates(Vec<engine::Component>),
    Finished(Result<String, String>),
}

pub struct Forge {
    core: Core,
    nav: nav_bar::Model,
    panel_nav: nav_bar::Model,
    lock_nav: nav_bar::Model,
    effects: Vec<Effect>,
    page: Page,
    settings_return: Page,
    store: Store,
    state: Fingerprint,
    pending: BTreeMap<String, (Control, Value)>,
    changed: Instant,
    polled: Instant,
    error: Option<String>,
    about: bool,
    needs_engine: bool,
    engine_updates: Vec<engine::Component>,
    show_engine_update: bool,
    theme_dirty: bool,
    status: &'static str,
    engine_note: Option<&'static str>,
    transfer: Option<std::sync::mpsc::Receiver<TransferEvent>>,
    engine_progress: Option<engine::Progress>,
    renderer_note: String,
    engine_component: engine::Component,
    panel_note: String,
    panel_revision: u64,
    mods: Vec<mods::Mod>,
    show_updates: bool,
    panel_names: Vec<String>,
    panel_index: usize,
    panel_mode: usize,
    panel_profile: Option<panel::Profile>,
    color_picker: widget::color_picker::Model,
    update_note: String,
    checked: Instant,
    catalog_stamp: u64,
}

impl Forge {
    fn remove_effect(&mut self) -> anyhow::Result<()> {
        let Page::Effect(index) = self.page else {
            anyhow::bail!("Select an imported effect first");
        };
        let effect = self.effects[index].clone();
        let pack_id = effect
            .pack_id
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("Built-in effects cannot be removed"))?;
        let active = self.store.selected() == effect.id;
        if active {
            self.store.select_effect(
                self.effects
                    .iter()
                    .find(|e| e.id == "frosted")
                    .expect("Built-in Frosted Glass"),
            )?;
        }
        if let Err(error) = packs::remove_effect(&packs::root(), pack_id, &effect.id) {
            if active {
                self.store.undo()?;
            }
            return Err(error);
        }
        self.reload_effects()
    }
    fn check_updates(&mut self) {
        if self.transfer.is_some() {
            return;
        }
        self.engine_progress = None;
        self.checked = Instant::now();
        self.start_transfer("Checking effects, mods and engine…", |sender| {
            let effects = match packs::update(&packs::root()) {
                Ok(note) => format!("Effects: {note}"),
                Err(e) => format!("Effects: {e:#}"),
            };
            let mods = match mods::update(&mods::root()) {
                Ok(note) => format!("Mods: {note}"),
                Err(e) => format!("Mods: {e:#}"),
            };
            let engine = match engine::available_updates() {
                Ok(updates) => {
                    let note = if updates.is_empty() {
                        "Engine: no updates available"
                    } else {
                        "Engine update available · open Settings to update"
                    };
                    let _ = sender.send(TransferEvent::EngineUpdates(updates));
                    note.to_owned()
                }
                Err(e) => format!("Engine: {e:#}"),
            };
            Ok(format!("{effects}\n{mods}\n{engine}"))
        });
    }
    fn start_transfer(
        &mut self,
        note: &str,
        work: impl FnOnce(std::sync::mpsc::Sender<TransferEvent>) -> anyhow::Result<String>
        + Send
        + 'static,
    ) {
        if self.transfer.is_some() {
            return;
        }
        let (sender, receiver) = std::sync::mpsc::channel();
        self.transfer = Some(receiver);
        self.transfer_note(note.into());
        std::thread::spawn(move || {
            let result = work(sender.clone()).map_err(|e| format!("{e:#}"));
            let _ = sender.send(TransferEvent::Finished(result));
        });
    }
    fn transfer_note(&mut self, note: String) {
        if self.engine_progress.is_some() && self.engine_component == engine::Component::Panel {
            self.panel_note = note;
        } else if self.engine_progress.is_some() {
            self.renderer_note = note;
        } else {
            self.update_note = note;
        }
    }
    fn start_component(&mut self, component: engine::Component, action: engine::Action) {
        if self.transfer.is_some() {
            return;
        }
        self.show_updates = false;
        self.engine_component = component;
        self.engine_progress = Some(engine::Progress::Checking);
        self.start_transfer("Checking installed engine…", move |sender| {
            engine::setup_component(component, action, |progress| {
                let _ = sender.send(TransferEvent::Progress(progress));
            })
        });
    }
    fn reload_effects(&mut self) -> anyhow::Result<()> {
        let effects = catalog::load(&packs::root().join("effects"))?;
        if !self.flush() {
            anyhow::bail!("Could not save pending controls");
        }
        self.effects = effects;
        self.catalog_stamp = packs::fingerprint(&packs::root());
        self.nav = nav_bar::Model::default();
        for (index, effect) in self.effects.iter().enumerate() {
            self.nav
                .insert()
                .text(effect.name.clone())
                .data(Page::Effect(index));
        }
        if let Some(effect) = self
            .effects
            .iter()
            .find(|e| e.id == self.store.selected() && store::effect_available(e))
        {
            self.store.select_effect(effect)?;
        }
        self.refresh();
        self.sync_selection();
        Ok(())
    }
    fn refresh(&mut self) {
        let state = self.store.fingerprint(&self.effects);
        self.theme_dirty |=
            state.material != self.state.material || state.categories != self.state.categories;
        self.state = state;
    }
    fn report<T>(&mut self, result: anyhow::Result<T>) -> bool {
        match result {
            Ok(_) => {
                self.error = None;
                self.refresh();
                true
            }
            Err(error) => {
                self.error = Some(format!("{error:#}"));
                false
            }
        }
    }
    fn flush(&mut self) -> bool {
        if self.pending.is_empty() {
            return true;
        }
        let result = (|| {
            let changes = self
                .store
                .controls_changes(self.pending.values().map(|(c, v)| (c, v)))?;
            self.store.commit(changes, "Adjust effect")
        })();
        if self.report(result) {
            self.pending.clear();
            true
        } else {
            false
        }
    }
    fn value(&self, control: &Control) -> Value {
        self.pending
            .get(&control.key)
            .map(|v| v.1.clone())
            .unwrap_or_else(|| self.store.value(control))
    }
    fn sync_selection(&mut self) {
        let index = self
            .effects
            .iter()
            .position(|e| e.id == self.state.selected)
            .unwrap_or(0);
        if matches!(self.page, Page::Effect(_)) {
            self.page = Page::Effect(index);
        }
        if matches!(self.settings_return, Page::Effect(_)) {
            self.settings_return = Page::Effect(index);
        }
        for model in [&mut self.nav, &mut self.panel_nav, &mut self.lock_nav] {
            let active = model.active();
            widget::segmented_button::Selectable::deactivate(model, active);
        }
        let page = match self.page {
            Page::Settings => self.settings_return,
            page => page,
        };
        match page {
            Page::Effect(index) => self.nav.activate_position(index as u16),
            Page::Panel(index) => self.panel_nav.activate_position(index as u16),
            Page::LockScreen => self.lock_nav.activate_position(0),
            Page::Settings => false,
        };
    }
    fn reload_panel(&mut self, reset_picker: bool) {
        let result = (|| -> anyhow::Result<()> {
            let names = panel::profiles(&self.store)?;
            if let Some(current) = self.panel_names.get(self.panel_index) {
                self.panel_index = names.iter().position(|n| n == current).unwrap_or(0);
            }
            self.panel_names = names;
            let profile = self
                .panel_names
                .get(self.panel_index)
                .map(|name| panel::load(&self.store, name))
                .transpose()?;
            if reset_picker || profile != self.panel_profile {
                self.panel_profile = profile;
                self.reset_panel_picker();
            }
            Ok(())
        })();
        if let Err(error) = result {
            self.error = Some(format!("{error:#}"));
        }
    }
    fn reset_panel_picker(&mut self) {
        let dark = self.panel_mode == 1;
        let color = self
            .panel_profile
            .as_ref()
            .and_then(|p| p.colors)
            .and_then(|c| c.get(dark));
        let rgb = |c: [f32; 3]| iced::Color::from_rgb(c[0], c[1], c[2]);
        self.color_picker = widget::color_picker::Model::new(
            "Hex",
            "RGB",
            Some(rgb(panel::theme_color(&self.store, dark))),
            color.map(rgb),
        );
    }
    fn save_panel_color(&mut self, color: Option<[f32; 3]>) {
        if self.panel_revision > 0
            && let Some(name) = self.panel_names.get(self.panel_index)
        {
            let result = panel::set_color(&mut self.store, name, self.panel_mode == 1, color);
            if self.report(result) {
                self.reload_panel(true);
            }
        }
    }
    fn reload_mods(&mut self) {
        let result = mods::load(&mods::root());
        let catalog = match result {
            Ok(c) => c,
            Err(e) => {
                self.error = Some(format!("{e:#}"));
                return;
            }
        };
        if self.mods == catalog.mods {
            return;
        }
        let remap = |page: Page| match page {
            Page::Panel(index) => self
                .mods
                .get(index)
                .and_then(|m| catalog.mods.iter().position(|new| new.id == m.id))
                .map(Page::Panel)
                .unwrap_or(Page::Effect(0)),
            page => page,
        };
        self.page = remap(self.page);
        self.settings_return = remap(self.settings_return);
        self.mods = catalog.mods;
        self.panel_nav = nav_bar::Model::default();
        for (index, m) in self.mods.iter().enumerate() {
            self.panel_nav
                .insert()
                .text(m.name.clone())
                .data(Page::Panel(index));
        }
        self.sync_selection();
    }
    fn set_mod_values(&mut self, values: BTreeMap<String, Value>) {
        if let Page::Panel(index) = self.page
            && let Some(m) = self.mods.get(index)
            && self.panel_revision >= m.engine_revision
            && let Some(profile) = self.panel_names.get(self.panel_index)
        {
            let result = panel::apply_controls(&mut self.store, profile, &m.controls, &values);
            if self.report(result) {
                self.reload_panel(false);
            }
        }
    }
    fn engine_component_view(&self, component: engine::Component) -> Element<'_, Message> {
        let (install_label, build_label, restore_label, name, note) = match component {
            engine::Component::Compositor => (
                "Install compositor",
                "Build compositor",
                "Restore original COSMIC compositor",
                "compositor",
                self.renderer_note.as_str(),
            ),
            engine::Component::Panel => (
                "Install panel",
                "Build panel",
                "Restore original COSMIC panel",
                "panel",
                self.panel_note.as_str(),
            ),
        };
        let mut install = button::standard(install_label);
        let mut build = button::standard(build_label);
        let mut restore = button::standard(restore_label);
        if self.transfer.is_none() {
            install = install.on_press(Message::Engine(component, engine::Action::Install));
            build = build.on_press(Message::Engine(component, engine::Action::Build));
            restore = restore.on_press(Message::Engine(component, engine::Action::Restore));
        }
        let mut content=widget::column::with_capacity(6).spacing(16)
            .push(text::heading(component.label()))
            .push(text::body(format!("Requires a supported COSMIC release from 1.10 onward. Install a prebuilt {name} or build from source, then sign out and back in.")))
            .push(widget::row::with_capacity(3).spacing(12).align_y(iced::Alignment::Center).push(install).push(text::body("or")).push(build))
            .push(restore);
        let progress = self
            .engine_progress
            .filter(|_| self.engine_component == component);
        if let Some(progress) = progress {
            let mut bar = widget::progress_bar::linear::Linear::new()
                .width(Length::Fill)
                .girth(6.0);
            if progress.finished() {
                bar = bar.progress(1.0);
            }
            let label = if component == engine::Component::Panel {
                progress.panel_label()
            } else {
                progress.label()
            };
            content = content.push(
                widget::column::with_capacity(2)
                    .spacing(10)
                    .push(text::body(label))
                    .push(bar),
            );
        } else if !note.is_empty() {
            content = content.push(text::caption(note));
        }
        content.into()
    }
    fn panel_view(&self, index: usize) -> Element<'_, Message> {
        let Some(m) = self.mods.get(index) else {
            return text::body("No panel mods are available.").into();
        };
        let mut content = widget::column::with_capacity(12)
            .spacing(18)
            .push(text::title2(m.name.as_str()))
            .push(text::body(m.description.as_str()))
            .push(
                settings::item::builder("Panel profile").control(widget::dropdown(
                    self.panel_names.as_slice(),
                    Some(self.panel_index),
                    Message::PanelProfile,
                )),
            );
        if self.panel_revision < m.engine_revision {
            return content.push(text::body("Install or update the panel engine in Settings, then sign out and back in to use this mod."))
                .push(button::suggested("Go to settings").on_press(Message::EngineSettings)).into();
        }
        let Some(profile) = &self.panel_profile else {
            return content
                .push(text::body("No COSMIC panel profiles are configured."))
                .into();
        };
        for (i, c) in m.controls.iter().enumerate() {
            match c.kind {
                mods::Kind::ThemeColor => {
                    let customized = profile
                        .colors
                        .and_then(|c| c.get(self.panel_mode == 1))
                        .is_some();
                    content = content
                        .push(settings::item::builder("Theme").control(widget::dropdown(
                            &["Light", "Dark"][..],
                            Some(self.panel_mode),
                            Message::PanelMode,
                        )))
                        .push(
                            settings::item::builder(c.label.as_str())
                                .description(if customized {
                                    "Custom color"
                                } else {
                                    "Following COSMIC theme"
                                })
                                .control(
                                    self.color_picker
                                        .picker_button(Message::PanelColor, None)
                                        .width(52)
                                        .height(36),
                                ),
                        )
                        .push(button::standard("Match theme").on_press(Message::MatchPanelTheme));
                    if self.color_picker.get_is_active() {
                        content = content.push(
                            self.color_picker
                                .builder(Message::PanelColor)
                                .save_label("Apply")
                                .cancel_label("Cancel")
                                .build("Recent colors", "Copy to clipboard", "Copied"),
                        );
                    }
                    content = content.push(text::caption("Light and dark colors switch with your desktop theme, even while Forge is closed."));
                }
                mods::Kind::Boolean => {
                    let value = panel::control_value(&self.store, &profile.name, c)
                        .ok()
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    content = content.push(
                        settings::item::builder(c.label.as_str())
                            .description(c.description.as_str())
                            .control(
                                widget::toggler(value)
                                    .on_toggle(move |v| Message::ModValue(i, json!(v))),
                            ),
                    );
                }
                mods::Kind::Integer => {
                    let value = panel::control_value(&self.store, &profile.name, c)
                        .ok()
                        .and_then(|v| v.as_i64())
                        .unwrap_or(i64::from(c.min)) as i32;
                    content = content
                        .push(
                            settings::item::builder(c.label.as_str())
                                .description(c.description.as_str())
                                .control(widget::spin_button(
                                    format!("{value}{}", c.unit),
                                    c.label.clone(),
                                    value,
                                    1,
                                    c.min,
                                    c.max,
                                    move |v| Message::ModValue(i, json!(v)),
                                )),
                        )
                        .push(
                            widget::slider(c.min..=c.max, value, move |v| {
                                Message::ModValue(i, json!(v))
                            })
                            .step(1),
                        );
                }
            }
        }
        if !m.preset.is_empty() {
            content = content.push(button::standard("Apply preset").on_press(Message::ModPreset));
        }
        content.into()
    }
    fn mods_view(&self, title: &str, note: &str) -> Element<'_, Message> {
        widget::column::with_capacity(3)
            .spacing(18)
            .push(text::title2(title.to_owned()))
            .push(text::body(note.to_owned()))
            .push(text::body("No mods are available in this section yet."))
            .into()
    }
    fn effect_view(&self, index: usize) -> Element<'_, Message> {
        let effect = &self.effects[index];
        let mut section = settings::section().title("Appearance");
        for (i, control) in effect.controls.iter().enumerate() {
            match control.kind {
                ControlKind::Boolean => {
                    section = section.add(
                        settings::item::builder(control.label.as_str())
                            .description(control.description.as_str())
                            .toggler(self.value(control).as_bool().unwrap_or(false), move |v| {
                                Message::Value(i, json!(v))
                            }),
                    )
                }
                ControlKind::Integer => {
                    let value = self
                        .value(control)
                        .as_i64()
                        .unwrap_or(i64::from(control.min)) as i32;
                    let label = if control.key == "frosted" {
                        format!("{} / 13", value)
                    } else {
                        format!("{}{}", value, control.unit)
                    };
                    let slider = widget::slider(control.min..=control.max, value, move |v| {
                        Message::Value(i, json!(v))
                    })
                    .step(1)
                    .on_release(Message::Flush)
                    .width(Length::Fill);
                    let spin = widget::spin_button(
                        label,
                        control.label.clone(),
                        value,
                        1,
                        control.min,
                        control.max,
                        move |v| Message::Value(i, json!(v)),
                    );
                    let heading = settings::item::builder(control.label.as_str())
                        .description(control.description.as_str())
                        .control(spin);
                    section = section.add(
                        widget::column::with_capacity(8)
                            .spacing(12)
                            .push(heading)
                            .push(slider),
                    );
                }
            }
        }
        let mut actions = widget::row::with_capacity(3);
        if effect.pack_id.is_some() {
            actions =
                actions.push(button::destructive("Remove effect").on_press(Message::RemoveEffect));
        }
        actions = actions
            .push(iced::widget::space::horizontal())
            .push(button::standard("Reset controls").on_press(Message::Reset));
        widget::column::with_capacity(8)
            .spacing(18)
            .push(text::title2(effect.name.as_str()))
            .push(text::body(effect.tagline.as_str()))
            .push(text::body(effect.description.as_str()))
            .push(section)
            .push(actions)
            .into()
    }
    fn settings_view(&self) -> Element<'_, Message> {
        let effects = settings::section().title("Effects").add(
            widget::column::with_capacity(6)
                .spacing(16)
                .push(text::body(
                    "Import glass effects and keep your shader packs up to date.",
                ))
                .push(button::standard("Import effect").on_press(Message::Import))
                .push(text::caption(
                    "Select a JSON pack, or select both a .frag file and its JSON settings.",
                ))
                .push(button::standard("Open effects folder").on_press(Message::Folder)),
        );
        widget::column::with_capacity(5)
            .spacing(24)
            .push(text::title2("Settings"))
            .push(effects)
            .push(
                settings::section().title("Engine").add(
                    widget::column::with_capacity(2)
                        .spacing(24)
                        .push(self.engine_component_view(engine::Component::Compositor))
                        .push(self.engine_component_view(engine::Component::Panel)),
                ),
            )
            .into()
    }
}

impl cosmic::Application for Forge {
    type Executor = cosmic::executor::Default;
    type Flags = Flags;
    type Message = Message;
    const APP_ID: &'static str = APP_ID;
    fn core(&self) -> &Core {
        &self.core
    }
    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }
    fn init(core: Core, _: Flags) -> (Self, Task<Message>) {
        let folder = xdg("XDG_DATA_HOME", ".local/share").join("cosmic-forge/effects");
        let (effects, mut error) = match catalog::load(&folder) {
            Ok(effects) => (effects, None),
            Err(error) => (
                serde_json::from_str(include_str!("../shaders/effects.json"))
                    .expect("Built-in catalog"),
                Some(format!("Could not load extra effects: {error:#}")),
            ),
        };
        let mut store = Store::default();
        if let Some(effect) = effects
            .iter()
            .find(|e| e.id == store.selected() && e.shader.is_some() && store::effect_available(e))
            && let Err(e) = store.select_effect(effect)
        {
            error = Some(format!("Could not activate glass pack: {e:#}"));
        }
        let state = store.fingerprint(&effects);
        let index = effects
            .iter()
            .position(|e| e.id == state.selected)
            .unwrap_or(0);
        let mut nav = nav_bar::Model::default();
        for (index, effect) in effects.iter().enumerate() {
            nav.insert()
                .text(effect.name.clone())
                .data(Page::Effect(index));
        }
        nav.activate_position(index as u16);
        let panel_nav = nav_bar::Model::default();

        let mut lock_nav = nav_bar::Model::default();
        lock_nav.insert().text("Lock screen mods");
        let (status, engine_note) = store::engine_status();
        let needs_engine = !store::effect_available(&effects[index]);
        let mut app = Self {
            core,
            nav,
            panel_nav,
            lock_nav,
            effects,
            page: Page::Effect(index),
            settings_return: Page::Effect(index),
            store,
            state,
            pending: BTreeMap::new(),
            changed: Instant::now(),
            polled: Instant::now(),
            error,
            about: false,
            needs_engine,
            engine_updates: Vec::new(),
            show_engine_update: false,
            theme_dirty: false,
            status,
            engine_note,
            transfer: None,
            engine_progress: None,
            renderer_note: String::new(),
            engine_component: engine::Component::Compositor,
            panel_note: String::new(),
            panel_revision: panel::installed_revision().unwrap_or(0),
            mods: Vec::new(),
            show_updates: false,
            panel_names: Vec::new(),
            panel_index: 0,
            panel_mode: usize::from(store::Store::default().dark()),
            panel_profile: None,
            color_picker: widget::color_picker::Model::new("Hex", "RGB", None, None),
            update_note: String::new(),
            checked: Instant::now(),
            catalog_stamp: packs::fingerprint(&packs::root()),
        };
        app.reload_mods();
        app.reload_panel(true);
        app.set_header_title("COSMIC Forge".into());
        if packs::due(&packs::root()) || mods::due(&mods::root()) {
            app.check_updates();
        }
        let task = app.set_window_title("COSMIC Forge".into(), app.core.main_window_id().unwrap());
        (app, task)
    }
    fn nav_model(&self) -> Option<&nav_bar::Model> {
        Some(&self.nav)
    }
    fn nav_bar(&self) -> Option<Element<'_, cosmic::Action<Message>>> {
        if !self.core.nav_bar_active() {
            return None;
        }
        fn group<'a>(
            title: &'static str,
            model: &'a nav_bar::Model,
            select: fn(nav_bar::Id) -> cosmic::Action<Message>,
        ) -> Element<'a, cosmic::Action<Message>> {
            widget::column::with_capacity(2)
                .spacing(4)
                .push(widget::container(text::heading(title).size(16)).padding([8, 16]))
                .push(
                    widget::segmented_button::vertical(model)
                        .button_height(32)
                        .button_padding([8, 24, 8, 24])
                        .button_spacing(8)
                        .spacing(8)
                        .style(cosmic::theme::SegmentedButton::NavBar)
                        .on_activate(select),
                )
                .into()
        }
        let groups = widget::column::with_capacity(3)
            .spacing(16)
            .push(group("Shaders", &self.nav, |id| {
                cosmic::Action::Cosmic(cosmic::app::Action::NavBar(id))
            }))
            .push(group("Panel & Dock", &self.panel_nav, |id| {
                cosmic::Action::App(Message::Panel(id))
            }))
            .push(group("Lock screen", &self.lock_nav, |_| {
                cosmic::Action::App(Message::LockScreen)
            }));
        let mut sidebar = widget::container(widget::scrollable(groups).height(Length::Fill))
            .padding(8)
            .width(Length::Shrink)
            .height(Length::Fill)
            .class(cosmic::theme::Container::custom(nav_bar::nav_bar_style));
        if !self.core.is_condensed() {
            sidebar = sidebar.max_width(280);
        }
        Some(sidebar.into())
    }
    fn on_nav_select(&mut self, id: nav_bar::Id) -> Task<Message> {
        if !self.flush() {
            return Task::none();
        }
        if let Some(page) = self.nav.data::<Page>(id).copied() {
            if let Page::Effect(index) = page {
                if !store::effect_available(&self.effects[index]) {
                    self.needs_engine = true;
                    return Task::none();
                }
                let result = self.store.select_effect(&self.effects[index]);
                if !self.report(result) {
                    return Task::none();
                }
            }
            self.page = page;
            self.sync_selection();
        }
        Task::none()
    }
    fn header_end(&self) -> Vec<Element<'_, Message>> {
        let mut update = button::icon(widget::icon::from_name("view-refresh-symbolic"));
        if self.transfer.is_none() {
            update = update.on_press(Message::CheckUpdates);
        }
        vec![
            update.into(),
            button::icon(widget::icon::from_name("preferences-system-symbolic"))
                .on_press(Message::Settings)
                .into(),
            button::icon(widget::icon::from_name("help-about-symbolic"))
                .on_press(Message::About(true))
                .into(),
        ]
    }
    fn view(&self) -> Element<'_, Message> {
        let content = match self.page {
            Page::Effect(i) => self.effect_view(i),
            Page::Panel(index) => self.panel_view(index),
            Page::LockScreen => self.mods_view("Lock screen", "Mods for your COSMIC lock screen."),
            Page::Settings => self.settings_view(),
        };
        widget::scrollable(
            widget::container(content)
                .max_width(880)
                .padding(24)
                .width(Length::Fill),
        )
        .height(Length::Fill)
        .into()
    }
    fn footer(&self) -> Option<Element<'_, Message>> {
        if self.error.is_none()
            && self.show_updates
            && (self.transfer.is_some() || self.checked.elapsed() < Duration::from_secs(20))
            && !self.update_note.is_empty()
        {
            return Some(
                widget::container(text::caption(self.update_note.as_str()))
                    .padding([10, 24])
                    .width(Length::Fill)
                    .into(),
            );
        }
        if self.error.is_none() && !self.engine_updates.is_empty() && !self.show_engine_update {
            return Some(
                widget::container(
                    widget::row::with_capacity(2)
                        .spacing(16)
                        .align_y(iced::Alignment::Center)
                        .push(text::caption("Engine update available"))
                        .push(button::standard("Go to settings").on_press(Message::EngineSettings)),
                )
                .padding([10, 24])
                .width(Length::Fill)
                .into(),
            );
        }
        if !matches!(self.page, Page::Effect(_)) && self.error.is_none() {
            return None;
        }
        let runtime_note = (matches!(self.page, Page::Effect(index) if self.effects[index].shader.is_some()) && !packs::engine_active())
            .then_some("This glass pack needs Forge renderer ABI 1. Install the new engine and sign in again.");
        let native =
            matches!(self.page, Page::Effect(index) if self.effects[index].id == "frosted");
        let message = self
            .error
            .as_deref()
            .or(if native {
                None
            } else {
                runtime_note.or(self.engine_note)
            })
            .unwrap_or(if native {
                "Live on your desktop"
            } else {
                self.status
            });
        Some(
            widget::container(text::caption(message))
                .padding([10, 24])
                .width(Length::Fill)
                .into(),
        )
    }
    fn dialog(&self) -> Option<Element<'_, Message>> {
        if self.needs_engine {
            return Some(widget::dialog().title("Glass compositor required")
                .body("To use this effect, install or update the glass compositor from Settings, then sign out and back in.")
                .primary_action(button::suggested("Go to settings").on_press(Message::EngineSettings))
                .secondary_action(button::standard("Cancel").on_press(Message::DismissEngine)).into());
        }
        if self.show_engine_update && !self.engine_updates.is_empty() {
            let components = self
                .engine_updates
                .iter()
                .map(|c| c.label())
                .collect::<Vec<_>>()
                .join(", ");
            return Some(widget::dialog().title("Engine update available").body(format!("An engine update is available for {components}. Install / update it in Settings, then sign out and back in to activate it."))
                .primary_action(button::suggested("Go to settings").on_press(Message::EngineSettings))
                .secondary_action(button::standard("Later").on_press(Message::DismissEngineUpdate)).into());
        }
        self.about.then(|| {
            widget::dialog()
                .title("COSMIC Forge")
                .body(format!("Version {VERSION}"))
                .control(
                    button::link("https://github.com/motixo/cosmic-forge")
                        .on_press(Message::Repository),
                )
                .control(text::caption("GPL-3.0-only"))
                .primary_action(button::suggested("Close").on_press(Message::About(false)))
                .into()
        })
    }
    fn on_escape(&mut self) -> Task<Message> {
        self.about = false;
        self.needs_engine = false;
        self.show_engine_update = false;
        Task::none()
    }
    fn on_app_exit(&mut self) -> Option<Message> {
        Some(Message::Exit)
    }
    fn on_close_requested(&self, _: iced::window::Id) -> Option<Message> {
        Some(Message::Exit)
    }
    fn subscription(&self) -> Subscription<Message> {
        Subscription::batch([
            iced::time::every(Duration::from_millis(100)).map(|_| Message::Tick),
            iced::event::listen_with(|event, status, _| {
                if status == iced::event::Status::Captured {
                    return None;
                }
                let iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                    key,
                    physical_key,
                    modifiers,
                    ..
                }) = event
                else {
                    return None;
                };
                if modifiers.command() && !modifiers.shift() && !modifiers.alt() {
                    match key.to_latin(physical_key) {
                        Some('q') => Some(Message::Exit),
                        _ => None,
                    }
                } else {
                    None
                }
            }),
        ])
    }
    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::DismissEngine => self.needs_engine = false,
            Message::DismissEngineUpdate => self.show_engine_update = false,
            Message::Repository => {
                let result = std::process::Command::new("xdg-open")
                    .arg("https://github.com/motixo/cosmic-forge")
                    .spawn()
                    .map(|_| ())
                    .map_err(Into::into);
                self.report(result);
            }
            Message::EngineSettings => {
                if self.flush() {
                    self.needs_engine = false;
                    self.show_engine_update = false;
                    self.show_updates = false;
                    if !matches!(self.page, Page::Settings) {
                        self.settings_return = self.page;
                    }
                    self.page = Page::Settings;
                }
            }
            Message::Value(index, value) => {
                if let Page::Effect(effect) = self.page {
                    let control = self.effects[effect].controls[index].clone();
                    self.pending.insert(control.key.clone(), (control, value));
                    self.changed = Instant::now();
                }
            }
            Message::Flush => {
                self.flush();
            }
            Message::Tick => {
                let events = self
                    .transfer
                    .as_ref()
                    .map(|r| r.try_iter().collect::<Vec<_>>())
                    .unwrap_or_default();
                for event in events {
                    let result = match event {
                        TransferEvent::EngineUpdates(updates) => {
                            self.show_engine_update = self.show_updates && !updates.is_empty();
                            self.engine_updates = updates;
                            continue;
                        }
                        TransferEvent::Progress(progress) => {
                            self.engine_progress = Some(progress);
                            continue;
                        }
                        TransferEvent::Finished(result) => result,
                    };
                    self.transfer = None;
                    if self.engine_progress.is_none() {
                        self.checked = Instant::now();
                    }
                    match result {
                        Ok(note) => {
                            if self.engine_progress.is_some() {
                                self.engine_updates.retain(|c| *c != self.engine_component);
                            }
                            self.transfer_note(note);
                            self.panel_revision = panel::installed_revision().unwrap_or(0);
                            self.reload_panel(true);
                            self.reload_mods();
                            (self.status, self.engine_note) = store::engine_status();
                            if let Err(e) = self.reload_effects() {
                                self.error = Some(format!("{e:#}"));
                            }
                        }
                        Err(note) => {
                            self.transfer_note(note);
                            self.engine_progress = None;
                        }
                    }
                }
                if self.checked.elapsed() >= Duration::from_secs(packs::CHECK_INTERVAL) {
                    self.check_updates();
                }
                if !self.pending.is_empty() && self.changed.elapsed() >= Duration::from_millis(180)
                {
                    self.flush();
                }
                if self.polled.elapsed() >= Duration::from_millis(900) && self.pending.is_empty() {
                    if packs::fingerprint(&packs::root()) != self.catalog_stamp
                        && let Err(e) = self.reload_effects()
                    {
                        self.catalog_stamp = packs::fingerprint(&packs::root());
                        self.error = Some(format!("Could not reload effects: {e:#}"));
                    }
                    let state = self.store.fingerprint(&self.effects);
                    if state != self.state {
                        let theme_changed = state.dark != self.state.dark
                            || state.accent != self.state.accent
                            || state.material != self.state.material
                            || state.categories != self.state.categories;
                        self.state = state;
                        self.sync_selection();
                        self.theme_dirty |= theme_changed;
                    }
                    self.reload_mods();
                    if !self.color_picker.get_is_active() {
                        self.reload_panel(false);
                    }
                    self.polled = Instant::now();
                }
            }
            Message::Settings => {
                if self.flush() {
                    self.page = match self.page {
                        Page::Settings => self.settings_return,
                        page => {
                            self.settings_return = page;
                            Page::Settings
                        }
                    };
                    self.sync_selection();
                }
            }
            Message::Panel(id) => {
                if self.flush()
                    && let Some(page) = self.panel_nav.data::<Page>(id).copied()
                {
                    self.page = page;
                    self.reload_panel(true);
                    self.sync_selection();
                }
            }
            Message::LockScreen => {
                if self.flush() {
                    self.page = Page::LockScreen;
                    self.sync_selection();
                }
            }
            Message::PanelProfile(index) => {
                if index < self.panel_names.len() {
                    self.panel_index = index;
                    self.reload_panel(true);
                }
            }
            Message::PanelMode(index) => {
                if index < 2 {
                    self.panel_mode = index;
                    self.reload_panel(true);
                }
            }
            Message::PanelColor(update) => {
                use widget::color_picker::ColorPickerUpdate as U;
                let reset = matches!(update, U::Reset);
                let apply = matches!(update, U::AppliedColor | U::ActionFinished);
                let task = self.color_picker.update::<Message>(update);
                if reset {
                    self.save_panel_color(None);
                } else if apply && let Some(c) = self.color_picker.get_applied_color() {
                    self.save_panel_color(Some([c.r, c.g, c.b]));
                }
                return task.map(cosmic::Action::App);
            }
            Message::MatchPanelTheme => self.save_panel_color(None),
            Message::ModValue(index, value) => {
                if let Page::Panel(m) = self.page
                    && let Some(c) = self.mods[m].controls.get(index)
                {
                    self.set_mod_values(BTreeMap::from([(c.key.clone(), value)]));
                }
            }
            Message::ModPreset => {
                if let Page::Panel(m) = self.page {
                    self.set_mod_values(self.mods[m].preset.clone());
                }
            }
            Message::Engine(component, action) => self.start_component(component, action),
            Message::Reset => {
                if self.flush()
                    && let Page::Effect(index) = self.page
                {
                    let result = self.store.reset(&self.effects[index]);
                    self.report(result);
                }
            }
            Message::About(show) => self.about = show,
            Message::Folder => {
                let folder = packs::root().join("packs");
                let result = std::fs::create_dir_all(&folder)
                    .and_then(|_| {
                        std::process::Command::new("xdg-open")
                            .arg(folder)
                            .spawn()
                            .map(|_| ())
                    })
                    .map_err(Into::into);
                self.report(result);
            }
            Message::CheckUpdates => {
                self.show_updates = true;
                self.check_updates();
            }
            Message::Import => {
                return cosmic::task::future(async {
                    use cosmic::dialog::file_chooser;
                    match file_chooser::open::Dialog::new()
                        .title("Select a JSON pack or a .frag + JSON pair")
                        .filter(
                            file_chooser::FileFilter::new("Glass effect files")
                                .glob("*.json")
                                .glob("*.frag"),
                        )
                        .open_files()
                        .await
                    {
                        Ok(files) => Message::Imported(Some(
                            files
                                .urls()
                                .iter()
                                .map(|url| {
                                    url.to_file_path()
                                        .map_err(|_| "Select local JSON and .frag files".into())
                                })
                                .collect(),
                        )),
                        Err(file_chooser::Error::Cancelled) => Message::Imported(None),
                        Err(e) => Message::Imported(Some(Err(e.to_string()))),
                    }
                });
            }
            Message::Imported(Some(result)) => {
                match result.and_then(|paths| {
                    packs::import_files(&packs::root(), &paths).map_err(|e| format!("{e:#}"))
                }) {
                    Ok(note) => {
                        self.error = None;
                        self.update_note = note;
                        if let Err(e) = self.reload_effects() {
                            self.error = Some(format!("{e:#}"));
                        }
                    }
                    Err(e) => self.error = Some(e),
                }
            }
            Message::Imported(None) => {}
            Message::RemoveEffect => {
                if self.flush() {
                    let result = self.remove_effect();
                    self.report(result);
                }
            }
            Message::Exit => {
                if self.flush() {
                    return iced::exit();
                }
            }
        }
        if std::mem::take(&mut self.theme_dirty) {
            return Task::done(cosmic::Action::Cosmic(cosmic::app::Action::AppThemeChange(
                cosmic::theme::system_preference(),
            )));
        }
        Task::none()
    }
}
