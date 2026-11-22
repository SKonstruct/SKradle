//! Updating the launcher itself from its GitHub Releases. Releases carry one installer per platform named
//! `SKradle-<version>-<os>-<arch>.<ext>`; GitHub publishes a SHA-256 for every asset (`digest`), which is required
//! here: an asset without one is never offered. Only download URLs under this project's own releases are accepted.
//! Starting the downloaded installer is up to the application.

use std::io;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::game::Os;
use crate::index::Arch;
use crate::net::{Client, DownloadOptions, NetError, Progress};

pub const REPO: &str = "SKonstruct/SKradle";
/// The release list, newest first. Not `releases/latest`: that answers 404 while nothing is published, which cannot be
/// told from a wrong address, and it never shows what a draft or pre-release hides.
pub const RELEASES_URL: &str =
    "https://api.github.com/repos/SKonstruct/SKradle/releases?per_page=20";
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

/// What a check found. Every outcome other than an error is a calm answer, not a failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Nothing is published yet (no release, only drafts and pre-releases, or the project's releases page is empty).
    NoReleases,
    /// The newest release is not newer than this build (or is the one the player skipped).
    UpToDate { latest: Version },
    /// A newer release exists but carries no installer (with a SHA-256) for this OS and CPU.
    NoInstaller { version: Version },
    Available(Update),
}

/// Why a check or download failed, as the page words it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure {
    /// GitHub's limit for anonymous requests (403 or 429); it lifts within the hour.
    RateLimited,
    /// No connection, a timeout or a TLS problem.
    Offline,
    /// GitHub answered with a server error.
    Server,
    /// The answer was not release data.
    Malformed,
    /// The downloaded installer did not match its published SHA-256.
    Checksum,
    Other,
}

#[derive(Debug)]
pub enum UpdateError {
    Net(NetError),
    Malformed(String),
}

impl UpdateError {
    pub fn failure(&self) -> Failure {
        match self {
            UpdateError::Malformed(_) => Failure::Malformed,
            UpdateError::Net(e) => failure_of(e),
        }
    }
}

