// SPDX-License-Identifier: GPL-3.0-only
use crate::catalog::{Control, ControlKind, Effect, valid_identifier};
use crate::runtime::{ABI, Parameter, RuntimeEffect};
use anyhow::{Context, Result, bail};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    env, fs,
    io::{ErrorKind, Write},
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub const BLUR_NAMES: [&str; 14] = [
    "ExtremelyLow",
    "ExtremelyLow2",
    "VeryLow",
    "VeryLow2",
    "Low",
    "Low2",
    "Medium",
    "Medium2",
    "High",
    "High2",
    "VeryHigh",
    "VeryHigh2",
    "ExtremelyHigh",
    "ExtremelyHigh2",
];
const CATEGORIES: [&str; 4] = [
    "frosted_windows",
    "frosted_panel",
    "frosted_applets",
    "frosted_system_interface",
];
pub type Changes = BTreeMap<PathBuf, Option<Vec<u8>>>;

pub fn xdg(variable: &str, fallback: &str) -> PathBuf {
    env::var_os(variable)
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| PathBuf::from(env::var_os("HOME").unwrap_or_default()).join(fallback))
}

#[derive(Debug, Clone)]
struct Snapshot {
    data: Option<Vec<u8>>,
    mode: u32,
}
#[derive(Debug, Serialize)]
struct BackupEntry {
    data: Option<String>,
    mode: u32,
}
#[derive(Debug)]
struct Edit {
    before: BTreeMap<PathBuf, Snapshot>,
    after: Changes,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fingerprint {
    pub selected: String,
    pub dark: bool,
    pub accent: String,
    pub values: Vec<Value>,
    pub categories: Vec<bool>,
    pub material: [i32; 2],
}

#[derive(Debug)]
pub struct Store {
    pub root: PathBuf,
    pub comp: PathBuf,
    pub backup: PathBuf,
    history: Vec<Edit>,
    originals: BTreeMap<PathBuf, BackupEntry>,
}

impl Default for Store {
    fn default() -> Self {
        Self::new(
            xdg("XDG_CONFIG_HOME", ".config"),
            xdg("XDG_STATE_HOME", ".local/state"),
        )
    }
}

impl Store {
    pub fn new(root: PathBuf, state_home: PathBuf) -> Self {
        let state = state_home.join("cosmic-forge");
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let backup = state
            .join("backups")
            .join(format!("{stamp}-{}.json", std::process::id()));
        Self {
            comp: root.join("cosmic/com.system76.CosmicComp/v1"),
            root,
            backup,
            history: Vec::new(),
            originals: BTreeMap::new(),
        }
    }

    pub fn text(path: &Path, fallback: &str) -> String {
        fs::read_to_string(path)
            .map(|v| v.trim().to_owned())
            .unwrap_or_else(|_| fallback.to_owned())
    }

    pub fn selected(&self) -> String {
        if let Ok(Value::String(name)) =
            serde_json::from_str::<Value>(&Self::text(&self.comp.join("glass_effect"), ""))
            && valid_identifier(&name)
        {
            return name;
        }
        if Self::text(&self.comp.join("crystal_enabled"), "false") == "true" {
            "crystal".into()
        } else {
            "frosted".into()
        }
    }

    pub fn dark(&self) -> bool {
        Self::text(
            &self
                .root
                .join("cosmic/com.system76.CosmicTheme.Mode/v1/is_dark"),
            "true",
        ) != "false"
    }
    pub fn theme_path(&self, key: &str) -> PathBuf {
        let mode = if self.dark() { "Dark" } else { "Light" };
        self.root
            .join(format!("cosmic/com.system76.CosmicTheme.{mode}/v2/{key}"))
    }
    pub fn accent(&self) -> String {
        let data = Self::text(&self.theme_path("accent"), "");
        if let Some((_, value)) = data.split_once("base:")
            && let Some(value) = value.trim_start().strip_prefix("\"#")
            && value.len() >= 6
            && value.as_bytes()[..6].iter().all(u8::is_ascii_hexdigit)
        {
            return format!("#{}", &value[..6]);
        }
        "#63cbd8".into()
    }

