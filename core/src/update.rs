//! Updating the launcher itself from its GitHub Releases. Releases carry one installer per platform named
//! `SKradle-<version>-<os>-<arch>.<ext>`; GitHub publishes a SHA-256 for every asset (`digest`), which is required
//! here: an asset without one is never offered. Only download URLs under this project's own releases are accepted.
//! Starting the downloaded installer is up to the application.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::game::Os;
use crate::index::Arch;
use crate::net::{Client, DownloadOptions, NetError, Progress};

pub const REPO: &str = "SKonstruct/SKradle";
pub const LATEST_RELEASE_URL: &str =
    "https://api.github.com/repos/SKonstruct/SKradle/releases/latest";
const DOWNLOAD_PREFIX: &str = "https://github.com/SKonstruct/SKradle/releases/download/";
const MAX_API_BYTES: u64 = 2 << 20;
const MAX_INSTALLER_BYTES: u64 = 1 << 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl Version {
    /// Parses `1.2.3` or `v1.2.3`. Pre-release and build suffixes are not accepted: they are never offered.
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim().strip_prefix('v').unwrap_or(text.trim());
        let mut parts = text.split('.');
        let version = Self {
            major: parts.next()?.parse().ok()?,
            minor: parts.next()?.parse().ok()?,
            patch: parts.next()?.parse().ok()?,
        };
        parts.next().is_none().then_some(version)
    }

    /// The version this program was built as.
    pub fn current() -> Self {
        Self::parse(env!("CARGO_PKG_VERSION")).expect("the crate version is major.minor.patch")
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asset {
    pub name: String,
    pub url: String,
    pub size: u64,
    /// Lower-case hex SHA-256 from GitHub's `digest` field.
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub version: Version,
    /// The release notes (Markdown).
    pub notes: String,
    pub assets: Vec<Asset>,
}

/// What the launcher offers the player.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Update {
    pub version: Version,
    pub notes: String,
    pub asset: Asset,
}

#[derive(Debug)]
pub enum UpdateError {
    Net(NetError),
    Malformed(String),
}

impl std::fmt::Display for UpdateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UpdateError::Net(e) => write!(f, "{e}"),
            UpdateError::Malformed(why) => write!(f, "unexpected release data: {why}"),
        }
    }
}

impl std::error::Error for UpdateError {}

impl From<NetError> for UpdateError {
    fn from(e: NetError) -> Self {
        UpdateError::Net(e)
    }
}

#[derive(Deserialize)]
struct RawRelease {
    tag_name: String,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<RawAsset>,
}

#[derive(Deserialize)]
struct RawAsset {
    name: String,
    browser_download_url: String,
    size: u64,
    #[serde(default)]
    digest: Option<String>,
}

/// The installer file name for a platform, for example `SKradle-1.2.3-macos-arm64.dmg`.
pub fn asset_name(version: Version, os: Os, arch: Arch) -> String {
    let (os_name, ext) = match os {
        Os::Windows => ("windows", "exe"),
        Os::MacOs => ("macos", "dmg"),
        Os::Linux => ("linux", "AppImage"),
    };
    let arch_name = match arch {
        Arch::X64 => "x64",
        Arch::Arm64 => "arm64",
    };
    format!("SKradle-{version}-{os_name}-{arch_name}.{ext}")
}

/// Parses GitHub's `releases/latest` JSON. Drafts, pre-releases and versions that are not plain `x.y.z` are rejected
/// by returning `Ok(None)`; assets that are unusable (no SHA-256, a URL outside this project's releases) are dropped.
pub fn parse_release(json: &str) -> Result<Option<Release>, UpdateError> {
    let raw: RawRelease =
        serde_json::from_str(json).map_err(|e| UpdateError::Malformed(e.to_string()))?;
    if raw.draft || raw.prerelease {
        return Ok(None);
    }
    let Some(version) = Version::parse(&raw.tag_name) else {
        return Ok(None);
    };
    let assets = raw
        .assets
        .into_iter()
        .filter_map(|a| {
            let sha256 = a.digest?.strip_prefix("sha256:")?.to_ascii_lowercase();
            let valid_name = !a.name.is_empty() && !a.name.contains(['/', '\\']);
            let valid_url = a.browser_download_url.starts_with(DOWNLOAD_PREFIX)
                && !a.browser_download_url[DOWNLOAD_PREFIX.len()..].contains("..");
            let valid_hash = sha256.len() == 64 && sha256.bytes().all(|b| b.is_ascii_hexdigit());
            (valid_name && valid_url && valid_hash && a.size > 0 && a.size <= MAX_INSTALLER_BYTES)
                .then_some(Asset {
                    name: a.name,
                    url: a.browser_download_url,
                    size: a.size,
                    sha256,
                })
        })
        .collect();
    Ok(Some(Release {
        version,
        notes: raw.body.unwrap_or_default(),
        assets,
    }))
}

