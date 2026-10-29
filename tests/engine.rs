// SPDX-License-Identifier: GPL-3.0-only
use base64::{Engine, engine::general_purpose::STANDARD};
use cosmic_forge::engine::{self, ASSET, REPOSITORY};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{fs, process::Command};

fn bundle() -> Value {
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join(ASSET);
    let result = Command::new("python3")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/engine/scripts/package_engine.py"
        ))
        .arg("--output")
        .arg(&output)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    serde_json::from_slice(&fs::read(output).unwrap()).unwrap()
}

fn release(bytes: &[u8]) -> Value {
    let digest: String = Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    json!({"assets":[{
        "name":ASSET, "size":bytes.len(), "digest":format!("sha256:{digest}"),
        "browser_download_url":format!("https://github.com/{REPOSITORY}/releases/download/engine-v1/{ASSET}")
    }]})
}

fn prepare(cache: &std::path::Path, bundle: &Value) -> anyhow::Result<std::path::PathBuf> {
    let bytes = serde_json::to_vec(bundle).unwrap();
    engine::prepare_from_release(
        cache,
        &serde_json::to_vec(&release(&bytes)).unwrap(),
        |_| Ok(bytes),
    )
}

#[test]
fn published_engine_sources_and_new_revisions_work_without_rebuilding_the_app() {
    let cache = tempfile::tempdir().unwrap();
    let mut bundle = bundle();
    let root = prepare(cache.path(), &bundle).unwrap();
    for (name, source) in bundle["files"].as_object().unwrap() {
        assert_eq!(
            fs::read_to_string(root.join(name)).unwrap(),
            source.as_str().unwrap()
        );
    }
    let before = root
        .join("install.py")
        .metadata()
        .unwrap()
        .modified()
        .unwrap();
    assert_eq!(prepare(cache.path(), &bundle).unwrap(), root);
    assert_eq!(
        root.join("install.py")
            .metadata()
            .unwrap()
            .modified()
            .unwrap(),
        before
    );
    // Installer updates keep the renderer revision and need no app rebuild.
    bundle["files"]["install.py"] = json!(format!(
        "{}\n# installer update\n",
        bundle["files"]["install.py"].as_str().unwrap()
    ));
    let updated = prepare(cache.path(), &bundle).unwrap();
    assert_ne!(updated, root);
    assert!(
        fs::read_to_string(updated.join("install.py"))
            .unwrap()
            .ends_with("# installer update\n")
    );
    let mut metadata: Value =
        serde_json::from_str(bundle["files"]["release.json"].as_str().unwrap()).unwrap();
    metadata["renderer_revision"] = json!(metadata["renderer_revision"].as_u64().unwrap() + 1);
    bundle["files"]["release.json"] = json!(metadata.to_string());
    assert_ne!(prepare(cache.path(), &bundle).unwrap(), updated);
}

#[test]
fn engine_download_rejects_foreign_incomplete_or_corrupted_assets() {
    let cache = tempfile::tempdir().unwrap();
    let bytes = serde_json::to_vec(&bundle()).unwrap();
    let good = release(&bytes);
    for (key, value) in [
        (
            "browser_download_url",
            json!("https://github.com/other/engine/releases/download/v1/engine.json"),
        ),
        ("size", json!(bytes.len() + 1)),
        ("digest", json!(format!("sha256:{}", "0".repeat(64)))),
        ("digest", Value::Null),
        ("size", json!(3 * 1024 * 1024)),
    ] {
        let mut release = good.clone();
        release["assets"][0][key] = value;
        assert!(
            engine::prepare_from_release(
                cache.path(),
                &serde_json::to_vec(&release).unwrap(),
                |_| Ok(bytes.clone())
            )
            .is_err()
        );
    }
    assert_eq!(fs::read_dir(cache.path()).unwrap().count(), 0);
}

#[test]
fn incompatible_or_unsafe_engine_bundles_never_escape_the_cache() {
    let temp = tempfile::tempdir().unwrap();
    let cache = temp.path().join("cache");
    let good = bundle();
    let mut variants = Vec::new();
    for (key, value) in [("abi", json!(2)), ("format", json!(2))] {
        let mut bad = good.clone();
        bad[key] = value;
        variants.push(bad);
    }
    for name in [
        "../escape",
        "/tmp/escape",
        "sub/../../escape",
        "sub//escape",
        "sub\\escape",
        "",
    ] {
        let mut bad = good.clone();
        bad["files"][name] = json!("untrusted source");
        variants.push(bad);
    }
    for metadata in [
        json!({"abi":2,"renderer_revision":1}),
        json!({"abi":1,"renderer_revision":0}),
    ] {
        let mut bad = good.clone();
        bad["files"]["release.json"] = json!(metadata.to_string());
        variants.push(bad);
    }
    let mut missing = good.clone();
    missing["files"]
        .as_object_mut()
        .unwrap()
        .remove("recovery.py");
    variants.push(missing);
    for bad in variants {
        assert!(prepare(&cache, &bad).is_err());
    }
    assert!(!cache.exists());
    assert!(!temp.path().join("escape").exists());
}

