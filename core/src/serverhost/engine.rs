//! The engine: finding the newest Tortodrone release, downloading and verifying it, unpacking only what the
//! launcher needs and installing it so the previous engine can be put back.
//!
//! **What a release is** (read from the engine's release workflow; no release was downloaded while writing this):
//! one asset, `Tortodrone-<tag>.zip` (tag like `v0.40.17`), with the files at the archive root: `tortodrone.jar`,
//! `VERSION`, `config/`, `content/`, `webui/`, `plugins/`, `docs/`, `licenses/`, `LICENSE`, `NOTICE`,
//! `THIRD-PARTY.md`. The engine's releases have been published as GitHub *pre-releases* (development releases), so
//! pre-releases are accepted (drafts never).
//! GitHub publishes a `digest` (`sha256:<hex>`) for each asset; a release without one is not offered.
//!
//! **What the launcher installs from it**: the jar, `content/` and `webui/` (each replaced as a whole, the old
//! copy kept as `.previous`), the default `config/` files (added only where missing, an existing configuration is
//! never touched) and the licence texts, which stay next to the jar. The docs, README and the like are skipped.
//! The player's News edits (the WebUI saves them in `content/shared/news`) are carried over into the new
//! `content/`; nothing else in `content/` is kept.
//! The engine serves its administration pages from `webui/`, so a release without `webui/index.html` is refused.
//! Replacing `webui/` is safe: the engine restores its locally prepared artwork from `data/` on the next start.
//!
//! **Safety**: HTTPS only, the download URL must be the release's own `github.com/N0rule/Tortodrone` asset URL, the
//! archive is hashed against GitHub's digest while it downloads, every entry name is checked (an archive with one
//! unsafe name is refused whole), links are refused, and size caps apply to the download and to what is unpacked
//! (declared sizes are not trusted: the bytes are counted as they are copied). Unpacking goes to a staging folder
//! first; the live files are replaced only when the staged set is complete, and put back if the swap fails.
//! Nothing is installed before the player has accepted the engine's licence ([`super::terms`]).

use std::cmp::Ordering;
use std::collections::HashSet;
use std::fmt;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::net::{self, Client, DownloadOptions, NetError, Progress};

use super::fsx::{copy_tree, remove_all, sibling, write_atomic};
use super::layout::{safe_relative, PathError, ServerLayout};
use super::terms;

pub const RELEASES_API: &str =
    "https://api.github.com/repos/N0rule/Tortodrone/releases?per_page=10";
const DOWNLOAD_PREFIX: &str = "https://github.com/N0rule/Tortodrone/releases/download/";

const MAX_API_BYTES: u64 = 4 * 1024 * 1024;
/// The most the release archive may be. A guess: the engine and its content pack are tens of MiB.
pub const MAX_ARCHIVE_BYTES: u64 = 1024 * 1024 * 1024;
/// The most one unpacked file may be.
pub const MAX_ENTRY_BYTES: u64 = 512 * 1024 * 1024;
/// The most all unpacked files together may be.
pub const MAX_UNPACKED_BYTES: u64 = 2 * 1024 * 1024 * 1024;
pub const MAX_ENTRIES: usize = 50_000;

#[derive(Debug)]
pub enum EngineError {
    /// The player has not accepted the engine's licence yet.
    TermsNotAccepted,
    Net(NetError),
    Io(io::Error),
    /// The releases listing could not be understood.
    BadListing(String),
    /// The archive is not a usable release; the text says why.
    BadArchive(String),
    /// An entry name in the archive is unsafe.
    UnsafeEntry(String, PathError),
    /// The archive's bytes do not have the expected SHA-256.
    ChecksumMismatch,
    /// There is no previous engine to go back to.
    NoPrevious,
}

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EngineError::TermsNotAccepted => {
                write!(f, "the engine's licence has not been accepted")
            }
            EngineError::Net(e) => write!(f, "{e}"),
            EngineError::Io(e) => write!(f, "{e}"),
            EngineError::BadListing(why) => write!(f, "unreadable release listing: {why}"),
            EngineError::BadArchive(why) => write!(f, "unusable release archive: {why}"),
            EngineError::UnsafeEntry(name, why) => {
                write!(f, "unsafe name in the archive ({why}): {name:?}")
            }
            EngineError::ChecksumMismatch => write!(f, "the archive does not match its checksum"),
            EngineError::NoPrevious => write!(f, "there is no previous engine to go back to"),
        }
    }
}

impl std::error::Error for EngineError {}

impl From<io::Error> for EngineError {
    fn from(e: io::Error) -> Self {
        EngineError::Io(e)
    }
}

impl From<NetError> for EngineError {
    fn from(e: NetError) -> Self {
        match e {
            NetError::Checksum { .. } => EngineError::ChecksumMismatch,
            other => EngineError::Net(other),
        }
    }
}

/// A dotted numeric version such as `0.40.17`. Compared number by number (so `0.40.17` is newer than `0.40.8`),
/// ignoring trailing zeros.
#[derive(Debug, Clone)]
pub struct EngineVersion {
    text: String,
    parts: Vec<u64>,
}

impl EngineVersion {
    /// Parses `0.40.17` or `v0.40.17`. Anything with other characters is refused.
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        let text = text.strip_prefix(['v', 'V']).unwrap_or(text);
        if text.is_empty() || text.len() > 32 {
            return None;
        }
        let mut parts = Vec::new();
        for part in text.split('.') {
            if part.is_empty()
                || !part.bytes().all(|b| b.is_ascii_digit())
                || (part.len() > 1 && part.starts_with('0'))
            {
                return None;
            }
            parts.push(part.parse().ok()?);
        }
        while parts.len() > 1 && parts.last() == Some(&0) {
            parts.pop();
        }
        Some(Self {
            text: text.to_owned(),
            parts,
        })
    }

    pub fn as_str(&self) -> &str {
        &self.text
    }
}

impl PartialEq for EngineVersion {
    fn eq(&self, other: &Self) -> bool {
        self.parts == other.parts
    }
}
impl Eq for EngineVersion {}
impl PartialOrd for EngineVersion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for EngineVersion {
    fn cmp(&self, other: &Self) -> Ordering {
        self.parts.cmp(&other.parts)
    }
}
impl fmt::Display for EngineVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)
    }
}

/// A published engine release that can be installed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub version: EngineVersion,
    /// The git tag, `v` plus the version.
    pub tag: String,
    pub prerelease: bool,
    /// The asset's download URL (always the engine's own GitHub release URL).
    pub url: String,
    /// The SHA-256 GitHub reports for the asset, lower-case hex.
    pub sha256: String,
    pub size: Option<u64>,
    pub page_url: String,
}