/// The update to offer, if any: the release is newer than `current`, was not skipped by the player, and has the
/// installer for this platform.
pub fn pick_update(
    release: &Release,
    current: Version,
    os: Os,
    arch: Arch,
    skipped: Option<Version>,
) -> Option<Update> {
    if release.version <= current || skipped == Some(release.version) {
        return None;
    }
    let wanted = asset_name(release.version, os, arch);
    let asset = release.assets.iter().find(|a| a.name == wanted)?;
    Some(Update {
        version: release.version,
        notes: release.notes.clone(),
        asset: asset.clone(),
    })
}

/// Asks GitHub for the latest release and returns the update to offer, if any.
pub fn check(
    client: &Client,
    current: Version,
    os: Os,
    arch: Arch,
    skipped: Option<Version>,
) -> Result<Option<Update>, UpdateError> {
    let json = client.get_text(LATEST_RELEASE_URL, MAX_API_BYTES)?;
    Ok(parse_release(&json)?.and_then(|release| pick_update(&release, current, os, arch, skipped)))
}

/// Downloads the installer into `dir`, verified against its published SHA-256, and returns its path.
pub fn download(
    client: &Client,
    update: &Update,
    dir: &Path,
    progress: &mut dyn FnMut(Progress),
) -> Result<PathBuf, NetError> {
    std::fs::create_dir_all(dir)?;
    let dest = dir.join(&update.asset.name);
    let options =
        DownloadOptions::new(update.asset.size.max(1)).with_sha256(update.asset.sha256.clone());
    client.download(&update.asset.url, &dest, &options, progress)?;
    Ok(dest)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHA: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    fn asset_json(name: &str, digest: Option<&str>, url_prefix: &str) -> String {
        let digest = digest.map_or("null".to_owned(), |d| format!("\"{d}\""));
        format!(
            r#"{{"name":"{name}","size":1234,"digest":{digest},"browser_download_url":"{url_prefix}v1.3.0/{name}","state":"uploaded"}}"#
        )
    }

    fn release_json(tag: &str, flags: &str, assets: &[String]) -> String {
        format!(
            r#"{{"tag_name":"{tag}","name":"SKradle","body":"Notes\n- a change",{flags}"assets":[{}]}}"#,
            assets.join(",")
        )
    }

    fn good_assets() -> Vec<String> {
        let digest = format!("sha256:{SHA}");
        [
            "macos-arm64.dmg",
            "macos-x64.dmg",
            "windows-x64.exe",
            "windows-arm64.exe",
            "linux-x64.AppImage",
        ]
        .iter()
        .map(|suffix| {
            asset_json(
                &format!("SKradle-1.3.0-{suffix}"),
                Some(&digest),
                DOWNLOAD_PREFIX,
            )
        })
        .collect()
    }

    fn v(text: &str) -> Version {
        Version::parse(text).unwrap()
    }

    #[test]
    fn versions_parse_and_order() {
        assert_eq!(
            v("1.2.3"),
            Version {
                major: 1,
                minor: 2,
                patch: 3
            }
        );
        assert_eq!(v("v1.2.3"), v("1.2.3"));
        assert!(v("1.10.0") > v("1.9.9"));
        assert!(v("2.0.0") > v("1.99.99"));
        for bad in [
            "",
            "1",
            "1.2",
            "1.2.3.4",
            "1.2.3-beta",
            "1.2.x",
            "v",
            "1..3",
        ] {
            assert_eq!(Version::parse(bad), None, "{bad:?}");
        }
        assert_eq!(v("1.2.3").to_string(), "1.2.3");
        let _ = Version::current();
    }

    #[test]
    fn asset_names_follow_the_convention() {
        assert_eq!(
            asset_name(v("1.3.0"), Os::MacOs, Arch::Arm64),
            "SKradle-1.3.0-macos-arm64.dmg"
        );
        assert_eq!(
            asset_name(v("1.3.0"), Os::Windows, Arch::X64),
            "SKradle-1.3.0-windows-x64.exe"
        );
        assert_eq!(
            asset_name(v("1.3.0"), Os::Linux, Arch::X64),
            "SKradle-1.3.0-linux-x64.AppImage"
        );
    }

    #[test]
    fn offers_a_newer_release_with_the_right_installer() {
        let release = parse_release(&release_json("v1.3.0", "", &good_assets()))
            .unwrap()
            .unwrap();
        assert_eq!(release.version, v("1.3.0"));
        assert_eq!(release.assets.len(), 5);
        let update = pick_update(&release, v("1.2.0"), Os::MacOs, Arch::Arm64, None).unwrap();
        assert_eq!(update.asset.name, "SKradle-1.3.0-macos-arm64.dmg");
        assert_eq!(update.asset.sha256, SHA);
        assert!(update.notes.contains("a change"));
    }

    #[test]
    fn offers_nothing_when_current_skipped_or_missing_the_platform() {
        let release = parse_release(&release_json("v1.3.0", "", &good_assets()))
            .unwrap()
            .unwrap();
        assert_eq!(
            pick_update(&release, v("1.3.0"), Os::MacOs, Arch::Arm64, None),
            None,
            "already current"
        );
        assert_eq!(
            pick_update(&release, v("1.4.0"), Os::MacOs, Arch::Arm64, None),
            None,
            "newer than the release"
        );
        assert_eq!(
            pick_update(
                &release,
                v("1.2.0"),
                Os::MacOs,
                Arch::Arm64,
                Some(v("1.3.0"))
            ),
            None,
            "skipped"
        );
        assert!(pick_update(
            &release,
            v("1.2.0"),
            Os::MacOs,
            Arch::Arm64,
            Some(v("1.2.5"))
        )
        .is_some());
        assert_eq!(
            pick_update(&release, v("1.2.0"), Os::Linux, Arch::Arm64, None),
            None,
            "no Linux arm64 installer"
        );
    }

    #[test]
    fn drafts_prereleases_and_odd_tags_are_ignored() {
        let assets = good_assets();
        assert_eq!(
            parse_release(&release_json("v1.3.0", r#""draft":true,"#, &assets)).unwrap(),
            None
        );
        assert_eq!(
            parse_release(&release_json("v1.3.0", r#""prerelease":true,"#, &assets)).unwrap(),
            None
        );
        assert_eq!(
            parse_release(&release_json("nightly", "", &assets)).unwrap(),
            None
        );
        assert_eq!(
            parse_release(&release_json("v1.3.0-rc1", "", &assets)).unwrap(),
            None
        );
    }

    #[test]
    fn unusable_assets_are_dropped() {
        let digest = format!("sha256:{SHA}");
        let assets = vec![
            asset_json("SKradle-1.3.0-macos-arm64.dmg", None, DOWNLOAD_PREFIX),
            asset_json(
                "SKradle-1.3.0-macos-x64.dmg",
                Some("md5:abc"),
                DOWNLOAD_PREFIX,
            ),
            asset_json(
                "SKradle-1.3.0-linux-x64.AppImage",
                Some("sha256:xyz"),
                DOWNLOAD_PREFIX,
            ),
            asset_json(
                "SKradle-1.3.0-windows-x64.exe",
                Some(&digest),
                "https://evil.example/releases/download/",
            ),
            asset_json(
                "SKradle-1.3.0-windows-arm64.exe",
                Some(&digest),
                "http://github.com/SKonstruct/SKradle/releases/download/",
            ),
            asset_json("../evil.exe", Some(&digest), DOWNLOAD_PREFIX),
            asset_json("SKradle-1.3.0-ok.zip", Some(&digest), DOWNLOAD_PREFIX),
        ];
        let release = parse_release(&release_json("v1.3.0", "", &assets))
            .unwrap()
            .unwrap();
        let names: Vec<_> = release.assets.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, ["SKradle-1.3.0-ok.zip"]);
    }

    #[test]
    fn malformed_json_is_an_error() {
        assert!(matches!(
            parse_release("not json"),
            Err(UpdateError::Malformed(_))
        ));
        assert!(matches!(
            parse_release("{}"),
            Err(UpdateError::Malformed(_))
        ));
    }
}
