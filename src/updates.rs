// SPDX-License-Identifier: GPL-3.0-only
use crate::{VERSION, engine, packs};
use anyhow::{Context, Result, ensure};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{fs, io::Write, path::Path, time::Duration};

pub const CHECK_INTERVAL: Duration = Duration::from_secs(60 * 60);

fn installed_hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn valid_tag(tag: &str) -> bool {
    !tag.is_empty() && tag.len() <= 256 && !tag.chars().any(char::is_control)
}

pub(crate) fn save_release_tag(path: &Path, tag: &str, installed: &[u8]) -> Result<()> {
    ensure!(valid_tag(tag), "Invalid release tag");
    ensure!(!path.is_symlink(), "Preserved a replaced release record");
    let bytes =
        serde_json::to_vec(&serde_json::json!({"tag":tag, "sha256":installed_hash(installed)}))?;
    if fs::read(path).is_ok_and(|old| old == bytes) {
        return Ok(());
    }
    let parent = path.parent().context("Missing release record directory")?;
    fs::create_dir_all(parent)?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(&bytes)?;
    file.as_file().sync_all()?;
    file.persist(path).map_err(|e| e.error)?;
    Ok(())
}

pub(crate) fn release_tag(path: &Path, installed: &[u8]) -> Option<String> {
    let record: Value = serde_json::from_slice(&fs::read(path).ok()?).ok()?;
    let tag = record["tag"].as_str()?;
    (valid_tag(tag) && record["sha256"].as_str()? == installed_hash(installed))
        .then(|| tag.to_owned())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Component {
    App,
    Engine,
    Shaders,
}

impl Component {
    pub fn name(self) -> &'static str {
        match self {
            Self::App => "COSMIC Forge",
            Self::Engine => "Compositor engine",
            Self::Shaders => "Shader pack",
        }
    }

    pub fn repository(self) -> &'static str {
        match self {
            Self::App => "motixo/cosmic-forge",
            Self::Engine => engine::REPOSITORY,
            Self::Shaders => packs::REPOSITORY,
        }
    }
}

#[derive(Debug)]
pub struct Release {
    pub component: Component,
    pub tag: String,
    pub url: String,
    pub notes: String,
    pub installed: bool,
}

impl Release {
    fn parse(component: Component, metadata: &[u8]) -> Result<Option<Self>> {
        let value: Value = serde_json::from_slice(metadata)?;
        if value["draft"] == true || value["prerelease"] == true {
            return Ok(None);
        }
        let tag = value["tag_name"]
            .as_str()
            .context("Missing release version")?;
        ensure!(!tag.is_empty(), "Missing release version");
        let url = value["html_url"].as_str().context("Missing release page")?;
        let prefix = format!(
            "https://github.com/{}/releases/tag/",
            component.repository()
        );
        ensure!(
            url.starts_with(&prefix) && url.len() > prefix.len(),
            "Unexpected release page"
        );
        Ok(Some(Self {
            component,
            tag: tag.into(),
            url: url.into(),
            notes: value["body"].as_str().unwrap_or("").trim().into(),
            installed: false,
        }))
    }
}

#[derive(Debug, Default)]
pub struct Report {
    pub releases: Vec<Release>,
    pub messages: Vec<String>,
    pub engine_available: Option<bool>,
}

pub fn due(enabled: bool, elapsed: Duration) -> bool {
    enabled && elapsed >= CHECK_INTERVAL
}

fn version(tag: &str) -> Result<Vec<u64>> {
    let parts = tag
        .strip_prefix('v')
        .unwrap_or(tag)
        .split('.')
        .map(str::parse::<u64>)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    ensure!(parts.len() == 3, "Unsupported app release version");
    Ok(parts)
}

pub fn check(data_root: &Path) -> Report {
    check_with(
        data_root,
        packs::download,
        engine::update_available_from_release,
    )
}