fn is_hex64(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// The newest installable release in a GitHub releases listing (JSON), or `None` when no release carries the
/// expected archive with a SHA-256 digest. Drafts are skipped; pre-releases count.
pub fn pick_latest(json: &str) -> Result<Option<Release>, EngineError> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|e| EngineError::BadListing(e.to_string()))?;
    let releases = value
        .as_array()
        .ok_or_else(|| EngineError::BadListing("not a list".into()))?;
    let mut best: Option<Release> = None;
    for release in releases {
        if release["draft"].as_bool().unwrap_or(false) {
            continue;
        }
        let Some(tag) = release["tag_name"].as_str() else {
            continue;
        };
        let Some(version) = EngineVersion::parse(tag) else {
            continue;
        };
        // The tag must be exactly `v<version>`: the asset and download URL are derived from it.
        if tag != format!("v{version}") {
            continue;
        }
        let asset_name = format!("Tortodrone-{tag}.zip");
        let Some(asset) = release["assets"]
            .as_array()
            .and_then(|a| a.iter().find(|a| a["name"].as_str() == Some(&asset_name)))
        else {
            continue;
        };
        let Some(digest) = asset["digest"]
            .as_str()
            .and_then(|d| d.strip_prefix("sha256:"))
            .filter(|d| is_hex64(d))
        else {
            continue;
        };
        let url = format!("{DOWNLOAD_PREFIX}{tag}/{asset_name}");
        if asset["browser_download_url"].as_str() != Some(&url) {
            continue;
        }
        let candidate = Release {
            version,
            tag: tag.to_owned(),
            prerelease: release["prerelease"].as_bool().unwrap_or(false),
            url,
            sha256: digest.to_ascii_lowercase(),
            size: asset["size"].as_u64(),
            page_url: release["html_url"].as_str().unwrap_or("").to_owned(),
        };
        if best.as_ref().is_none_or(|b| candidate.version > b.version) {
            best = Some(candidate);
        }
    }
    Ok(best)
}

/// Asks GitHub for the newest installable release.
pub fn fetch_latest(client: &Client) -> Result<Option<Release>, EngineError> {
    let text = client.get_text(RELEASES_API, MAX_API_BYTES)?;
    pick_latest(&text)
}

/// The launcher's record of the installed engine, in `engine.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstalledEngine {
    pub version: String,
    #[serde(rename = "jarSha256")]
    pub jar_sha256: String,
    #[serde(rename = "archiveSha256")]
    pub archive_sha256: String,
    /// Seconds since the Unix epoch.
    #[serde(rename = "installedAt")]
    pub installed_at: u64,
    /// The version kept as `tortodrone.jar.previous`, if any.
    #[serde(default, rename = "previousVersion")]
    pub previous_version: Option<String>,
}

/// What is installed in a server folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineState {
    NotInstalled,
    Installed(InstalledEngine),
    /// A jar is there but it is not the one the record describes (replaced or damaged by hand).
    Modified,
}

pub fn installed(layout: &ServerLayout) -> EngineState {
    let record = fs::read_to_string(layout.engine_record())
        .ok()
        .and_then(|t| serde_json::from_str::<InstalledEngine>(&t).ok());
    let jar_hash = net::sha256_file(&layout.jar()).ok();
    match (record, jar_hash) {
        (Some(r), Some(h)) if r.jar_sha256.eq_ignore_ascii_case(&h) => EngineState::Installed(r),
        (None, None) => EngineState::NotInstalled,
        (Some(_), None) => EngineState::NotInstalled,
        _ => EngineState::Modified,
    }
}

/// Whether the engine's files (jar, content, WebUI pages, a configuration) are in place and ready to start.
pub fn is_ready(layout: &ServerLayout) -> bool {
    matches!(installed(layout), EngineState::Installed(_))
        && layout.content_dir().is_dir()
        && layout.webui_dir().join("index.html").is_file()
        && layout.server_yml().is_file()
}

/// Where an installation is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Stage {
    Downloading(Progress),
    Verifying,
    Unpacking,
    Installing,
    Done,
}

/// Why an installation did nothing or what it did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateOutcome {
    UpToDate(String),
    Installed(InstalledEngine),
}

/// What a look at the newest release found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Latest {
    /// The installed engine (this version) is at least as new as the newest release.
    UpToDate(String),
    Newer(Release),
}

/// Whether `release` is newer than what `layout` has installed.
fn compare_installed(layout: &ServerLayout, release: Release) -> Latest {
    if let EngineState::Installed(current) = installed(layout) {
        if EngineVersion::parse(&current.version).is_some_and(|v| v >= release.version) {
            return Latest::UpToDate(current.version);
        }
    }
    Latest::Newer(release)
}

/// Looks up the newest release and compares it with the installed engine. Changes nothing, so it needs neither a
/// stopped server nor a backup.
pub fn check_latest(client: &Client, layout: &ServerLayout) -> Result<Latest, EngineError> {
    require_terms(layout)?;
    let Some(release) = fetch_latest(client)? else {
        return Err(EngineError::BadListing(
            "no release has the expected archive and checksum".into(),
        ));
    };
    Ok(compare_installed(layout, release))
}

/// Installs the newest release unless the installed engine is already at least that new. The caller holds the
/// lifecycle lock with the server stopped, and has backed up `data/` and `config/` first.
pub fn update_to_latest(
    client: &Client,
    layout: &ServerLayout,
    progress: &mut dyn FnMut(Stage),
) -> Result<UpdateOutcome, EngineError> {
    match check_latest(client, layout)? {
        Latest::UpToDate(version) => Ok(UpdateOutcome::UpToDate(version)),
        Latest::Newer(release) => {
            install_release(client, layout, &release, progress).map(UpdateOutcome::Installed)
        }
    }
}

fn require_terms(layout: &ServerLayout) -> Result<(), EngineError> {
    if terms::is_accepted(layout) {
        Ok(())
    } else {
        Err(EngineError::TermsNotAccepted)
    }
}

/// Downloads `release`, verifies it and installs it.
pub fn install_release(
    client: &Client,
    layout: &ServerLayout,
    release: &Release,
    progress: &mut dyn FnMut(Stage),
) -> Result<InstalledEngine, EngineError> {
    require_terms(layout)?;
    layout.ensure()?;
    let zip = layout.download_path();
    let result = (|| {
        fetch_archive(client, &release.url, &release.sha256, &zip, progress)?;
        install_archive(layout, &zip, release, terms::now_secs(), progress)
    })();
    let _ = fs::remove_file(&zip);
    result
}

/// Downloads `url` to `dest`, checking its SHA-256 while it is written.
pub(crate) fn fetch_archive(
    client: &Client,
    url: &str,
    sha256: &str,
    dest: &Path,
    progress: &mut dyn FnMut(Stage),
) -> Result<(), EngineError> {
    let options = DownloadOptions::new(MAX_ARCHIVE_BYTES).with_sha256(sha256);
    client.download(url, dest, &options, &mut |p| {
        progress(Stage::Downloading(p))
    })?;
    Ok(())
}

