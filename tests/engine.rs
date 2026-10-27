// SPDX-License-Identifier: GPL-3.0-only
use base64::{Engine, engine::general_purpose::STANDARD};
use sha2::{Digest, Sha256};
use std::{fs, process::Command};

#[test]
fn bundled_setup_restores_original_compositor_without_the_source_checkout() {
    if std::env::var_os("FORGE_TEST_RESTORE_CHILD").is_some() {
        let mut stages = Vec::new();
        let result = cosmic_forge::engine::setup(true, |stage| stages.push(stage));
        if !std::path::Path::new("/usr/bin/cosmic-comp").exists() {
            assert!(
                result
                    .unwrap_err()
                    .to_string()
                    .contains("original COSMIC compositor is unavailable")
            );
            return;
        }
        assert!(
            result
                .unwrap()
                .contains("Original COSMIC compositor restored")
        );
        assert_eq!(cosmic_forge::store::engine_status().0, "Desktop effects");
        assert!(stages.contains(&cosmic_forge::engine::Progress::Restoring));
        assert_eq!(
            stages.last(),
            Some(&cosmic_forge::engine::Progress::Complete)
        );
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let state = temp.path().join("state");
    let launcher = home.join(".local/bin/cosmic-comp");
    fs::create_dir_all(launcher.parent().unwrap()).unwrap();
    fs::create_dir_all(state.join("cosmic-forge")).unwrap();
    fs::create_dir_all(state.join("cosmic-crystal")).unwrap();
    let current = b"#!/bin/sh\n# Managed by cosmic-forge renderer\nexec /new/cosmic-comp\n";
    let previous = b"#!/bin/sh\nexec /old/cosmic-comp\n";
    fs::write(&launcher, current).unwrap();
    fs::write(
        state.join("cosmic-crystal/installation.json"),
        serde_json::to_vec(&serde_json::json!({
            "destination":temp.path().join("old-engine"),
            "shims":{"cosmic-comp":String::from_utf8_lossy(previous)}
        }))
        .unwrap(),
    )
    .unwrap();
    let recovery = state.join("cosmic-forge/engine-previous-compositor");
    fs::write(&recovery, previous).unwrap();
    let digest: String = Sha256::digest(current)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    fs::write(
        state.join("cosmic-forge/engine-installation.json"),
        serde_json::to_vec(&serde_json::json!({
            "abi":1, "launcher":launcher, "launcher_sha256":digest,
            "previous":{"data":STANDARD.encode(previous),"mode":493}
        }))
        .unwrap(),
    )
    .unwrap();
    let result = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "bundled_setup_restores_original_compositor_without_the_source_checkout",
            "--nocapture",
        ])
        .env("FORGE_TEST_RESTORE_CHILD", "1")
        .env("HOME", &home)
        .env("XDG_STATE_HOME", &state)
        .env("XDG_DATA_HOME", temp.path().join("data"))
        .env("XDG_CACHE_HOME", temp.path().join("cache"))
        .current_dir(temp.path())
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    if !std::path::Path::new("/usr/bin/cosmic-comp").exists() {
        assert_eq!(fs::read(launcher).unwrap(), current);
        assert!(state.join("cosmic-forge/engine-installation.json").exists());
        return;
    }
    assert!(!launcher.exists());
    assert_eq!(fs::read(recovery).unwrap(), previous);
    assert!(!state.join("cosmic-forge/engine-installation.json").exists());
}