    pub fn value(&self, control: &Control) -> Value {
        if control.target == "theme" && control.key == "alpha_map" {
            return json!(crate::material::opacity(self));
        }
        if control.target == "theme" {
            let name = Self::text(&self.theme_path("frosted"), "Medium");
            return BLUR_NAMES
                .iter()
                .position(|item| *item == name)
                .map_or_else(|| control.default.clone(), |v| json!(v));
        }
        let data = Self::text(&self.comp.join(&control.key), "");
        if data.is_empty() && control.legacy.as_deref() == Some("crystal_blur") {
            let old = Self::text(&self.comp.join("crystal_blur"), "3")
                .parse::<u16>()
                .unwrap_or(3)
                .min(13);
            return json!((u32::from(old) * 100 + 6) / 13);
        }
        match (control.kind, serde_json::from_str::<Value>(&data)) {
            (ControlKind::Boolean, Ok(Value::Bool(v))) => json!(v),
            (ControlKind::Integer, Ok(value)) if value.as_i64().is_some() => json!(
                value
                    .as_i64()
                    .unwrap()
                    .clamp(i64::from(control.min), i64::from(control.max))
            ),
            _ => control.default.clone(),
        }
    }

    fn category(&self, key: &str) -> bool {
        Self::text(&self.theme_path(key), "false") == "true"
    }

    fn snapshot(path: &Path) -> Result<Snapshot> {
        match fs::read(path) {
            Ok(data) => Ok(Snapshot {
                data: Some(data),
                mode: fs::metadata(path)?.permissions().mode() & 0o777,
            }),
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(Snapshot {
                data: None,
                mode: 0o600,
            }),
            Err(e) => Err(e).with_context(|| format!("Could not read {}", path.display())),
        }
    }

    fn atomic(path: &Path, data: &[u8], mode: u32) -> Result<()> {
        let parent = path
            .parent()
            .context("A config file needs a parent directory")?;
        fs::create_dir_all(parent)?;
        let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
        temporary.write_all(data)?;
        temporary.as_file().sync_all()?;
        temporary
            .as_file()
            .set_permissions(fs::Permissions::from_mode(mode))?;
        temporary.persist(path).map_err(|e| e.error)?;
        Ok(())
    }

    fn write(path: &Path, value: &Option<Vec<u8>>, mode: u32) -> Result<()> {
        if let Some(data) = value {
            Self::atomic(path, data, mode)
        } else {
            match fs::remove_file(path) {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
                Err(e) => Err(e.into()),
            }
        }
    }

    pub fn commit(&mut self, mut changes: Changes) -> Result<bool> {
        let mut before = BTreeMap::new();
        for path in changes.keys() {
            before.insert(path.clone(), Self::snapshot(path)?);
        }
        changes.retain(|path, value| before[path].data != *value);
        if changes.is_empty() {
            return Ok(false);
        }
        before.retain(|path, _| changes.contains_key(path));
        for (path, snapshot) in &before {
            self.originals
                .entry(path.clone())
                .or_insert_with(|| BackupEntry {
                    data: snapshot.data.as_ref().map(|data| STANDARD.encode(data)),
                    mode: snapshot.mode,
                });
        }
        Self::atomic(
            &self.backup,
            &serde_json::to_vec_pretty(&self.originals)?,
            0o600,
        )?;
        let mut completed = Vec::new();
        for (path, value) in &changes {
            if let Err(error) = Self::write(path, value, before[path].mode) {
                for done in completed.into_iter().rev() {
                    let snapshot: &Snapshot = &before[done];
                    Self::write(done, &snapshot.data, snapshot.mode)
                        .context("Could not roll back an incomplete preference change")?;
                }
                return Err(error).with_context(|| format!("Could not save {}", path.display()));
            }
            completed.push(path);
        }
        self.history.push(Edit {
            before,
            after: changes,
        });
        if self.history.len() > 100 {
            self.history.remove(0);
        }
        Ok(true)
    }

    pub fn select_effect(&mut self, effect: &Effect) -> Result<bool> {
        effect.validate()?;
        let mut changes = Changes::from([(
            self.comp.join("glass_effect"),
            Some(format!("{}\n", serde_json::to_string(&effect.id)?).into_bytes()),
        )]);
        let runtime = effect.shader.as_ref().map(|shader| RuntimeEffect {
            abi: ABI,
            id: effect.id.clone(),
            base: shader.base.clone(),
            source: shader.source.clone(),
            animated: shader.animated,
            parameters: shader
                .uniforms
                .iter()
                .map(|(name, key)| {
                    let control = effect
                        .controls
                        .iter()
                        .find(|c| &c.key == key)
                        .expect("Validated uniform");
                    let value = self.value(control);
                    Parameter {
                        key: key.clone(),
                        name: name.clone(),
                        value: numeric(&value),
                    }
                })
                .collect(),
        });
        changes.insert(
            self.comp.join("forge_runtime"),
            runtime
                .map(|r| ron::to_string(&r).map(String::into_bytes))
                .transpose()?,
        );
        self.commit(changes)
    }