/// Installs the already downloaded archive `zip`. Checks its SHA-256 again, unpacks it to staging and swaps it in.
/// `now` is recorded as the install time.
pub fn install_archive(
    layout: &ServerLayout,
    zip: &Path,
    release: &Release,
    now: u64,
    progress: &mut dyn FnMut(Stage),
) -> Result<InstalledEngine, EngineError> {
    require_terms(layout)?;
    layout.ensure()?;
    progress(Stage::Verifying);
    let archive_hash = net::sha256_file(zip)?;
    if !archive_hash.eq_ignore_ascii_case(&release.sha256) {
        return Err(EngineError::ChecksumMismatch);
    }
    progress(Stage::Unpacking);
    let staging = layout.staging_dir();
    remove_all(&staging)?;
    let result = (|| {
        unpack(zip, &staging, &release.version)?;
        progress(Stage::Installing);
        let previous_version = match installed(layout) {
            EngineState::Installed(old) => Some(old.version),
            _ => None,
        };
        swap_in(layout, &staging)?;
        let record = InstalledEngine {
            version: release.version.to_string(),
            jar_sha256: net::sha256_file(&layout.jar())?,
            archive_sha256: archive_hash.clone(),
            installed_at: now,
            previous_version,
        };
        write_record(layout, &record)?;
        Ok(record)
    })();
    let _ = remove_all(&staging);
    if result.is_ok() {
        progress(Stage::Done);
    }
    result
}

fn write_record(layout: &ServerLayout, record: &InstalledEngine) -> io::Result<()> {
    let json = serde_json::to_string_pretty(record).map_err(io::Error::other)?;
    write_atomic(&layout.engine_record(), json.as_bytes())
}

/// Which archive entries the launcher installs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Wanted {
    Jar,
    Version,
    Content,
    Webui,
    Config,
    Terms,
}

const TERM_FILES: [&str; 3] = ["LICENSE", "NOTICE", "THIRD-PARTY.md"];

fn classify(path: &Path) -> Option<(Wanted, PathBuf)> {
    let parts: Vec<&str> = path.iter().filter_map(|p| p.to_str()).collect();
    match parts.as_slice() {
        ["tortodrone.jar"] => Some((Wanted::Jar, PathBuf::from("tortodrone.jar"))),
        ["VERSION"] => Some((Wanted::Version, PathBuf::from("VERSION"))),
        ["content", rest @ ..] if !rest.is_empty() => Some((Wanted::Content, path.to_path_buf())),
        ["webui", rest @ ..] if !rest.is_empty() => Some((Wanted::Webui, path.to_path_buf())),
        ["config", _] => Some((Wanted::Config, path.to_path_buf())),
        ["licenses", rest @ ..] if !rest.is_empty() => Some((Wanted::Terms, path.to_path_buf())),
        [name] if TERM_FILES.contains(name) => Some((Wanted::Terms, path.to_path_buf())),
        // Older releases kept the licence texts under docs/.
        ["docs", name] if TERM_FILES.contains(name) => Some((Wanted::Terms, PathBuf::from(name))),
        ["docs", "licenses", rest @ ..] if !rest.is_empty() => Some((
            Wanted::Terms,
            std::iter::once("licenses")
                .chain(rest.iter().copied())
                .collect(),
        )),
        _ => None,
    }
}

/// Unpacks the wanted entries of `zip` into `staging` and checks the set is complete.
fn unpack(zip: &Path, staging: &Path, version: &EngineVersion) -> Result<(), EngineError> {
    let mut archive = zip::ZipArchive::new(File::open(zip)?)
        .map_err(|e| EngineError::BadArchive(e.to_string()))?;
    if archive.len() > MAX_ENTRIES {
        return Err(EngineError::BadArchive("too many entries".into()));
    }
    // Pass 1: look at every name before writing anything.
    let mut plan: Vec<(usize, Wanted, PathBuf)> = Vec::new();
    // Looked up for every entry, so these are sets (a list would make a 50,000-entry archive quadratic). Names are
    // folded to lower case: two entries that differ only by case would overwrite each other on Windows and macOS.
    let mut seen: HashSet<String> = HashSet::new();
    let mut targets: HashSet<PathBuf> = HashSet::new();
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|e| EngineError::BadArchive(e.to_string()))?;
        let name = entry
            .name()
            .map_err(|e| EngineError::BadArchive(e.to_string()))?
            .into_owned();
        let relative =
            safe_relative(&name).map_err(|e| EngineError::UnsafeEntry(name.clone(), e))?;
        if entry.is_symlink() || entry.encrypted() {
            return Err(EngineError::BadArchive(format!(
                "{name:?} is a link or encrypted"
            )));
        }
        if entry.is_dir() {
            continue;
        }
        if !seen.insert(relative.to_string_lossy().to_lowercase()) {
            return Err(EngineError::BadArchive(format!(
                "{name:?} appears twice (or differs from another entry only by case)"
            )));
        }
        if let Some((kind, target)) = classify(&relative) {
            // A docs/ copy must not shadow a root one.
            if !targets.insert(target.clone()) {
                continue;
            }
            plan.push((index, kind, target));
        }
    }
    for needed in [Wanted::Jar, Wanted::Version, Wanted::Content, Wanted::Webui] {
        if !plan.iter().any(|(_, kind, _)| *kind == needed) {
            return Err(EngineError::BadArchive(format!(
                "no {needed:?} in the archive"
            )));
        }
    }
    for needed in ["config/server.yml", "webui/index.html", "LICENSE", "NOTICE"] {
        if !plan.iter().any(|(_, _, t)| t == Path::new(needed)) {
            return Err(EngineError::BadArchive(format!(
                "no {needed} in the archive"
            )));
        }
    }

    // Pass 2: copy, counting the real bytes.
    let mut total = 0u64;
    for (index, _, target) in &plan {
        let mut entry = archive
            .by_index(*index)
            .map_err(|e| EngineError::BadArchive(e.to_string()))?;
        let dest = staging.join(target);
        if let Some(dir) = dest.parent() {
            fs::create_dir_all(dir)?;
        }
        let mut out = File::create(&dest)?;
        let copied = copy_capped(&mut entry, &mut out, MAX_ENTRY_BYTES)?;
        total += copied;
        if total > MAX_UNPACKED_BYTES {
            return Err(EngineError::BadArchive("unpacks to too much data".into()));
        }
        out.sync_all()?;
    }

    // Checks on what was unpacked.
    let inside = fs::read_to_string(staging.join("VERSION"))?;
    if inside.trim() != version.as_str() {
        return Err(EngineError::BadArchive(format!(
            "the archive says it is version {:?}, not {}",
            inside.trim(),
            version
        )));
    }
    let mut magic = [0u8; 4];
    File::open(staging.join("tortodrone.jar"))?
        .read_exact(&mut magic)
        .ok();
    if &magic != b"PK\x03\x04" {
        return Err(EngineError::BadArchive(
            "tortodrone.jar is not a jar".into(),
        ));
    }
    Ok(())
}

fn copy_capped(from: &mut impl Read, to: &mut impl Write, cap: u64) -> Result<u64, EngineError> {
    let mut buf = vec![0u8; 64 * 1024];
    let mut done = 0u64;
    loop {
        let n = from.read(&mut buf)?;
        if n == 0 {
            return Ok(done);
        }
        done += n as u64;
        if done > cap {
            return Err(EngineError::BadArchive(
                "an entry is larger than allowed".into(),
            ));
        }
        to.write_all(&buf[..n])?;
    }
}

