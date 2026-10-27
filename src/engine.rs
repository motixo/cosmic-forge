// SPDX-License-Identifier: GPL-3.0-only
use crate::store::xdg;
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    path::PathBuf,
    process::{Command, Stdio},
};

const FILES: &[(&str, &[u8])] = &[
    ("build.py", include_bytes!("../engine/build.py")),
    ("install.py", include_bytes!("../engine/install.py")),
    ("release.json", include_bytes!("../engine/release.json")),
    ("runtime.rs", include_bytes!("../engine/runtime.rs")),
    ("loader.rs", include_bytes!("../engine/loader.rs")),
    (
        "patches/glass.patch",
        include_bytes!("../engine/patches/glass.patch"),
    ),
    (
        "patches/runtime.patch",
        include_bytes!("../engine/patches/runtime.patch"),
    ),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Progress {
    Checking,
    Preparing,
    Building,
    Installing,
    Restoring,
    Complete,
    AlreadyInstalled,
}

impl Progress {
    fn from_line(line: &str) -> Option<Self> {
        Some(match line.strip_prefix("FORGE_PROGRESS:")? {
            "checking" => Self::Checking,
            "preparing" => Self::Preparing,
            "building" => Self::Building,
            "installing" => Self::Installing,
            "restoring" => Self::Restoring,
            "complete" => Self::Complete,
            "already_installed" => Self::AlreadyInstalled,
            _ => return None,
        })
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Checking => "Checking installed renderer…",
            Self::Preparing => "Downloading and patching compositor source…",
            Self::Building => "Compiling the glass renderer…",
            Self::Installing => "Installing the glass renderer…",
            Self::Restoring => "Restoring the original COSMIC compositor…",
            Self::Complete => "Finished · sign out and back in to activate",
            Self::AlreadyInstalled => "This renderer version is already installed",
        }
    }
    pub fn finished(self) -> bool {
        matches!(self, Self::Complete | Self::AlreadyInstalled)
    }
}

pub fn setup(restore: bool, mut progress: impl FnMut(Progress)) -> Result<String> {
    progress(Progress::Checking);
    let mut digest = Sha256::new();
    for (name, data) in FILES {
        digest.update(name);
        digest.update(data);
    }
    let hash: String = digest
        .finalize()
        .iter()
        .take(8)
        .map(|b| format!("{b:02x}"))
        .collect();
    let root = xdg("XDG_CACHE_HOME", ".cache")
        .join("cosmic-forge/renderer")
        .join(hash);
    for (name, data) in FILES {
        let path = root.join(name);
        if fs::read(&path).is_ok_and(|existing| existing == *data) {
            continue;
        }
        fs::create_dir_all(path.parent().context("Missing engine directory")?)?;
        let mut file = tempfile::NamedTempFile::new_in(path.parent().unwrap())?;
        file.write_all(data)?;
        file.as_file().sync_all()?;
        file.persist(&path).map_err(|e| e.error)?;
    }
    let log_path: PathBuf =
        xdg("XDG_STATE_HOME", ".local/state").join("cosmic-forge/engine-build.log");
    fs::create_dir_all(log_path.parent().unwrap())?;
    let mut log = fs::File::create(&log_path)?;
    let mut command = Command::new("python3");
    command
        .arg(root.join("install.py"))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(log.try_clone()?);
    if restore {
        command.arg("--restore");
    }
    let mut child = command
        .spawn()
        .context("The engine installer requires Python 3")?;
    let mut last = Progress::Checking;
    let output = BufReader::new(child.stdout.take().context("Missing installer output")?);
    let read_result = (|| -> Result<()> {
        let mut log_error = None;
        for line in output.lines() {
            let line = line?;
            if let Err(error) = writeln!(log, "{line}") {
                log_error = Some(error);
            }
            if let Some(stage) = Progress::from_line(&line) {
                last = stage;
                progress(stage);
            }
        }
        if let Some(error) = log_error {
            return Err(error.into());
        }
        Ok(())
    })();
    let result = child.wait()?;
    read_result?;
    let output = fs::read_to_string(&log_path).unwrap_or_default();
    let tail = output
        .lines()
        .rev()
        .take(8)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("\n");
    ensure!(
        result.success(),
        "Renderer setup failed: {tail}\nFull log: {}",
        log_path.display()
    );
    Ok(if last == Progress::AlreadyInstalled {
        "The matching Forge glass renderer is already installed. No rebuild or reinstall needed."
            .into()
    } else if restore {
        "Original COSMIC compositor restored. Sign out and back in to activate it.".into()
    } else {
        "Glass renderer installed. Sign out and back in to activate it.".into()
    })
}