    pub fn set_control(&mut self, control: &Control, value: &Value) -> Result<bool> {
        self.commit(self.controls_changes([(control, value)])?)
    }
    pub fn reset(&mut self, effect: &Effect) -> Result<bool> {
        let changes = self.controls_changes(effect.controls.iter().map(|c| (c, &c.default)))?;
        self.commit(changes)
    }
    pub fn controls_changes<'a>(
        &self,
        controls: impl IntoIterator<Item = (&'a Control, &'a Value)>,
    ) -> Result<Changes> {
        let mut changes = Changes::new();
        let mut runtime = fs::read_to_string(self.comp.join("forge_runtime"))
            .ok()
            .and_then(|s| ron::from_str::<RuntimeEffect>(&s).ok())
            .filter(|r| r.valid() && r.id == self.selected());
        let mut runtime_changed = false;
        let (mut thickness, mut opacity) = (None, None);
        for (control, value) in controls {
            control.validate()?;
            let encoded = match control.kind {
                ControlKind::Boolean => json!(value.as_bool().context("Expected a boolean")?),
                ControlKind::Integer => json!(
                    value
                        .as_i64()
                        .context("Expected an integer")?
                        .clamp(i64::from(control.min), i64::from(control.max))
                ),
            };
            if control.target == "theme" {
                let amount = encoded.as_i64().unwrap() as i32;
                if control.key == "alpha_map" {
                    opacity = Some(amount);
                } else {
                    thickness = Some(amount);
                }
            } else {
                if let Some(runtime) = &mut runtime {
                    for parameter in &mut runtime.parameters {
                        if parameter.key == control.key {
                            parameter.value = numeric(&encoded);
                            runtime_changed = true;
                        }
                    }
                }
                changes.insert(
                    self.comp.join(&control.key),
                    Some(format!("{encoded}\n").into_bytes()),
                );
            }
        }
        if runtime_changed {
            changes.insert(
                self.comp.join("forge_runtime"),
                Some(ron::to_string(&runtime.unwrap())?.into_bytes()),
            );
        }
        if thickness.is_some() || opacity.is_some() {
            changes.extend(crate::material::changes(self, thickness, opacity)?);
        }
        Ok(changes)
    }
    pub fn undo(&mut self) -> Result<bool> {
        let Some(edit) = self.history.last() else {
            return Ok(false);
        };
        for (path, expected) in &edit.after {
            if Self::snapshot(path)?.data != *expected {
                bail!("This setting changed in another app and was left unchanged.");
            }
        }
        let mut completed = Vec::new();
        for (path, snapshot) in &edit.before {
            if let Err(error) = Self::write(path, &snapshot.data, snapshot.mode) {
                for done in completed.into_iter().rev() {
                    Self::write(done, &edit.after[done], edit.before[done].mode)?;
                }
                return Err(error).context("Could not undo the last change");
            }
            completed.push(path);
        }
        self.history.pop();
        Ok(true)
    }

    pub fn fingerprint(&self, effects: &[Effect]) -> Fingerprint {
        Fingerprint {
            material: [
                crate::material::strength(self),
                crate::material::opacity(self),
            ],
            selected: self.selected(),
            dark: self.dark(),
            accent: self.accent(),
            values: effects
                .iter()
                .flat_map(|e| e.controls.iter().map(|c| self.value(c)))
                .collect(),
            categories: CATEGORIES.iter().map(|c| self.category(c)).collect(),
        }
    }
}

fn numeric(value: &Value) -> f32 {
    value.as_bool().map_or_else(
        || value.as_i64().unwrap_or(0) as f32,
        |b| if b { 1.0 } else { 0.0 },
    )
}

fn installed_engine(state: &Path, launcher: &Path, minimum_revision: u64) -> Option<PathBuf> {
    let forge = state.join("cosmic-forge/engine-installation.json");
    let is_forge = forge.is_file();
    if minimum_revision > 0 && !is_forge || launcher.is_symlink() {
        return None;
    }
    let record = if is_forge {
        forge
    } else {
        state.join("cosmic-crystal/installation.json")
    };
    let record: Value = serde_json::from_slice(&fs::read(record).ok()?).ok()?;
    let current = fs::read(launcher).ok()?;
    if is_forge {
        let digest: String = Sha256::digest(&current)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        if record["renderer_revision"].as_u64().unwrap_or(1) < minimum_revision
            || record["abi"] != 1
            || record["launcher"].as_str()? != launcher.to_str()?
            || record["launcher_sha256"].as_str()? != digest
        {
            return None;
        }
    } else if current != record["shims"]["cosmic-comp"].as_str()?.as_bytes() {
        return None;
    }
    let binary = Path::new(record["destination"].as_str()?).join("cosmic-comp");
    let metadata = fs::metadata(&binary).ok()?;
    (metadata.is_file() && metadata.permissions().mode() & 0o111 != 0 && !binary.is_symlink())
        .then_some(binary)
}