/// Sorts a network error into what the player can do about it.
pub fn failure_of(error: &NetError) -> Failure {
    match error {
        NetError::Status(403 | 429) => Failure::RateLimited,
        NetError::Status(500..=599) => Failure::Server,
        NetError::Request(_) => Failure::Offline,
        NetError::Io(e) if e.kind() != io::ErrorKind::InvalidData => Failure::Offline,
        NetError::Checksum { .. } => Failure::Checksum,
        NetError::TooLarge { .. } => Failure::Malformed,
        _ => Failure::Other,
    }
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

fn convert(raw: RawRelease) -> Option<Release> {
    if raw.draft || raw.prerelease {
        return None;
    }
    let version = Version::parse(&raw.tag_name)?;
    let assets = raw
        .assets
        .into_iter()
        .filter_map(|a| {
            let sha256 = a.digest?.strip_prefix("sha256:")?.to_ascii_lowercase();
            let valid_name = is_plain_file_name(&a.name);
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
    Some(Release {
        version,
        notes: raw.body.unwrap_or_default(),
        assets,
    })
}

/// Parses one release object. Drafts, pre-releases and versions that are not plain `x.y.z` are rejected by returning
/// `Ok(None)`; assets that are unusable (no SHA-256, a URL outside this project's releases) are dropped.
pub fn parse_release(json: &str) -> Result<Option<Release>, UpdateError> {
    let raw: RawRelease =
        serde_json::from_str(json).map_err(|e| UpdateError::Malformed(e.to_string()))?;
    Ok(convert(raw))
}

/// Parses GitHub's release list and returns the highest usable release, or `Ok(None)` when there is none (an empty
/// list, or only drafts, pre-releases and odd tags). Anything that is not a list of releases is malformed.
pub fn parse_releases(json: &str) -> Result<Option<Release>, UpdateError> {
    let raw: Vec<RawRelease> =
        serde_json::from_str(json).map_err(|e| UpdateError::Malformed(e.to_string()))?;
    Ok(raw
        .into_iter()
        .filter_map(convert)
        .max_by_key(|release| release.version))
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

/// What to tell the player about `release` for this build: the [`Outcome`] of comparing them.
pub fn decide(
    release: Option<&Release>,
    current: Version,
    os: Os,
    arch: Arch,
    skipped: Option<Version>,
) -> Outcome {
    let Some(release) = release else {
        return Outcome::NoReleases;
    };
    if release.version <= current || skipped == Some(release.version) {
        return Outcome::UpToDate {
            latest: release.version,
        };
    }
    match pick_update(release, current, os, arch, skipped) {
        Some(update) => Outcome::Available(update),
        None => Outcome::NoInstaller {
            version: release.version,
        },
    }
}

/// [`decide`] from the body and status of the release-list request. A 404 means the project has no releases page
/// content yet, which is the same calm answer as an empty list.
pub fn outcome_from_response(
    response: Result<String, NetError>,
    current: Version,
    os: Os,
    arch: Arch,
    skipped: Option<Version>,
) -> Result<Outcome, UpdateError> {
    match response {
        Err(NetError::Status(404)) => Ok(Outcome::NoReleases),
        Err(e) => Err(e.into()),
        Ok(json) => {
            let release = parse_releases(&json)?;
            Ok(decide(release.as_ref(), current, os, arch, skipped))
        }
    }
}

/// Asks GitHub for the project's releases and says what that means for this build.
pub fn check(
    client: &Client,
    current: Version,
    os: Os,
    arch: Arch,
    skipped: Option<Version>,
) -> Result<Outcome, UpdateError> {
    let response = client.get_text(RELEASES_URL, MAX_API_BYTES);
    outcome_from_response(response, current, os, arch, skipped)
}

/// A name that is one plain file name: not empty, not `.` or `..`, and no path separator.
fn is_plain_file_name(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\\'])
}

/// Downloads the installer into `dir`, verified against its published SHA-256, and returns its path.
pub fn download(
    client: &Client,
    update: &Update,
    dir: &Path,
    progress: &mut dyn FnMut(Progress),
) -> Result<PathBuf, NetError> {
    // `Update` can be built by anyone, so the name is checked here again, not trusted from `parse_release`.
    if !is_plain_file_name(&update.asset.name) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "unsafe installer name").into());
    }
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
            asset_json("..", Some(&digest), DOWNLOAD_PREFIX),
            asset_json(".", Some(&digest), DOWNLOAD_PREFIX),
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

    fn list(releases: &[String]) -> String {
        format!("[{}]", releases.join(","))
    }

    fn outcome(response: Result<String, NetError>, current: &str) -> Result<Outcome, UpdateError> {
        outcome_from_response(response, v(current), Os::MacOs, Arch::Arm64, None)
    }

    #[test]
    fn nothing_published_yet_is_a_calm_answer_in_every_form() {
        // The list is empty.
        assert_eq!(outcome(Ok("[]".into()), "1.0.0").unwrap(), Outcome::NoReleases);
        // The repository answers 404 (nothing there yet).
        assert_eq!(
            outcome(Err(NetError::Status(404)), "1.0.0").unwrap(),
            Outcome::NoReleases
        );
        // Only drafts and pre-releases.
        let assets = good_assets();
        let only_hidden = list(&[
            release_json("v1.3.0", r#""draft":true,"#, &assets),
            release_json("v1.4.0", r#""prerelease":true,"#, &assets),
            release_json("nightly", "", &assets),
        ]);
        assert_eq!(outcome(Ok(only_hidden), "1.0.0").unwrap(), Outcome::NoReleases);
    }

    #[test]
    fn a_release_that_is_not_newer_is_up_to_date() {
        let body = list(&[release_json("v1.3.0", "", &good_assets())]);
        assert_eq!(
            outcome(Ok(body.clone()), "1.3.0").unwrap(),
            Outcome::UpToDate { latest: v("1.3.0") }
        );
        assert_eq!(
            outcome(Ok(body), "2.0.0").unwrap(),
            Outcome::UpToDate { latest: v("1.3.0") }
        );
    }

    #[test]
    fn the_newest_usable_release_wins_whatever_the_order() {
        let assets = good_assets();
        let body = list(&[
            release_json("v1.4.0", r#""draft":true,"#, &assets),
            release_json("v1.2.0", "", &assets),
            release_json("v1.3.0", "", &assets),
            release_json("v1.5.0", r#""prerelease":true,"#, &assets),
        ]);
        match outcome(Ok(body), "1.0.0").unwrap() {
            Outcome::Available(update) => assert_eq!(update.version, v("1.3.0")),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_newer_release_without_an_installer_for_this_machine_says_so() {
        // Only Windows installers exist; this is macOS.
        let digest = format!("sha256:{SHA}");
        let assets = vec![asset_json(
            "SKradle-1.3.0-windows-x64.exe",
            Some(&digest),
            DOWNLOAD_PREFIX,
        )];
        let body = list(&[release_json("v1.3.0", "", &assets)]);
        assert_eq!(
            outcome(Ok(body), "1.0.0").unwrap(),
            Outcome::NoInstaller { version: v("1.3.0") }
        );
        // An installer without a published SHA-256 does not count either.
        let no_hash = vec![asset_json("SKradle-1.3.0-macos-arm64.dmg", None, DOWNLOAD_PREFIX)];
        let body = list(&[release_json("v1.3.0", "", &no_hash)]);
        assert!(matches!(
            outcome(Ok(body), "1.0.0").unwrap(),
            Outcome::NoInstaller { .. }
        ));
    }

    #[test]
    fn a_newer_release_with_its_installer_is_offered_and_a_skipped_one_is_not() {
        let body = list(&[release_json("v1.3.0", "", &good_assets())]);
        match outcome(Ok(body.clone()), "1.0.0").unwrap() {
            Outcome::Available(update) => {
                assert_eq!(update.asset.name, "SKradle-1.3.0-macos-arm64.dmg")
            }
            other => panic!("{other:?}"),
        }
        let skipped = outcome_from_response(
            Ok(body),
            v("1.0.0"),
            Os::MacOs,
            Arch::Arm64,
            Some(v("1.3.0")),
        )
        .unwrap();
        assert_eq!(skipped, Outcome::UpToDate { latest: v("1.3.0") });
    }

    #[test]
    fn failures_are_sorted_into_what_the_player_can_do() {
        let kind = |e: NetError| outcome(Err(e), "1.0.0").unwrap_err().failure();
        assert_eq!(kind(NetError::Status(403)), Failure::RateLimited);
        assert_eq!(kind(NetError::Status(429)), Failure::RateLimited);
        assert_eq!(kind(NetError::Status(502)), Failure::Server);
        assert_eq!(kind(NetError::Status(401)), Failure::Other);
        assert_eq!(kind(NetError::Request("timeout".into())), Failure::Offline);
        assert_eq!(
            kind(NetError::Io(io::Error::new(io::ErrorKind::TimedOut, "t"))),
            Failure::Offline
        );
        assert_eq!(kind(NetError::TooLarge { limit: 1 }), Failure::Malformed);
        assert_eq!(
            failure_of(&NetError::Checksum {
                expected: "a".into(),
                actual: "b".into()
            }),
            Failure::Checksum
        );
    }

    #[test]
    fn malformed_release_lists_are_failures_not_silence() {
        for body in ["not json", "{}", "{\"tag_name\":\"v1.0.0\"}", "[{\"x\":1}]", ""] {
            let error = outcome(Ok(body.into()), "1.0.0").unwrap_err();
            assert_eq!(error.failure(), Failure::Malformed, "{body:?}");
        }
        // A list with one release that has the fields but an odd tag is just "nothing usable".
        assert_eq!(
            outcome(Ok(list(&[release_json("x", "", &[])])), "1.0.0").unwrap(),
            Outcome::NoReleases
        );
    }
}
