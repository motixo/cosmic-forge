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
    engine, packs,
    runtime::Backdrop,
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
    Settings,
}
#[derive(Clone, Debug)]
pub enum Message {
    Value(usize, Value),
    Backdrop(usize),
    Animation(bool),
    Flush,
    Engine(engine::Action),
    Settings,
    ApplyEffect,
    Reset,
    About(bool),
    Repository,
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

enum EngineEvent {
    Progress(engine::Progress),
    Finished(Result<String, String>),
}

pub struct Forge {
    core: Core,
    nav: nav_bar::Model,
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
    about_details: String,
    engine_update: bool,
    show_engine_update: bool,
    theme_dirty: bool,
    status: &'static str,
    engine_note: Option<&'static str>,
    updates: Option<std::sync::mpsc::Receiver<(String, Option<bool>)>>,
    engine_transfer: Option<std::sync::mpsc::Receiver<EngineEvent>>,
    engine_progress: Option<engine::Progress>,
    renderer_note: String,
    show_updates: bool,
    update_note: String,
    checked: Instant,
    catalog_stamp: u64,
}

impl Forge {
    fn backdrop_picker<'a>(&self, effect: &'a Effect) -> Element<'a, Message> {
        const SOURCES: [&str; 2] = ["Behind window", "Wallpaper only"];
        let selected = usize::from(self.store.backdrop(effect) == Backdrop::Wallpaper);
        settings::item::builder("Backdrop source")
            .control(widget::dropdown(
                &SOURCES,
                Some(selected),
                Message::Backdrop,
            ))
            .into()
    }
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
        if self.updates.is_some() || self.engine_transfer.is_some() {
            return;
        }
        self.engine_progress = None;
        self.checked = Instant::now();
        self.update_note = "Checking shaders and compositor…".into();
        let (sender, receiver) = std::sync::mpsc::channel();
        self.updates = Some(receiver);
        std::thread::spawn(move || {
            let effects = match packs::update(&packs::root()) {
                Ok(note) => format!("effects: {note}"),
                Err(e) => format!("effects: {e:#}"),
            };
            let (engine, available) = match engine::update_available() {
                Ok(available) => {
                    let note = if !available {
                        "Engine: no updates available"
                    } else {
                        "Engine update available · open Settings to update"
                    };
                    (note.to_owned(), Some(available))
                }
                Err(e) => (format!("Engine: {e:#}"), None),
            };
            let _ = sender.send((format!("{effects}\n{engine}"), available));
        });
    }
    fn start_engine(&mut self, action: engine::Action) {
        if self.engine_transfer.is_some() {
            return;
        }
        self.show_updates = false;
        self.show_engine_update = false;
        self.engine_update = false;
        self.engine_progress = Some(engine::Progress::Checking);
        self.renderer_note.clear();
        let (sender, receiver) = std::sync::mpsc::channel();
        self.engine_transfer = Some(receiver);
        std::thread::spawn(move || {
            let result = engine::setup(action, |progress| {
                let _ = sender.send(EngineEvent::Progress(progress));
            })
            .map_err(|e| format!("{e:#}"));
            let _ = sender.send(EngineEvent::Finished(result));
        });
    }
    fn reload_effects(&mut self) -> anyhow::Result<()> {
        let effects = catalog::load(&packs::root().join("effects"))?;
        if !self.flush() {
            anyhow::bail!("Could not save pending controls");
        }
        let remap = |page| match page {
            Page::Effect(index) => Page::Effect(
                effects
                    .iter()
                    .position(|e| e.id == self.effects[index].id)
                    .or_else(|| effects.iter().position(|e| e.id == self.store.selected()))
                    .unwrap_or(0),
            ),
            page => page,
        };
        self.page = remap(self.page);
        self.settings_return = remap(self.settings_return);
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
            self.store.commit(changes)
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
        let page = match self.page {
            Page::Settings => self.settings_return,
            page => page,
        };
        match page {
            Page::Effect(index) => self.nav.activate_position(index as u16),
            Page::Settings => false,
        };
    }
    fn compositor_view(&self) -> Element<'_, Message> {
        let install_label = "Install / update compositor";
        let restore_label = "Restore original COSMIC compositor";
        let mut install = button::standard(install_label);
        let mut restore = button::standard(restore_label);
        if self.engine_transfer.is_none() {
            install = install.on_press(Message::Engine(engine::Action::Install));
            restore = restore.on_press(Message::Engine(engine::Action::Restore));
        }
        let mut content = widget::column::with_capacity(6)
            .spacing(16)
            .push(text::heading("Compositor"))
            .push(install)
            .push(restore);
        if let Some(progress) = self.engine_progress {
            let mut bar = widget::progress_bar::linear::Linear::new()
                .width(Length::Fill)
                .girth(6.0);
            if progress.finished() {
                bar = bar.progress(1.0);
            }
            content = content.push(
                widget::column::with_capacity(2)
                    .spacing(10)
                    .push(text::body(progress.label()))
                    .push(bar),
            );
        } else if !self.renderer_note.is_empty() {
            content = content.push(text::caption(self.renderer_note.as_str()));
        }
        content.into()
    }
    fn effect_view(&self, index: usize) -> Element<'_, Message> {
        let effect = &self.effects[index];
        let mut content = widget::column::with_capacity(8)
            .spacing(18)
            .push(text::title2(effect.name.as_str()))
            .push(text::body(effect.tagline.as_str()))
            .push(text::body(effect.description.as_str()));
        let available = store::effect_available(effect);
        let backdrop_available = store::renderer_available(engine::WALLPAPER_RENDERER_REVISION);
        if !available || self.state.selected != effect.id {
            if backdrop_available {
                content = content.push(self.backdrop_picker(effect));
            }
            content = if available {
                content
                    .push(text::body(
                        "The compositor engine is ready. Apply this effect to start using it.",
                    ))
                    .push(button::suggested("Apply effect").on_press(Message::ApplyEffect))
            } else {
                content
                    .push(text::body("Install or update the compositor engine in Settings, then sign out and back in to use this effect."))
                    .push(button::suggested("Go to settings").on_press(Message::EngineSettings))
            };
            if effect.pack_id.is_some() {
                content = content
                    .push(button::destructive("Remove effect").on_press(Message::RemoveEffect));
            }
            return content.into();
        }
        let mut section = settings::section().title("Appearance");
        if effect.shader.as_ref().is_some_and(|s| s.animated) {
            section = section.add(
                settings::item::builder("Animation")
                    .toggler(self.store.animated(effect), Message::Animation),
            );
        }
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
        if backdrop_available {
            section = section.add(self.backdrop_picker(effect));
        }
        let mut actions = widget::row::with_capacity(3);
        if effect.pack_id.is_some() {
            actions =
                actions.push(button::destructive("Remove effect").on_press(Message::RemoveEffect));
        }
        actions = actions
            .push(iced::widget::space::horizontal())
            .push(button::standard("Reset controls").on_press(Message::Reset));
        content.push(section).push(actions).into()
    }
    fn settings_view(&self) -> Element<'_, Message> {
        let effects = settings::section().title("Effects").add(
            widget::column::with_capacity(6)
                .spacing(16)
                .push(text::body("Import glass effects."))
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
                settings::section().title("Patches").add(
                    widget::column::with_capacity(4)
                        .spacing(24)
                        .push(text::body("Requires a supported COSMIC release from 1.10 onward. Install the prebuilt compositor, then sign out and back in."))
                        .push(self.compositor_view()),
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
        let (status, engine_note) = store::engine_status();
        let mut app = Self {
            core,
            nav,
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
            about_details: String::new(),
            engine_update: false,
            show_engine_update: false,
            theme_dirty: false,
            status,
            engine_note,
            updates: None,
            engine_transfer: None,
            engine_progress: None,
            renderer_note: String::new(),
            show_updates: false,
            update_note: String::new(),
            checked: Instant::now(),
            catalog_stamp: packs::fingerprint(&packs::root()),
        };
        app.set_header_title("COSMIC Forge".into());
        if packs::due(&packs::root()) {
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
        let shaders = widget::column::with_capacity(2)
            .spacing(4)
            .push(widget::container(text::heading("Shaders").size(16)).padding([8, 16]))
            .push(
                widget::segmented_button::vertical(&self.nav)
                    .button_height(32)
                    .button_padding([8, 24, 8, 24])
                    .button_spacing(8)
                    .spacing(8)
                    .style(cosmic::theme::SegmentedButton::NavBar)
                    .on_activate(|id| cosmic::Action::Cosmic(cosmic::app::Action::NavBar(id))),
            );
        let mut sidebar = widget::container(widget::scrollable(shaders).height(Length::Fill))
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
            if let Page::Effect(index) = page
                && store::effect_available(&self.effects[index])
            {
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
        if self.updates.is_none() && self.engine_transfer.is_none() {
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
            && (self.updates.is_some() || self.checked.elapsed() < Duration::from_secs(20))
            && !self.update_note.is_empty()
        {
            return Some(
                widget::container(text::caption(self.update_note.as_str()))
                    .padding([10, 24])
                    .width(Length::Fill)
                    .into(),
            );
        }
        if self.error.is_none() && self.engine_update && !self.show_engine_update {
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
        if let Page::Effect(index) = self.page
            && self.error.is_none()
            && (!store::effect_available(&self.effects[index])
                || self.state.selected != self.effects[index].id)
        {
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
        if self.show_engine_update && self.engine_update {
            return Some(widget::dialog().title("Engine update available").body("A compositor update is available. Install / update it in Settings, then sign out and back in to activate it.")
                .primary_action(button::suggested("Go to settings").on_press(Message::EngineSettings))
                .secondary_action(button::standard("Later").on_press(Message::DismissEngineUpdate)).into());
        }
        self.about.then(|| {
            widget::dialog()
                .title("COSMIC Forge")
                .body(self.about_details.as_str())
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
                    self.show_engine_update = false;
                    self.show_updates = false;
                    if !matches!(self.page, Page::Settings) {
                        self.settings_return = self.page;
                    }
                    self.page = Page::Settings;
                }
            }
            Message::Value(index, value) => {
                if let Page::Effect(effect) = self.page
                    && store::effect_available(&self.effects[effect])
                    && self.state.selected == self.effects[effect].id
                {
                    let control = self.effects[effect].controls[index].clone();
                    self.pending.insert(control.key.clone(), (control, value));
                    self.changed = Instant::now();
                }
            }
            Message::Animation(animated) => {
                if self.flush()
                    && let Page::Effect(effect) = self.page
                    && store::effect_available(&self.effects[effect])
                    && self.store.selected() == self.effects[effect].id
                {
                    let result = self.store.set_animation(&self.effects[effect], animated);
                    self.report(result);
                }
            }
            Message::Backdrop(index) => {
                if self.flush()
                    && let Page::Effect(effect) = self.page
                {
                    let backdrop = if index == 1 {
                        Backdrop::Wallpaper
                    } else {
                        Backdrop::Scene
                    };
                    if backdrop == Backdrop::Wallpaper
                        && !store::renderer_available(engine::WALLPAPER_RENDERER_REVISION)
                    {
                        self.error = Some("Wallpaper backgrounds require the new compositor engine. Install / update it in Settings.".into());
                    } else {
                        let result = self.store.set_backdrop(&self.effects[effect], backdrop);
                        self.report(result);
                    }
                }
            }
            Message::Flush => {
                self.flush();
            }
            Message::Tick => {
                if let Some((note, available)) =
                    self.updates.as_ref().and_then(|r| r.try_recv().ok())
                {
                    self.updates = None;
                    self.update_note = note;
                    self.checked = Instant::now();
                    // A compositor action supersedes the check that was already running.
                    if self.engine_progress.is_none()
                        && let Some(available) = available
                    {
                        self.show_engine_update = self.show_updates && available;
                        self.engine_update = available;
                    }
                    (self.status, self.engine_note) = store::engine_status();
                    if let Err(e) = self.reload_effects() {
                        self.error = Some(format!("{e:#}"));
                    }
                }
                let events = self
                    .engine_transfer
                    .as_ref()
                    .map(|r| r.try_iter().collect::<Vec<_>>())
                    .unwrap_or_default();
                for event in events {
                    let result = match event {
                        EngineEvent::Progress(progress) => {
                            self.engine_progress = Some(progress);
                            continue;
                        }
                        EngineEvent::Finished(result) => result,
                    };
                    self.engine_transfer = None;
                    match result {
                        Ok(note) => {
                            self.engine_update = false;
                            self.renderer_note = note;
                            (self.status, self.engine_note) = store::engine_status();
                            if let Err(e) = self.reload_effects() {
                                self.error = Some(format!("{e:#}"));
                            }
                        }
                        Err(note) => {
                            self.renderer_note = note;
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
                        if state.selected != self.state.selected {
                            let index = self
                                .effects
                                .iter()
                                .position(|e| e.id == state.selected)
                                .unwrap_or(0);
                            if matches!(self.page, Page::Effect(_)) {
                                self.page = Page::Effect(index);
                            }
                            if matches!(self.settings_return, Page::Effect(_)) {
                                self.settings_return = Page::Effect(index);
                            }
                        }
                        let theme_changed = state.dark != self.state.dark
                            || state.accent != self.state.accent
                            || state.material != self.state.material
                            || state.categories != self.state.categories;
                        self.state = state;
                        self.sync_selection();
                        self.theme_dirty |= theme_changed;
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
            Message::Engine(action) => self.start_engine(action),
            Message::ApplyEffect => {
                if self.flush()
                    && let Page::Effect(index) = self.page
                    && store::effect_available(&self.effects[index])
                {
                    let result = self.store.select_effect(&self.effects[index]);
                    self.report(result);
                }
            }
            Message::Reset => {
                if self.flush()
                    && let Page::Effect(index) = self.page
                    && store::effect_available(&self.effects[index])
                    && self.state.selected == self.effects[index].id
                {
                    let result = self.store.reset(&self.effects[index]);
                    self.report(result);
                }
            }
            Message::About(show) => {
                if show {
                    let compositor = store::installed_compositor_version()
                        .unwrap_or_else(|| "Not installed".into());
                    let shaders =
                        packs::installed_release_tag(&packs::root()).unwrap_or_else(|| {
                            if packs::installed_revision(&packs::root()).is_some() {
                                "Unknown release".into()
                            } else {
                                "Bundled".into()
                            }
                        });
                    self.about_details =
                        format!("Version {VERSION}\nEngine: {compositor}\nShader pack: {shaders}");
                }
                self.about = show;
            }
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