fn check_with(
    data_root: &Path,
    mut fetch: impl FnMut(&str) -> Result<(u16, Vec<u8>)>,
    mut engine_available: impl FnMut(&[u8]) -> Result<bool>,
) -> Report {
    let mut report = Report::default();
    for component in [Component::App, Component::Engine, Component::Shaders] {
        let result = (|| -> Result<Option<Release>> {
            let (status, metadata) = fetch(&format!(
                "https://api.github.com/repos/{}/releases/latest",
                component.repository()
            ))?;
            if status == 404 {
                report
                    .messages
                    .push(format!("{}: no releases published", component.name()));
                return Ok(None);
            }
            ensure!(status == 200, "GitHub returned HTTP {status}");
            let Some(mut release) = Release::parse(component, &metadata)? else {
                return Ok(None);
            };
            let available = match component {
                Component::App => version(&release.tag)? > version(VERSION)?,
                Component::Engine => {
                    let available = engine_available(&metadata)?;
                    report.engine_available = Some(available);
                    available
                }
                Component::Shaders => {
                    let before = packs::installed_revision(data_root);
                    let result = packs::update_from_release(data_root, &metadata, |url| {
                        let (status, bytes) = fetch(url)?;
                        ensure!(status == 200, "The shader pack is unavailable");
                        Ok(bytes)
                    });
                    match result {
                        Ok(_) => {
                            release.installed = packs::installed_revision(data_root) > before;
                            release.installed
                        }
                        Err(error) => {
                            report
                                .messages
                                .push(format!("Shader pack: automatic update failed: {error:#}"));
                            return Ok(Some(release));
                        }
                    }
                }
            };
            report.messages.push(format!(
                "{}: {}",
                component.name(),
                if release.installed {
                    "updated automatically"
                } else if available {
                    "update available"
                } else {
                    "up to date"
                }
            ));
            Ok(available.then_some(release))
        })();
        match result {
            Ok(Some(release)) => report.releases.push(release),
            Ok(None) => {}
            Err(error) => report
                .messages
                .push(format!("{}: {error:#}", component.name())),
        }
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use sha2::{Digest, Sha256};

    fn metadata(component: Component, tag: &str) -> Value {
        json!({
            "tag_name": tag,
            "html_url": format!("https://github.com/{}/releases/tag/{tag}", component.repository()),
            "body": "## Changes\n\nA useful update.",
            "draft": false,
            "prerelease": false,
        })
    }

    fn shader_release() -> (Value, Vec<u8>) {
        let effects: Value = serde_json::from_str(include_str!("../shaders/effects.json")).unwrap();
        let bytes = serde_json::to_vec(&json!({
            "format": 1, "id": "cosmic-glass", "revision": 9,
            "effects": [effects[0].clone()]
        }))
        .unwrap();
        let mut release = metadata(Component::Shaders, "glass-v9");
        release["assets"] = json!([{
            "name": packs::ASSET,
            "size": bytes.len(),
            "digest": format!("sha256:{}", Sha256::digest(&bytes).iter().map(|b| format!("{b:02x}")).collect::<String>()),
            "browser_download_url": format!("https://github.com/{}/releases/download/glass-v9/{}", packs::REPOSITORY, packs::ASSET),
        }]);
        (release, bytes)
    }

    #[test]
    fn stable_versions_and_release_links_are_checked_before_notifying() {
        assert!(version("v0.12.0").unwrap() > version("0.11.9").unwrap());
        assert!(version("v1.0.0").unwrap() > version("v0.99.0").unwrap());
        assert_eq!(version("v0.11.2").unwrap(), version("0.11.2").unwrap());
        assert!(version("v0.12.0-beta").is_err());
        assert!(version("not-a-version").is_err());
        let mut release = metadata(Component::App, "v0.12.0");
        release["prerelease"] = json!(true);
        assert!(
            Release::parse(Component::App, &serde_json::to_vec(&release).unwrap())
                .unwrap()
                .is_none()
        );
        release["prerelease"] = json!(false);
        for url in [
            "https://example.com/releases/tag/v1.0.0",
            "https://github.com/motixo/cosmic-forge-engine/releases/tag/v1.0.0",
        ] {
            release["html_url"] = json!(url);
            assert!(
                Release::parse(Component::App, &serde_json::to_vec(&release).unwrap()).is_err()
            );
        }
    }

    #[test]
    fn checks_all_three_repositories_and_only_installs_the_verified_shader_pack() {
        let root = tempfile::tempdir().unwrap();
        let (shaders, bytes) = shader_release();
        let mut requests = Vec::new();
        let mut check = || {
            check_with(
                root.path(),
                |url| {
                    requests.push(url.to_owned());
                    let data = if url.ends_with(packs::ASSET) {
                        bytes.clone()
                    } else if url.contains(packs::REPOSITORY) {
                        serde_json::to_vec(&shaders).unwrap()
                    } else if url.contains(engine::REPOSITORY) {
                        serde_json::to_vec(&metadata(Component::Engine, "engine-v0.6.0")).unwrap()
                    } else {
                        serde_json::to_vec(&metadata(Component::App, "v99.0.0")).unwrap()
                    };
                    Ok((200, data))
                },
                |_| Ok(true),
            )
        };
        let report = check();
        assert_eq!(
            report
                .releases
                .iter()
                .map(|r| r.component)
                .collect::<Vec<_>>(),
            [Component::App, Component::Engine, Component::Shaders]
        );
        assert_eq!(report.engine_available, Some(true));
        assert!(report.releases[2].installed);
        assert!(!report.releases[0].installed && !report.releases[1].installed);
        assert_eq!(packs::installed_revision(root.path()), Some(9));
        assert!(
            report
                .releases
                .iter()
                .all(|r| r.notes.contains("A useful update"))
        );
        assert_eq!(
            check().releases.len(),
            2,
            "An unchanged shader pack is not announced again"
        );
        assert_eq!(
            requests
                .iter()
                .filter(|url| url.ends_with("/releases/latest"))
                .count(),
            6
        );
        assert!(!root.path().join("engine-installation.json").exists());
    }

    #[test]
    fn one_unreachable_repository_does_not_hide_other_updates_or_allow_bad_shader_data() {
        let root = tempfile::tempdir().unwrap();
        let (shaders, _) = shader_release();
        let report = check_with(
            root.path(),
            |url| {
                if url.contains(packs::REPOSITORY) {
                    Ok((
                        200,
                        if url.ends_with(packs::ASSET) {
                            b"corrupt".to_vec()
                        } else {
                            serde_json::to_vec(&shaders).unwrap()
                        },
                    ))
                } else if url.contains(engine::REPOSITORY) {
                    Ok((
                        200,
                        serde_json::to_vec(&metadata(Component::Engine, "engine-v0.6.0")).unwrap(),
                    ))
                } else {
                    anyhow::bail!("offline")
                }
            },
            |_| Ok(true),
        );
        assert_eq!(report.releases.len(), 2);
        assert_eq!(report.engine_available, Some(true));
        assert!(report.releases.iter().all(|r| !r.installed));
        assert!(report.messages.iter().any(|m| m.contains("offline")));
        assert!(
            report
                .messages
                .iter()
                .any(|m| m.contains("automatic update failed"))
        );
        assert_eq!(packs::installed_revision(root.path()), None);
    }

    #[test]
    fn automatic_checks_require_opt_in_and_a_full_hour() {
        assert!(!due(false, Duration::from_secs(7200)));
        assert!(!due(true, Duration::from_secs(3599)));
        assert!(due(true, Duration::from_secs(3600)));
        assert!(due(true, Duration::from_secs(7200)));
    }
}