/// A step of the swap that can be undone.
enum Undo {
    /// `from` was moved to `to`; undoing moves it back.
    Moved { from: PathBuf, to: PathBuf },
    /// `path` was created; undoing removes it.
    Created(PathBuf),
}

fn rollback(steps: Vec<Undo>) {
    for step in steps.into_iter().rev() {
        match step {
            Undo::Moved { from, to } => {
                let _ = remove_all(&from);
                let _ = fs::rename(&to, &from);
            }
            Undo::Created(path) => {
                let _ = remove_all(&path);
            }
        }
    }
}

/// The folders that are replaced as a whole on update, with the name each has in staging, in the server folder
/// and as the previous generation.
fn replaced_dirs(layout: &ServerLayout) -> [(&'static str, PathBuf, PathBuf); 2] {
    [
        ("content", layout.content_dir(), layout.previous_content()),
        ("webui", layout.webui_dir(), layout.previous_webui()),
    ]
}

/// Puts the staged files in place. The current jar, `content/` and `webui/` become `.previous` (an older
/// `.previous` is dropped), default configuration files are added only where missing, licence texts are
/// replaced. If any step fails the earlier ones are undone and the old engine is as it was.
fn swap_in(layout: &ServerLayout, staging: &Path) -> Result<(), EngineError> {
    let mut undo: Vec<Undo> = Vec::new();
    let result = (|| -> io::Result<()> {
        let jar = layout.jar();
        let previous_jar = layout.previous_jar();

        // Retire the current engine; an older previous generation makes room.
        if jar.is_file() {
            remove_all(&previous_jar)?;
            fs::rename(&jar, &previous_jar)?;
            undo.push(Undo::Moved {
                from: jar.clone(),
                to: previous_jar.clone(),
            });
        }
        for (name, live, previous) in replaced_dirs(layout) {
            if live.is_dir() {
                remove_all(&previous)?;
                fs::rename(&live, &previous)?;
                undo.push(Undo::Moved {
                    from: live.clone(),
                    to: previous,
                });
            }
            fs::rename(staging.join(name), &live)?;
            undo.push(Undo::Created(live));
        }
        fs::rename(staging.join("tortodrone.jar"), &jar)?;
        undo.push(Undo::Created(jar));

        // The WebUI saves News edits and uploaded banners under `content/shared/news`; they are the player's, so
        // they are carried into the new content instead of being replaced by the release's defaults.
        let old_news = layout.previous_content().join("shared").join("news");
        if old_news.is_dir() {
            let target = layout.content_dir().join("shared").join("news");
            remove_all(&target)?;
            copy_tree(&old_news, &target, &|_| false)?;
        }

        // Default configuration: only what is missing.
        if let Ok(entries) = fs::read_dir(staging.join("config")) {
            fs::create_dir_all(layout.config_dir())?;
            for entry in entries.flatten() {
                let target = layout.config_dir().join(entry.file_name());
                // A link counts as present, even a dangling one: copying would write through it.
                if fs::symlink_metadata(&target).is_err() {
                    fs::copy(entry.path(), &target)?;
                    undo.push(Undo::Created(target));
                }
            }
        }
        fs::create_dir_all(layout.plugins_dir())?;

        // Licence texts travel with the jar and are replaced by the new release's.
        for name in TERM_FILES {
            let from = staging.join(name);
            if from.is_file() {
                let to = layout.root().join(name);
                // A link (or a stale file) is replaced, never written through; a folder is left, so the copy fails.
                if fs::symlink_metadata(&to).is_ok_and(|m| !m.is_dir()) {
                    fs::remove_file(&to)?;
                }
                fs::copy(&from, &to)?;
            }
        }
        if staging.join("licenses").is_dir() {
            let target = layout.root().join("licenses");
            remove_all(&target)?;
            copy_tree(&staging.join("licenses"), &target, &|_| false)?;
        }
        Ok(())
    })();
    match result {
        Ok(()) => Ok(()),
        Err(e) => {
            rollback(undo);
            Err(EngineError::Io(e))
        }
    }
}

/// Swaps `live` and `previous` through a temporary name; undone if a rename fails.
fn exchange(live: &Path, previous: &Path, undo: &mut Vec<Undo>) -> io::Result<()> {
    let temp = sibling(live, "swap");
    remove_all(&temp)?;
    let had_live = live.exists();
    if had_live {
        fs::rename(live, &temp)?;
        undo.push(Undo::Moved {
            from: live.to_path_buf(),
            to: temp.clone(),
        });
    }
    fs::rename(previous, live)?;
    undo.push(Undo::Moved {
        from: previous.to_path_buf(),
        to: live.to_path_buf(),
    });
    if had_live {
        fs::rename(&temp, previous)?;
        // Without this step a later failure would delete the stashed live engine at `previous`.
        undo.push(Undo::Moved {
            from: temp,
            to: previous.to_path_buf(),
        });
    }
    Ok(())
}

/// Puts the previous engine (jar, content and WebUI) back; the engine being replaced becomes the new previous
/// one. Run with the server stopped. The database may have been changed by the newer engine; restore a backup
/// too if the older engine refuses to start (it refuses a schema it does not know).
pub fn rollback_to_previous(layout: &ServerLayout) -> Result<InstalledEngine, EngineError> {
    let jar = layout.jar();
    let previous_jar = layout.previous_jar();
    let dirs = replaced_dirs(layout);
    if !previous_jar.is_file() || dirs.iter().any(|(_, _, previous)| !previous.is_dir()) {
        return Err(EngineError::NoPrevious);
    }
    let old_record = match installed(layout) {
        EngineState::Installed(r) => Some(r),
        _ => None,
    };
    let mut undo: Vec<Undo> = Vec::new();
    let result = (|| -> io::Result<()> {
        exchange(&jar, &previous_jar, &mut undo)?;
        for (_, live, previous) in &dirs {
            exchange(live, previous, &mut undo)?;
        }
        Ok(())
    })();
    if let Err(e) = result {
        rollback(undo);
        return Err(EngineError::Io(e));
    }
    let record = InstalledEngine {
        version: old_record
            .as_ref()
            .and_then(|r| r.previous_version.clone())
            .unwrap_or_else(|| "unknown".to_owned()),
        jar_sha256: net::sha256_file(&jar)?,
        archive_sha256: String::new(),
        installed_at: terms::now_secs(),
        previous_version: old_record.map(|r| r.version),
    };
    write_record(layout, &record)?;
    Ok(record)
}

#[cfg(test)]
pub(crate) mod testkit {
    //! Builds release archives for tests.
    use super::*;
    use zip::write::SimpleFileOptions;

    pub fn jar_bytes(tag: &str) -> Vec<u8> {
        let mut bytes = b"PK\x03\x04".to_vec();
        bytes.extend_from_slice(format!("jar of {tag}").as_bytes());
        bytes
    }

    /// A release archive like the engine's: files at the root.
    pub fn release_zip(version: &str, extra: &[(&str, &[u8])]) -> Vec<u8> {
        let mut files: Vec<(String, Vec<u8>)> = vec![
            ("tortodrone.jar".into(), jar_bytes(version)),
            ("VERSION".into(), format!("{version}\n").into_bytes()),
            (
                "config/server.yml".into(),
                b"# default\nserver-port: 47624\n".to_vec(),
            ),
            (
                "config/combat.yml".into(),
                b"dodgeCooldownMs: 8000\n".to_vec(),
            ),
            (
                "content/maps/haven/map.yml".into(),
                b"type: town\n".to_vec(),
            ),
            ("content/items/weapons.yml".into(), b"items: []\n".to_vec()),
            ("LICENSE".into(), b"licence text".to_vec()),
            ("NOTICE".into(), b"Required Notice".to_vec()),
            ("THIRD-PARTY.md".into(), b"third party".to_vec()),
            ("licenses/apache-2.0.txt".into(), b"apache".to_vec()),
            ("webui/index.html".into(), b"<html>".to_vec()),
            ("README.md".into(), b"readme".to_vec()),
        ];
        for (name, data) in extra {
            files.retain(|(n, _)| n != name);
            files.push(((*name).to_owned(), data.to_vec()));
        }
        zip_of(&files)
    }

    pub fn zip_of(files: &[(String, Vec<u8>)]) -> Vec<u8> {
        let mut out = io::Cursor::new(Vec::new());
        let mut writer = zip::ZipWriter::new(&mut out);
        for (name, data) in files {
            writer
                .start_file(name.as_str(), SimpleFileOptions::default())
                .unwrap();
            writer.write_all(data).unwrap();
        }
        writer.finish().unwrap();
        out.into_inner()
    }

    pub fn release_for(version: &str, zip: &[u8]) -> Release {
        let tag = format!("v{version}");
        Release {
            version: EngineVersion::parse(version).unwrap(),
            url: format!("{DOWNLOAD_PREFIX}{tag}/Tortodrone-{tag}.zip"),
            tag,
            prerelease: true,
            sha256: net::sha256_hex(zip),
            size: Some(zip.len() as u64),
            page_url: String::new(),
        }
    }

    pub fn accepted_layout(dir: &Path) -> ServerLayout {
        let layout = ServerLayout::new(dir.join("server"));
        layout.ensure().unwrap();
        terms::accept(&layout, 1).unwrap();
        layout
    }

    /// Installs `version` into `layout` from a generated archive.
    pub fn install(
        layout: &ServerLayout,
        version: &str,
        extra: &[(&str, &[u8])],
    ) -> InstalledEngine {
        let zip = release_zip(version, extra);
        let path = layout.root().join("test-release.zip");
        fs::write(&path, &zip).unwrap();
        let record =
            install_archive(layout, &path, &release_for(version, &zip), 10, &mut |_| {}).unwrap();
        fs::remove_file(path).unwrap();
        record
    }
}

#[cfg(test)]
mod tests {
    use super::testkit::*;
    use super::*;

    fn listing(entries: &[(&str, bool, bool, Option<&str>)]) -> String {
        let releases: Vec<_> = entries
            .iter()
            .map(|(tag, draft, prerelease, digest)| {
                let name = format!("Tortodrone-{tag}.zip");
                let mut asset = serde_json::json!({
                    "name": name,
                    "size": 1234,
                    "browser_download_url": format!("{DOWNLOAD_PREFIX}{tag}/{name}"),
                });
                if let Some(d) = digest {
                    asset["digest"] = serde_json::json!(d);
                }
                serde_json::json!({
                    "tag_name": tag, "draft": draft, "prerelease": prerelease,
                    "html_url": format!("https://github.com/N0rule/Tortodrone/releases/tag/{tag}"),
                    "assets": [asset],
                })
            })
            .collect();
        serde_json::to_string(&releases).unwrap()
    }

    const DIGEST: &str = "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789ABCDEF";

    #[test]
    fn versions_compare_numerically() {
        let v = |s| EngineVersion::parse(s).unwrap();
        assert!(v("0.40.17") > v("0.40.8"));
        assert!(v("v0.41") > v("0.40.99"));
        assert_eq!(v("1.0"), v("1.0.0"));
        assert_eq!(v("v0.40.8").to_string(), "0.40.8");
        for bad in [
            "",
            "v",
            "1..2",
            "1.x",
            "1.2-rc1",
            "../1",
            "99999999999999999999999",
        ] {
            assert!(EngineVersion::parse(bad).is_none(), "{bad:?}");
        }
    }

    #[test]
    fn picks_the_newest_release_including_prereleases_but_not_drafts() {
        let json = listing(&[
            ("v0.40.8", false, true, Some(DIGEST)),
            ("v0.40.17", false, true, Some(DIGEST)),
            ("v0.41.0", true, true, Some(DIGEST)),
            ("v0.40.9", false, true, Some(DIGEST)),
        ]);
        let release = pick_latest(&json).unwrap().unwrap();
        assert_eq!(release.tag, "v0.40.17");
        assert!(release.prerelease);
        assert_eq!(
            release.sha256,
            DIGEST["sha256:".len()..].to_ascii_lowercase()
        );
        assert_eq!(
            release.url,
            "https://github.com/N0rule/Tortodrone/releases/download/v0.40.17/Tortodrone-v0.40.17.zip"
        );
        assert_eq!(release.size, Some(1234));
    }

    #[test]
    fn a_release_without_a_valid_digest_or_the_expected_asset_is_not_offered() {
        let json = listing(&[
            ("v0.50.0", false, true, None),
            ("v0.49.0", false, true, Some("sha256:abc")),
            (
                "v0.48.0",
                false,
                true,
                Some("md5:00000000000000000000000000000000"),
            ),
            ("v0.40.1", false, true, Some(DIGEST)),
        ]);
        assert_eq!(pick_latest(&json).unwrap().unwrap().tag, "v0.40.1");
        assert_eq!(pick_latest("[]").unwrap(), None);
        assert!(pick_latest("{}").is_err());
        assert!(pick_latest("nonsense").is_err());
    }

    #[test]
    fn a_download_url_outside_the_engines_release_is_not_offered() {
        let mut releases: serde_json::Value =
            serde_json::from_str(&listing(&[("v0.40.17", false, true, Some(DIGEST))])).unwrap();
        releases[0]["assets"][0]["browser_download_url"] =
            serde_json::json!("https://evil.example/Tortodrone-v0.40.17.zip");
        assert_eq!(pick_latest(&releases.to_string()).unwrap(), None);
        // A tag that is not exactly v<version> (different spelling of the number) is skipped too.
        let json = listing(&[("v0.040.17", false, true, Some(DIGEST))]);
        assert_eq!(pick_latest(&json).unwrap(), None);
    }

    #[test]
    fn installs_only_what_the_launcher_needs() {
        let dir = tempfile::tempdir().unwrap();
        let layout = accepted_layout(dir.path());
        let record = install(&layout, "0.40.17", &[]);
        assert_eq!(record.version, "0.40.17");
        assert_eq!(record.previous_version, None);
        assert_eq!(fs::read(layout.jar()).unwrap(), jar_bytes("0.40.17"));
        assert!(layout.content_dir().join("maps/haven/map.yml").is_file());
        assert!(layout.webui_dir().join("index.html").is_file());
        assert!(layout.server_yml().is_file());
        assert!(layout.combat_yml().is_file());
        for kept in [
            "LICENSE",
            "NOTICE",
            "THIRD-PARTY.md",
            "licenses/apache-2.0.txt",
        ] {
            assert!(layout.root().join(kept).is_file(), "{kept}");
        }
        for skipped in ["README.md", "VERSION"] {
            assert!(!layout.root().join(skipped).exists(), "{skipped}");
        }
        assert!(layout.plugins_dir().is_dir());
        assert!(!layout.staging_dir().exists());
        assert!(is_ready(&layout));
        assert_eq!(installed(&layout), EngineState::Installed(record));
    }

    #[test]
    fn the_players_news_edits_survive_an_update() {
        let dir = tempfile::tempdir().unwrap();
        let layout = accepted_layout(dir.path());
        install(
            &layout,
            "0.40.8",
            &[("content/shared/news/news.yml", b"release default 1")],
        );
        fs::write(
            layout.content_dir().join("shared/news/news.yml"),
            "edited by the player",
        )
        .unwrap();
        fs::write(layout.content_dir().join("shared/news/banner.png"), b"png").unwrap();
        install(
            &layout,
            "0.40.17",
            &[
                ("content/shared/news/news.yml", b"release default 2"),
                ("content/maps/new_map/map.yml", b"type: town\n"),
            ],
        );
        let news = layout.content_dir().join("shared/news");
        assert_eq!(
            fs::read_to_string(news.join("news.yml")).unwrap(),
            "edited by the player"
        );
        assert_eq!(fs::read(news.join("banner.png")).unwrap(), b"png");
        assert!(
            layout.content_dir().join("maps/new_map/map.yml").is_file(),
            "the rest is the new release's"
        );
    }

    #[test]
    fn an_existing_configuration_is_never_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let layout = accepted_layout(dir.path());
        fs::write(layout.server_yml(), "server-port: 5000\n# mine\n").unwrap();
        install(&layout, "0.40.17", &[]);
        assert_eq!(
            fs::read_to_string(layout.server_yml()).unwrap(),
            "server-port: 5000\n# mine\n"
        );
        assert!(layout.combat_yml().is_file());
    }

    #[test]
    fn refuses_to_install_before_the_licence_is_accepted() {
        let dir = tempfile::tempdir().unwrap();
        let layout = ServerLayout::new(dir.path().join("s"));
        let zip = release_zip("0.40.17", &[]);
        let path = dir.path().join("r.zip");
        fs::write(&path, &zip).unwrap();
        let err = install_archive(
            &layout,
            &path,
            &release_for("0.40.17", &zip),
            1,
            &mut |_| {},
        )
        .unwrap_err();
        assert!(matches!(err, EngineError::TermsNotAccepted), "{err}");
        assert!(!layout.jar().exists());
        let err = install_release(
            &Client::new(),
            &layout,
            &release_for("0.40.17", &zip),
            &mut |_| {},
        )
        .unwrap_err();
        assert!(matches!(err, EngineError::TermsNotAccepted));
    }

    #[test]
    fn a_wrong_checksum_installs_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let layout = accepted_layout(dir.path());
        let zip = release_zip("0.40.17", &[]);
        let path = dir.path().join("r.zip");
        fs::write(&path, &zip).unwrap();
        let mut release = release_for("0.40.17", &zip);
        release.sha256 = "00".repeat(32);
        let err = install_archive(&layout, &path, &release, 1, &mut |_| {}).unwrap_err();
        assert!(matches!(err, EngineError::ChecksumMismatch));
        assert!(!layout.jar().exists());
        assert_eq!(installed(&layout), EngineState::NotInstalled);
    }

    fn try_install(
        extra: &[(&str, &[u8])],
        version_in_zip: &str,
    ) -> (
        tempfile::TempDir,
        ServerLayout,
        Result<InstalledEngine, EngineError>,
    ) {
        let dir = tempfile::tempdir().unwrap();
        let layout = accepted_layout(dir.path());
        let zip = release_zip(version_in_zip, extra);
        let path = dir.path().join("r.zip");
        fs::write(&path, &zip).unwrap();
        let result = install_archive(
            &layout,
            &path,
            &release_for(version_in_zip, &zip),
            1,
            &mut |_| {},
        );
        (dir, layout, result)
    }

    #[test]
    fn hostile_entry_names_refuse_the_whole_archive() {
        for name in [
            "../evil",
            "content/../../evil",
            "/abs/evil",
            "a\\b",
            "C:/x",
            "x/../y",
        ] {
            let (_dir, layout, result) = try_install(&[(name, b"x")], "0.40.17");
            let err = result.unwrap_err();
            assert!(matches!(err, EngineError::UnsafeEntry(..)), "{name}: {err}");
            assert!(!layout.jar().exists(), "{name}");
            assert!(!layout.staging_dir().exists());
        }
    }

    #[test]
    fn an_incomplete_archive_is_refused_and_leaves_the_install_alone() {
        let dir = tempfile::tempdir().unwrap();
        let layout = accepted_layout(dir.path());
        install(&layout, "0.40.8", &[]);
        for missing in [
            "tortodrone.jar",
            "VERSION",
            "webui/index.html",
            "config/server.yml",
            "LICENSE",
            "NOTICE",
        ] {
            let files: Vec<(String, Vec<u8>)> = zip_files("0.40.17")
                .into_iter()
                .filter(|(n, _)| n != missing)
                .collect();
            let err = install_zip(&layout, "0.40.17", &zip_of(&files)).unwrap_err();
            assert!(
                matches!(err, EngineError::BadArchive(_)),
                "{missing}: {err}"
            );
        }
        let files: Vec<(String, Vec<u8>)> = zip_files("0.40.17")
            .into_iter()
            .filter(|(n, _)| !n.starts_with("content/"))
            .collect();
        assert!(install_zip(&layout, "0.40.17", &zip_of(&files)).is_err());
        assert_eq!(fs::read(layout.jar()).unwrap(), jar_bytes("0.40.8"));
        assert!(layout.content_dir().join("maps/haven/map.yml").is_file());
    }

    fn zip_files(version: &str) -> Vec<(String, Vec<u8>)> {
        // Re-read the standard archive back into its entries.
        let zip = release_zip(version, &[]);
        let mut archive = zip::ZipArchive::new(io::Cursor::new(zip)).unwrap();
        (0..archive.len())
            .map(|i| {
                let mut e = archive.by_index(i).unwrap();
                let mut data = Vec::new();
                e.read_to_end(&mut data).unwrap();
                (e.name().unwrap().into_owned(), data)
            })
            .collect()
    }

    fn install_zip(
        layout: &ServerLayout,
        version: &str,
        zip: &[u8],
    ) -> Result<InstalledEngine, EngineError> {
        let path = layout.root().join("t.zip");
        fs::write(&path, zip).unwrap();
        let r = install_archive(layout, &path, &release_for(version, zip), 1, &mut |_| {});
        let _ = fs::remove_file(path);
        r
    }

    #[test]
    fn the_archive_must_say_it_is_the_release_it_claims_to_be() {
        let dir = tempfile::tempdir().unwrap();
        let layout = accepted_layout(dir.path());
        let zip = release_zip("0.40.16", &[]);
        let err = install_zip(&layout, "0.40.17", &zip).unwrap_err();
        assert!(matches!(err, EngineError::BadArchive(_)), "{err}");
        assert!(!layout.jar().exists());
    }

    #[test]
    fn two_entries_that_differ_only_by_case_are_refused() {
        let mut files = zip_files("0.40.17");
        // The standard archive already holds content/maps/haven/map.yml.
        files.push(("content/maps/haven/MAP.yml".into(), b"x".to_vec()));
        let dir = tempfile::tempdir().unwrap();
        let layout = accepted_layout(dir.path());
        let err = install_zip(&layout, "0.40.17", &zip_of(&files)).unwrap_err();
        assert!(
            err.to_string().contains("only by case") || err.to_string().contains("twice"),
            "{err}"
        );
        assert!(!layout.jar().exists());
    }

    #[test]
    fn a_windows_device_name_in_an_entry_is_refused() {
        let mut files = zip_files("0.40.17");
        files.push(("content/aux.yml".into(), b"x".to_vec()));
        let dir = tempfile::tempdir().unwrap();
        let layout = accepted_layout(dir.path());
        assert!(install_zip(&layout, "0.40.17", &zip_of(&files)).is_err());
        assert!(!layout.jar().exists());
    }

    #[test]
    fn a_jar_that_is_not_a_zip_is_refused() {
        let (_d, layout, result) = try_install(&[("tortodrone.jar", b"#!/bin/sh\n")], "0.40.17");
        assert!(matches!(result.unwrap_err(), EngineError::BadArchive(_)));
        assert!(!layout.jar().exists());
    }

    #[test]
    fn links_are_refused() {
        use zip::write::SimpleFileOptions;
        let dir = tempfile::tempdir().unwrap();
        let layout = accepted_layout(dir.path());
        let mut out = io::Cursor::new(Vec::new());
        let mut writer = zip::ZipWriter::new(&mut out);
        for (name, data) in zip_files("0.40.17") {
            writer
                .start_file(name, SimpleFileOptions::default())
                .unwrap();
            writer.write_all(&data).unwrap();
        }
        writer
            .add_symlink("content/link", "/etc/passwd", SimpleFileOptions::default())
            .unwrap();
        writer.finish().unwrap();
        let err = install_zip(&layout, "0.40.17", &out.into_inner()).unwrap_err();
        assert!(matches!(err, EngineError::BadArchive(_)), "{err}");
        assert!(!layout.jar().exists());
    }

    #[test]
    fn an_oversized_entry_is_stopped_by_the_cap_not_the_header() {
        let mut out = Vec::new();
        let err = copy_capped(&mut io::repeat(7), &mut out, 1000).unwrap_err();
        assert!(matches!(err, EngineError::BadArchive(_)));
        assert!(out.len() <= 1000);
        let mut out = Vec::new();
        assert_eq!(copy_capped(&mut &b"abc"[..], &mut out, 1000).unwrap(), 3);
    }

    #[test]
    fn an_update_keeps_the_previous_engine_and_rollback_restores_it() {
        let dir = tempfile::tempdir().unwrap();
        let layout = accepted_layout(dir.path());
        install(&layout, "0.40.8", &[("content/maps/old_only.txt", b"old")]);
        let second = install(&layout, "0.40.17", &[("content/maps/new_only.txt", b"new")]);
        assert_eq!(second.previous_version.as_deref(), Some("0.40.8"));
        assert_eq!(
            fs::read(layout.previous_jar()).unwrap(),
            jar_bytes("0.40.8")
        );
        assert!(layout
            .previous_content()
            .join("maps/old_only.txt")
            .is_file());
        assert!(layout.content_dir().join("maps/new_only.txt").is_file());
        assert!(!layout.content_dir().join("maps/old_only.txt").exists());

        let back = rollback_to_previous(&layout).unwrap();
        assert_eq!(back.version, "0.40.8");
        assert_eq!(back.previous_version.as_deref(), Some("0.40.17"));
        assert_eq!(fs::read(layout.jar()).unwrap(), jar_bytes("0.40.8"));
        assert!(layout.content_dir().join("maps/old_only.txt").is_file());
        assert_eq!(
            fs::read(layout.previous_jar()).unwrap(),
            jar_bytes("0.40.17")
        );
        assert!(matches!(installed(&layout), EngineState::Installed(r) if r.version == "0.40.8"));
        // And forward again.
        assert_eq!(rollback_to_previous(&layout).unwrap().version, "0.40.17");
    }

    #[cfg(unix)]
    #[test]
    fn a_failed_rollback_to_previous_keeps_both_engines() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let layout = accepted_layout(dir.path());
        install(&layout, "0.40.8", &[("content/maps/old_only.txt", b"old")]);
        install(&layout, "0.40.17", &[("content/maps/new_only.txt", b"new")]);
        // The jar swaps first, then `content/`; make the content swap fail by leaving an undeletable leftover
        // where its temporary name goes.
        let blocker = sibling(&layout.content_dir(), "swap");
        fs::create_dir_all(blocker.join("inner")).unwrap();
        fs::write(blocker.join("inner/f"), b"x").unwrap();
        fs::set_permissions(blocker.join("inner"), fs::Permissions::from_mode(0o555)).unwrap();
        let can_fail = fs::remove_dir_all(&blocker).is_err();
        let result = if can_fail {
            Some(rollback_to_previous(&layout))
        } else {
            None // running as a user that ignores permissions: nothing to inject
        };
        fs::set_permissions(blocker.join("inner"), fs::Permissions::from_mode(0o755)).ok();
        let Some(result) = result else { return };
        assert!(matches!(result, Err(EngineError::Io(_))));
        assert_eq!(fs::read(layout.jar()).unwrap(), jar_bytes("0.40.17"));
        assert_eq!(
            fs::read(layout.previous_jar()).unwrap(),
            jar_bytes("0.40.8")
        );
        assert!(layout.content_dir().join("maps/new_only.txt").is_file());
        assert!(layout
            .previous_content()
            .join("maps/old_only.txt")
            .is_file());
    }

    #[test]
    fn a_release_is_newer_only_when_it_is_ahead_of_the_installed_engine() {
        let dir = tempfile::tempdir().unwrap();
        let layout = accepted_layout(dir.path());
        let release = |v: &str| Release {
            version: EngineVersion::parse(v).unwrap(),
            tag: format!("v{v}"),
            prerelease: false,
            url: String::new(),
            sha256: String::new(),
            size: None,
            page_url: String::new(),
        };
        // Nothing installed: anything is newer.
        assert!(matches!(
            compare_installed(&layout, release("0.40.17")),
            Latest::Newer(_)
        ));
        install(&layout, "0.40.17", &[]);
        assert_eq!(
            compare_installed(&layout, release("0.40.17")),
            Latest::UpToDate("0.40.17".into())
        );
        assert_eq!(
            compare_installed(&layout, release("0.40.8")),
            Latest::UpToDate("0.40.17".into())
        );
        assert!(matches!(
            compare_installed(&layout, release("0.40.18")),
            Latest::Newer(_)
        ));
    }

    #[cfg(unix)]
    #[test]
    fn installing_never_writes_through_links_in_the_server_folder() {
        let dir = tempfile::tempdir().unwrap();
        let layout = accepted_layout(dir.path());
        let outside = dir.path().join("outside");
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("precious.txt"), b"precious").unwrap();
        fs::create_dir_all(layout.config_dir()).unwrap();
        // A dangling link where a default config file would be copied, and a link in place of a licence text.
        std::os::unix::fs::symlink(
            outside.join("planted.yml"),
            layout.config_dir().join("extra.yml"),
        )
        .unwrap();
        std::os::unix::fs::symlink(outside.join("precious.txt"), layout.root().join("LICENSE"))
            .unwrap();
        install(&layout, "0.40.8", &[("config/extra.yml", b"default: 1\n")]);
        assert!(!outside.join("planted.yml").exists());
        assert_eq!(fs::read(outside.join("precious.txt")).unwrap(), b"precious");
        assert!(!fs::symlink_metadata(layout.root().join("LICENSE"))
            .unwrap()
            .file_type()
            .is_symlink());
    }

    #[test]
    fn rollback_without_a_previous_engine_fails_cleanly() {
        let dir = tempfile::tempdir().unwrap();
        let layout = accepted_layout(dir.path());
        install(&layout, "0.40.8", &[]);
        assert!(matches!(
            rollback_to_previous(&layout).unwrap_err(),
            EngineError::NoPrevious
        ));
        assert_eq!(fs::read(layout.jar()).unwrap(), jar_bytes("0.40.8"));
    }

    #[test]
    fn a_failed_swap_puts_the_old_engine_back() {
        let dir = tempfile::tempdir().unwrap();
        let layout = accepted_layout(dir.path());
        install(&layout, "0.40.8", &[]);
        // Make the licences step fail after the jar and content were swapped: `licenses` exists as a file that
        // cannot be replaced by a tree copy because its parent is read-only is unix specific, so use a directory
        // named like a staged licence *file* instead.
        fs::remove_file(layout.root().join("LICENSE")).unwrap();
        fs::create_dir_all(layout.root().join("LICENSE")).unwrap();
        fs::write(layout.root().join("LICENSE/keep"), b"x").unwrap();
        let err = install_zip(&layout, "0.40.17", &release_zip("0.40.17", &[])).unwrap_err();
        assert!(matches!(err, EngineError::Io(_)), "{err}");
        assert_eq!(fs::read(layout.jar()).unwrap(), jar_bytes("0.40.8"));
        assert!(layout.content_dir().join("maps/haven/map.yml").is_file());
        assert!(!layout.staging_dir().exists());
    }

    #[test]
    fn a_replaced_jar_is_reported_as_modified() {
        let dir = tempfile::tempdir().unwrap();
        let layout = accepted_layout(dir.path());
        assert_eq!(installed(&layout), EngineState::NotInstalled);
        install(&layout, "0.40.17", &[]);
        fs::write(layout.jar(), b"PK\x03\x04tampered").unwrap();
        assert_eq!(installed(&layout), EngineState::Modified);
        assert!(!is_ready(&layout));
    }

    /// Serves one fixed body to every request on a loopback port.
    fn serve_body(body: Vec<u8>) -> String {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!(
            "http://{}/Tortodrone-v0.40.17.zip",
            listener.local_addr().unwrap()
        );
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let mut stream = stream;
                let mut buf = [0u8; 1024];
                let _ = stream.read(&mut buf);
                let head = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(head.as_bytes());
                let _ = stream.write_all(&body);
            }
        });
        url
    }

    #[test]
    fn a_downloaded_archive_is_checked_against_the_published_digest_then_installed() {
        let dir = tempfile::tempdir().unwrap();
        let layout = accepted_layout(dir.path());
        let zip = release_zip("0.40.17", &[]);
        let url = serve_body(zip.clone());
        let client = Client::plain_http_for_tests();
        let dest = layout.download_path();
        let mut seen = Vec::new();
        fetch_archive(&client, &url, &net::sha256_hex(&zip), &dest, &mut |s| {
            seen.push(s)
        })
        .unwrap();
        assert!(matches!(seen.last(), Some(Stage::Downloading(p)) if p.done == zip.len() as u64));
        let record = install_archive(
            &layout,
            &dest,
            &release_for("0.40.17", &zip),
            5,
            &mut |_| {},
        )
        .unwrap();
        assert_eq!(record.archive_sha256, net::sha256_hex(&zip));

        // A different body than the digest promised leaves nothing behind.
        let other = dir.path().join("other.zip");
        let err = fetch_archive(&client, &url, &"ab".repeat(32), &other, &mut |_| {}).unwrap_err();
        assert!(matches!(err, EngineError::ChecksumMismatch), "{err}");
        assert!(!other.exists());
    }

    #[test]
    fn the_launcher_client_refuses_a_plain_http_archive_url() {
        let dir = tempfile::tempdir().unwrap();
        let err = fetch_archive(
            &Client::new(),
            "http://example.invalid/x.zip",
            &"00".repeat(32),
            &dir.path().join("x.zip"),
            &mut |_| {},
        )
        .unwrap_err();
        assert!(
            matches!(err, EngineError::Net(NetError::InsecureUrl(_))),
            "{err}"
        );
    }

    #[test]
    fn stages_are_reported_in_order() {
        let dir = tempfile::tempdir().unwrap();
        let layout = accepted_layout(dir.path());
        let zip = release_zip("0.40.17", &[]);
        let path = dir.path().join("r.zip");
        fs::write(&path, &zip).unwrap();
        let mut stages = Vec::new();
        install_archive(&layout, &path, &release_for("0.40.17", &zip), 1, &mut |s| {
            stages.push(s)
        })
        .unwrap();
        assert_eq!(
            stages,
            [
                Stage::Verifying,
                Stage::Unpacking,
                Stage::Installing,
                Stage::Done
            ]
        );
    }

    #[test]
    fn older_releases_with_licences_under_docs_are_understood() {
        let files: Vec<(String, Vec<u8>)> = zip_files("0.40.8")
            .into_iter()
            .filter(|(n, _)| n != "LICENSE" && n != "NOTICE" && !n.starts_with("licenses/"))
            .chain([
                ("docs/LICENSE".to_owned(), b"licence".to_vec()),
                ("docs/NOTICE".to_owned(), b"notice".to_vec()),
                ("docs/licenses/jline.txt".to_owned(), b"jline".to_vec()),
            ])
            .collect();
        let dir = tempfile::tempdir().unwrap();
        let layout = accepted_layout(dir.path());
        install_zip(&layout, "0.40.8", &zip_of(&files)).unwrap();
        assert_eq!(fs::read(layout.root().join("LICENSE")).unwrap(), b"licence");
        assert!(layout.root().join("licenses/jline.txt").is_file());
    }
}