#[test]
fn engine_cache_preserves_symlinks_and_their_targets() {
    use std::os::unix::fs::symlink;
    let temp = tempfile::tempdir().unwrap();
    let bundle = bundle();
    let cache = temp.path().join("cache");
    let root = prepare(&cache, &bundle).unwrap();
    let outside = temp.path().join("outside");
    fs::write(&outside, "preserved").unwrap();
    fs::remove_file(root.join("install.py")).unwrap();
    symlink(&outside, root.join("install.py")).unwrap();
    assert!(prepare(&cache, &bundle).is_err());
    assert_eq!(fs::read_to_string(&outside).unwrap(), "preserved");
    fs::remove_dir_all(&root).unwrap();
    symlink(temp.path(), &root).unwrap();
    assert!(prepare(&cache, &bundle).is_err());
    assert!(root.is_symlink());
}

#[test]
fn install_uses_the_verified_release_without_requesting_a_build() {
    if std::env::var_os("FORGE_TEST_ACTION_CHILD").is_some() {
        let mut stages = Vec::new();
        assert!(
            engine::setup(engine::Action::Install, |stage| stages.push(stage))
                .unwrap()
                .contains("already installed")
        );
        assert!(stages.contains(&engine::Progress::Downloading));
        assert_eq!(stages.last(), Some(&engine::Progress::AlreadyInstalled));
        return;
    }
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let tools = root.join("tools");
    fs::create_dir(&tools).unwrap();
    let installer = r#"import argparse,json,os
from pathlib import Path
p=argparse.ArgumentParser()
p.add_argument('--build',action='store_true')
p.add_argument('--release-tag',required=True)
a=p.parse_args()
with Path(os.environ['FORGE_ACTION_AUDIT']).open('a') as f:
    f.write(json.dumps({'build':a.build,'tag':a.release_tag})+'\n')
print('FORGE_PROGRESS:already_installed',flush=True)
"#;
    let bytes = serde_json::to_vec(&json!({"format":1,"abi":1,"files":{
        "install.py":installer,"build.py":"","recovery.py":"",
        "release.json":"{\"abi\":1,\"renderer_revision\":2}"
    }}))
    .unwrap();
    let mut metadata = release(&bytes);
    metadata["tag_name"] = json!("engine-test-actions");
    fs::write(
        root.join("release.json"),
        serde_json::to_vec(&metadata).unwrap(),
    )
    .unwrap();
    fs::write(root.join("bundle.json"), bytes).unwrap();
    let curl = tools.join("curl");
    fs::write(
        &curl,
        r#"#!/usr/bin/python3
import os,sys
from pathlib import Path
root=Path(os.environ['FORGE_ACTION_FIXTURE'])
source='release.json' if sys.argv[-1].endswith('/releases/latest') else 'bundle.json'
Path(sys.argv[sys.argv.index('--output')+1]).write_bytes((root/source).read_bytes())
sys.stdout.write('200')
"#,
    )
    .unwrap();
    fs::set_permissions(&curl, fs::Permissions::from_mode(0o755)).unwrap();
    let audit = root.join("audit.jsonl");
    let result = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "install_uses_the_verified_release_without_requesting_a_build",
            "--nocapture",
        ])
        .env("FORGE_TEST_ACTION_CHILD", "1")
        .env("FORGE_ACTION_FIXTURE", root)
        .env("FORGE_ACTION_AUDIT", &audit)
        .env("XDG_STATE_HOME", root.join("state"))
        .env("XDG_CACHE_HOME", root.join("cache"))
        .env(
            "PATH",
            format!("{}:{}", tools.display(), std::env::var("PATH").unwrap()),
        )
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    let calls: Vec<Value> = fs::read_to_string(audit)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(calls, [json!({"build":false,"tag":"engine-test-actions"})]);
}

#[test]
fn offline_recovery_restores_original_compositor_without_the_source_checkout() {
    if std::env::var_os("FORGE_TEST_RESTORE_CHILD").is_some() {
        let mut stages = Vec::new();
        let result =
            cosmic_forge::engine::setup(engine::Action::Restore, |stage| stages.push(stage));
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
            "offline_recovery_restores_original_compositor_without_the_source_checkout",
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