pub fn effect_available(effect: &Effect) -> bool {
    effect.id == "frosted"
        || installed_engine(
            &xdg("XDG_STATE_HOME", ".local/state"),
            &xdg("HOME", "").join(".local/bin/cosmic-comp"),
            effect
                .shader
                .as_ref()
                .map_or(0, |shader| shader.renderer_revision()),
        )
        .is_some()
}

pub fn engine_status() -> (&'static str, Option<&'static str>) {
    if let Some(installed) = installed_engine(
        &xdg("XDG_STATE_HOME", ".local/state"),
        &xdg("HOME", "").join(".local/bin/cosmic-comp"),
        0,
    ) {
        if let (Ok(installed), Ok(own), Ok(processes)) = (
            fs::metadata(installed),
            fs::metadata("/proc/self"),
            fs::read_dir("/proc"),
        ) {
            for process in processes.flatten() {
                let path = process.path();
                if path
                    .file_name()
                    .is_none_or(|p| !p.to_string_lossy().bytes().all(|b| b.is_ascii_digit()))
                {
                    continue;
                }
                if fs::metadata(&path).is_ok_and(|metadata| metadata.uid() == own.uid())
                    && Store::text(&path.join("comm"), "") == "cosmic-comp"
                    && let Ok(running) = fs::metadata(path.join("exe"))
                    && running.ino() == installed.ino()
                    && running.dev() == installed.dev()
                {
                    return ("Live on your desktop", None);
                }
            }
        }
        (
            "Ready for next login",
            Some("Sign out and back in once to activate the installed effects engine."),
        )
    } else {
        (
            "Desktop effects",
            Some("These controls require the COSMIC Crystal / Aero compositor."),
        )
    }
}

#[cfg(test)]
mod engine_tests {
    use super::*;

    #[test]
    fn availability_requires_a_matching_launcher_and_executable_renderer() {
        let temp = tempfile::tempdir().unwrap();
        let state = temp.path().join("state");
        let launcher = temp.path().join("cosmic-comp");
        let destination = temp.path().join("renderer");
        fs::create_dir_all(state.join("cosmic-forge")).unwrap();
        fs::create_dir_all(state.join("cosmic-crystal")).unwrap();
        fs::create_dir_all(&destination).unwrap();
        let binary = destination.join("cosmic-comp");
        fs::write(&binary, b"test renderer").unwrap();
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o755)).unwrap();
        let current = b"test launcher";
        fs::write(&launcher, current).unwrap();
        let legacy = state.join("cosmic-crystal/installation.json");
        fs::write(
            &legacy,
            serde_json::to_vec(&json!({
                "destination":destination, "shims":{"cosmic-comp":"test launcher"}
            }))
            .unwrap(),
        )
        .unwrap();
        assert_eq!(installed_engine(&state, &launcher, 0), Some(binary.clone()));
        assert!(installed_engine(&state, &launcher, 1).is_none());
        let digest: String = Sha256::digest(current)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        fs::write(
            state.join("cosmic-forge/engine-installation.json"),
            serde_json::to_vec(&json!({
                "abi":1, "destination":destination, "launcher":launcher, "launcher_sha256":digest
            }))
            .unwrap(),
        )
        .unwrap();
        assert_eq!(installed_engine(&state, &launcher, 1), Some(binary.clone()));
        assert!(installed_engine(&state, &launcher, 2).is_none());
        let record_path = state.join("cosmic-forge/engine-installation.json");
        let mut record: Value = serde_json::from_slice(&fs::read(&record_path).unwrap()).unwrap();
        record["renderer_revision"] = json!(2);
        fs::write(&record_path, serde_json::to_vec(&record).unwrap()).unwrap();
        assert_eq!(installed_engine(&state, &launcher, 2), Some(binary.clone()));
        assert!(installed_engine(&state, &launcher, 3).is_none());
        record["renderer_revision"] = json!(3);
        fs::write(&record_path, serde_json::to_vec(&record).unwrap()).unwrap();
        assert_eq!(installed_engine(&state, &launcher, 3), Some(binary.clone()));
        fs::write(&launcher, b"another compositor").unwrap();
        assert!(installed_engine(&state, &launcher, 0).is_none());
        fs::write(&launcher, current).unwrap();
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o600)).unwrap();
        assert!(installed_engine(&state, &launcher, 1).is_none());
        fs::remove_file(&binary).unwrap();
        assert!(installed_engine(&state, &launcher, 1).is_none());
        fs::remove_file(&launcher).unwrap();
        assert!(installed_engine(&state, &launcher, 0).is_none());
    }
}
