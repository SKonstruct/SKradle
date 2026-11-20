//! Keeping the mods folder in step with the owner's modpack, the `mods/` folder of
//! `github.com/SirDank/Spiral-Knights-Modpack`.
//!
//! The pack is read through the GitHub contents API (`api.github.com`), which lists each file with its **git blob
//! SHA-1** (`sha1("blob <length>\0" + bytes)`, what `git hash-object` prints). That is the decision rule: a local file
//! with the published hash is current and skipped; anything else (missing, different, a link) is downloaded again.
//! Every download is checked against the published hash **before** it replaces anything, so a truncated transfer, a
//! captive portal's page or a tampered response never becomes a mod. SHA-1 is a weak hash against a determined
//! attacker, but the listing itself comes over HTTPS from the same service and the files are only ever read by the
//! mod scanner, which treats every archive as hostile anyway ([`crate::jar`]).
//!
//! Rules, all enforced here and tested:
//! - only `.zip`, `.jar` and `.modpack` files are taken, with a plain one-component file name that does not start with
//!   a dot (the listing is remote data);
//! - a download URL must be `https://raw.githubusercontent.com/...` exactly: no other host, no user info, no port,
//!   no plain HTTP; the listing request itself goes to `api.github.com` only. (The HTTP client follows redirects, and
//!   the hash check is what covers a redirect target.);
//! - `.modpack-synced` in the mods folder lists the files this sync wrote, one name per line. A sync deletes a file
//!   only if that list names it **and** the pack no longer has it, so the player's own mods are never removed. A file
//!   that already matched the pack but was not written by a sync is not claimed;
//! - files are written to a temporary name beside the mod and renamed into place, so an interrupted run never leaves
//!   a truncated mod for the game to load;
//! - an empty listing is an error and deletes nothing; a failure on one file is reported and does not stop the
//!   others.
//!
//! A player's own file with the same name as a pack file is replaced by the pack's version (the name is the
//! identity) and from then on counts as synced.

use std::collections::{BTreeSet, HashSet};
use std::fmt;
use std::fs;
use std::io::{self, Read};
use std::path::Path;

use serde::Deserialize;
use sha1::{Digest, Sha1};

use crate::jar::{temp_path, write_atomic};
use crate::net::{hex, Client, DownloadOptions};

use super::model::Container;

/// The contents listing of the pack's `mods/` folder.
pub const LISTING_URL: &str =
    "https://api.github.com/repos/SirDank/Spiral-Knights-Modpack/contents/mods";
/// The file in the mods folder that records what a sync wrote.
pub const MANIFEST: &str = ".modpack-synced";
const MAX_LISTING_BYTES: u64 = 4 * 1024 * 1024;
/// Largest mod file downloaded (the biggest is 84 MB).
pub const MAX_MOD_BYTES: u64 = 256 * 1024 * 1024;
const DOWNLOAD_HOST: &str = "raw.githubusercontent.com";

/// The two requests a sync makes, so tests can use a fake.
pub trait PackHttp {
    /// The JSON listing of the pack's `mods/` folder.
    fn listing(&self) -> Result<Vec<u8>, String>;
    /// One file, streamed to `dest` (never held whole in memory), at most `max_bytes`. `url` has been checked by
    /// [`is_trusted_download_url`]. On an error nothing may be left at `dest`.
    fn download(&self, url: &str, dest: &Path, max_bytes: u64) -> Result<(), String>;
}

/// The real thing, over [`Client`] (HTTPS only, size caps).
pub struct GithubHttp {
    client: Client,
}

impl GithubHttp {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
        }
    }
}

impl Default for GithubHttp {
    fn default() -> Self {
        Self::new()
    }
}

impl PackHttp for GithubHttp {
    fn listing(&self) -> Result<Vec<u8>, String> {
        self.client
            .get_bytes(LISTING_URL, MAX_LISTING_BYTES)
            .map_err(|e| e.to_string())
    }

    fn download(&self, url: &str, dest: &Path, max_bytes: u64) -> Result<(), String> {
        if !is_trusted_download_url(url) {
            return Err(format!("refusing to download from {url}"));
        }
        self.client
            .download(url, dest, &DownloadOptions::new(max_bytes), &mut |_| {})
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}

/// One file the pack offers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteFile {
    pub name: String,
    pub url: String,
    /// The git blob SHA-1, lower-case hex.
    pub sha: String,
    pub size: Option<u64>,
}

#[derive(Debug)]
pub enum SyncError {
    /// The listing could not be fetched or is not a listing (a rate-limit answer, say).
    Listing(String),
    /// The pack lists no usable mods; nothing was changed.
    EmptyListing,
    Io(io::Error),
}

impl fmt::Display for SyncError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SyncError::Listing(why) => write!(f, "cannot read the modpack listing: {why}"),
            SyncError::EmptyListing => f.write_str("the modpack lists no mods"),
            SyncError::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for SyncError {}

impl From<io::Error> for SyncError {
    fn from(e: io::Error) -> Self {
        SyncError::Io(e)
    }
}

/// What a sync did.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SyncReport {
    pub downloaded: Vec<String>,
    pub skipped: Vec<String>,
    /// Files removed because the pack dropped them.
    pub deleted: Vec<String>,
    /// File name and why it could not be brought up to date.
    pub failed: Vec<(String, String)>,
    /// Listing entries that were ignored, and why.
    pub warnings: Vec<String>,
    /// The sync was stopped on request: files after the stop were not looked at and nothing was deleted.
    pub cancelled: bool,
}

impl SyncReport {
    /// True when every file of the pack is now current.
    pub fn is_complete(&self) -> bool {
        self.failed.is_empty()
    }
}

/// How far a sync has come.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncProgress {
    /// 1-based position of the file being handled.
    pub current: usize,
    pub total: usize,
    pub name: String,
}

/// Git's hash of a blob with these contents.
pub fn git_blob_sha1(bytes: &[u8]) -> String {
    let mut hasher = Sha1::new();
    hasher.update(format!("blob {}\0", bytes.len()).as_bytes());
    hasher.update(bytes);
    hex(hasher.finalize().as_slice())
}

/// [`git_blob_sha1`] of a file, read in chunks.
pub fn git_blob_sha1_file(path: &Path) -> io::Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha1::new();
    hasher.update(format!("blob {}\0", file.metadata()?.len()).as_bytes());
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            return Ok(hex(hasher.finalize().as_slice()));
        }
        hasher.update(&buf[..n]);
    }
}

/// A file name this sync may write or delete: one component, no dot at the start, a mod extension.
pub fn is_safe_mod_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 255
        && !name.starts_with('.')
        && matches!(
            crate::jar::safe_entry_path(name),
            Ok((ref clean, false)) if clean == name && !clean.contains('/')
        )
        && Container::from_file_name(name).is_some()
}

/// `https://raw.githubusercontent.com/<path>` and nothing else: no other host, no user info, no port, no HTTP.
pub fn is_trusted_download_url(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://") else {
        return false;
    };
    let Some((host, path)) = rest.split_once('/') else {
        return false;
    };
    host == DOWNLOAD_HOST
        && !path.is_empty()
        && !path
            .chars()
            .any(|c| c.is_control() || c.is_whitespace() || c == '\\')
        && !path.split(['/', '?', '#']).any(|part| part == "..")
}

#[derive(Deserialize)]
struct Entry {
    #[serde(default, rename = "type")]
    kind: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    download_url: Option<String>,
    #[serde(default)]
    sha: Option<String>,
    #[serde(default)]
    size: Option<u64>,
}

/// The usable files of a contents-API listing, plus a note for each entry that was ignored. A JSON document that is
/// not an array (GitHub's error answers are objects) is an error.
pub fn parse_listing(json: &[u8]) -> Result<(Vec<RemoteFile>, Vec<String>), SyncError> {
    let value: serde_json::Value =
        serde_json::from_slice(json).map_err(|e| SyncError::Listing(e.to_string()))?;
    let items = match value {
        serde_json::Value::Array(items) => items,
        serde_json::Value::Object(map) => {
            let message = map
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("not a listing");
            return Err(SyncError::Listing(message.to_owned()));
        }
        _ => return Err(SyncError::Listing("not a listing".to_owned())),
    };
    let mut files = Vec::new();
    let mut warnings = Vec::new();
    let mut seen = HashSet::new();
    for item in items {
        let Ok(entry) = serde_json::from_value::<Entry>(item) else {
            warnings.push("a listing entry could not be read".to_owned());
            continue;
        };
        if entry.kind != "file" {
            continue;
        }
        let name = entry.name;
        if !is_safe_mod_name(&name) {
            warnings.push(format!("{name:?} is not a safe mod file name, ignored"));
            continue;
        }
        let Some(url) = entry.download_url.filter(|u| is_trusted_download_url(u)) else {
            warnings.push(format!("{name}: no trusted download address, ignored"));
            continue;
        };
        let Some(sha) = entry
            .sha
            .filter(|s| s.len() == 40 && s.bytes().all(|b| b.is_ascii_hexdigit()))
        else {
            warnings.push(format!("{name}: no usable hash, ignored"));
            continue;
        };
        if entry.size.is_some_and(|s| s > MAX_MOD_BYTES) {
            warnings.push(format!("{name}: larger than the limit, ignored"));
            continue;
        }
        if !seen.insert(name.to_lowercase()) {
            warnings.push(format!(
                "{name}: listed twice (names differing only in case), ignored"
            ));
            continue;
        }
        files.push(RemoteFile {
            name,
            url,
            sha: sha.to_ascii_lowercase(),
            size: entry.size,
        });
    }
    Ok((files, warnings))
}

/// What is at the local path of a pack file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Local {
    Absent,
    /// A regular file with this git blob SHA-1.
    Regular {
        sha: String,
    },
    /// A link: replaced, never followed.
    Link,
    /// A folder (or something else that is not a file): cannot be replaced.
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Why {
    Missing,
    Changed,
    /// A link stands where the file belongs; it is replaced by the file.
    ReplacesLink,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    Skip,
    Download(Why),
    /// Something that cannot be replaced is in the way.
    Blocked,
}

/// Skip, download or refuse, from the local state and the hash the listing publishes.
pub fn decide(local: &Local, published_sha: &str) -> Decision {
    match local {
        Local::Absent => Decision::Download(Why::Missing),
        Local::Regular { sha } if sha.eq_ignore_ascii_case(published_sha) => Decision::Skip,
        Local::Regular { .. } => Decision::Download(Why::Changed),
        Local::Link => Decision::Download(Why::ReplacesLink),
        Local::Other => Decision::Blocked,
    }
}

fn classify(path: &Path) -> io::Result<Local> {
    match fs::symlink_metadata(path) {
        Ok(m) if m.file_type().is_symlink() => Ok(Local::Link),
        Ok(m) if m.is_file() => Ok(Local::Regular {
            sha: git_blob_sha1_file(path)?,
        }),
        Ok(_) => Ok(Local::Other),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Local::Absent),
        Err(e) => Err(e),
    }
}

/// The names a sync recorded earlier. Anything that is not a safe mod name is dropped, so a hand-edited or hostile
/// manifest cannot point a deletion anywhere else.
fn read_manifest(dir: &Path) -> BTreeSet<String> {
    let Ok(bytes) = crate::bounded::read_regular(&dir.join(MANIFEST), 1024 * 1024) else {
        return BTreeSet::new();
    };
    String::from_utf8_lossy(&bytes)
        .lines()
        .map(str::trim)
        .filter(|l| is_safe_mod_name(l))
        .map(str::to_owned)
        .collect()
}

fn write_manifest(dir: &Path, names: &BTreeSet<String>) -> io::Result<()> {
    let path = dir.join(MANIFEST);
    if names.is_empty() {
        return match fs::remove_file(&path) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        };
    }
    let mut text = names.iter().cloned().collect::<Vec<_>>().join("\n");
    text.push('\n');
    write_atomic(&path, text.as_bytes())
}

/// Brings `mods_dir` in line with the pack using the real GitHub endpoints.
pub fn sync_pack(
    mods_dir: &Path,
    progress: &mut dyn FnMut(&SyncProgress),
) -> Result<SyncReport, SyncError> {
    sync(mods_dir, &GithubHttp::new(), progress)
}

/// Brings `mods_dir` in line with the pack: downloads what is new or changed (checking each file's hash before it
/// lands), then removes what an earlier sync wrote and the pack has dropped. See the module docs for the rules.
pub fn sync(
    mods_dir: &Path,
    http: &dyn PackHttp,
    progress: &mut dyn FnMut(&SyncProgress),
) -> Result<SyncReport, SyncError> {
    sync_with(mods_dir, http, progress, CASE_INSENSITIVE_FS)
}

/// [`sync`] that can be stopped: `cancel` is asked before each file, and a `true` ends the sync there. The file being
/// downloaded at that moment still completes (a download is never cut in half). A cancelled sync deletes nothing and
/// keeps every claim an earlier sync made, so a later sync picks up where this one stopped.
pub fn sync_cancellable(
    mods_dir: &Path,
    http: &dyn PackHttp,
    progress: &mut dyn FnMut(&SyncProgress),
    cancel: &dyn Fn() -> bool,
) -> Result<SyncReport, SyncError> {
    sync_inner(mods_dir, http, progress, CASE_INSENSITIVE_FS, cancel)
}

/// What a sync would do, without doing it.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct PackStatus {
    /// Files the pack lists.
    pub total: usize,
    /// Pack files that are missing here or differ from the published hash.
    pub to_download: Vec<String>,
    /// Files an earlier sync wrote that the pack has dropped.
    pub to_delete: Vec<String>,
    /// An earlier sync wrote at least one file that is still here.
    pub installed: bool,
}

impl PackStatus {
    /// True when a sync would change something.
    pub fn has_changes(&self) -> bool {
        !self.to_download.is_empty() || !self.to_delete.is_empty()
    }
}

/// Reads the pack's listing and compares it with `mods_dir` (one request, no download).
pub fn status(mods_dir: &Path, http: &dyn PackHttp) -> Result<PackStatus, SyncError> {
    let fold_case = CASE_INSENSITIVE_FS;
    let listing = http.listing().map_err(SyncError::Listing)?;
    let (remote, _) = parse_listing(&listing)?;
    if remote.is_empty() {
        return Err(SyncError::EmptyListing);
    }
    let previous = read_manifest(mods_dir);
    let remote_keys: HashSet<String> = remote
        .iter()
        .map(|f| name_key(&f.name, fold_case))
        .collect();
    let mut out = PackStatus {
        total: remote.len(),
        ..PackStatus::default()
    };
    for file in &remote {
        let dest = mods_dir.join(&file.name);
        if let Ok(local) = classify(&dest) {
            if matches!(decide(&local, &file.sha), Decision::Download(_)) {
                out.to_download.push(file.name.clone());
            }
        }
    }
    for name in &previous {
        let exists = fs::symlink_metadata(mods_dir.join(name)).is_ok_and(|m| m.is_file());
        if exists {
            out.installed = true;
            if !remote_keys.contains(&name_key(name, fold_case)) {
                out.to_delete.push(name.clone());
            }
        }
    }
    Ok(out)
}

/// Whether file names that differ only in case are the same file on this platform's usual file systems.
const CASE_INSENSITIVE_FS: bool = cfg!(any(windows, target_os = "macos"));

/// How names are compared when deciding whether a file is still in the pack.
fn name_key(name: &str, fold_case: bool) -> String {
    if fold_case {
        name.to_lowercase()
    } else {
        name.to_owned()
    }
}

/// [`sync`] with the name comparison chosen by the caller, so the case-insensitive behaviour is testable on any
/// file system. With `fold_case`, a pack file whose name changed only in letter case is the same file: it is kept,
/// not deleted as dropped (on such a file system the new name resolves to the old file).
fn sync_with(
    mods_dir: &Path,
    http: &dyn PackHttp,
    progress: &mut dyn FnMut(&SyncProgress),
    fold_case: bool,
) -> Result<SyncReport, SyncError> {
    sync_inner(mods_dir, http, progress, fold_case, &|| false)
}

fn sync_inner(
    mods_dir: &Path,
    http: &dyn PackHttp,
    progress: &mut dyn FnMut(&SyncProgress),
    fold_case: bool,
    cancel: &dyn Fn() -> bool,
) -> Result<SyncReport, SyncError> {
    fs::create_dir_all(mods_dir)?;
    let listing = http.listing().map_err(SyncError::Listing)?;
    let (remote, warnings) = parse_listing(&listing)?;
    if remote.is_empty() {
        return Err(SyncError::EmptyListing);
    }
    let mut report = SyncReport {
        warnings,
        ..SyncReport::default()
    };
    let previous = read_manifest(mods_dir);
    let previous_keys: HashSet<String> = previous.iter().map(|n| name_key(n, fold_case)).collect();
    let remote_keys: HashSet<String> = remote
        .iter()
        .map(|f| name_key(&f.name, fold_case))
        .collect();
    // Files this sync is responsible for after it ends.
    let mut owned: BTreeSet<String> = BTreeSet::new();

    for (i, file) in remote.iter().enumerate() {
        if cancel() {
            report.cancelled = true;
            break;
        }
        progress(&SyncProgress {
            current: i + 1,
            total: remote.len(),
            name: file.name.clone(),
        });
        let dest = mods_dir.join(&file.name);
        let outcome = (|| -> Result<bool, String> {
            let local = classify(&dest).map_err(|e| e.to_string())?;
            match decide(&local, &file.sha) {
                Decision::Skip => Ok(false),
                Decision::Blocked => Err("a folder is in the way".to_owned()),
                Decision::Download(_) => {
                    // Streamed to a temporary file next to the target, hashed from disk in chunks, then renamed
                    // over it: a mod is never held whole in memory and `dest` is only ever old or complete.
                    let tmp = temp_path(&dest);
                    let fetched = http
                        .download(&file.url, &tmp, MAX_MOD_BYTES)
                        .and_then(|()| match git_blob_sha1_file(&tmp) {
                            Ok(sha) if sha == file.sha => Ok(()),
                            Ok(_) => {
                                Err("the download did not match its published hash".to_owned())
                            }
                            Err(e) => Err(e.to_string()),
                        })
                        .and_then(|()| fs::rename(&tmp, &dest).map_err(|e| e.to_string()));
                    if let Err(why) = fetched {
                        let _ = fs::remove_file(&tmp);
                        return Err(why);
                    }
                    Ok(true)
                }
            }
        })();
        match outcome {
            Ok(true) => {
                owned.insert(file.name.clone());
                report.downloaded.push(file.name.clone());
            }
            Ok(false) => {
                if previous_keys.contains(&name_key(&file.name, fold_case)) {
                    owned.insert(file.name.clone());
                }
                report.skipped.push(file.name.clone());
            }
            Err(why) => {
                if previous_keys.contains(&name_key(&file.name, fold_case)) && dest.exists() {
                    owned.insert(file.name.clone());
                }
                report.failed.push((file.name.clone(), why));
            }
        }
    }

    if report.cancelled {
        // Stopped on request: keep every earlier claim, delete nothing.
        owned.extend(previous.iter().cloned());
        write_manifest(mods_dir, &owned)?;
        return Ok(report);
    }

    // Remove what the pack dropped, but only files an earlier sync wrote.
    for name in &previous {
        if remote_keys.contains(&name_key(name, fold_case)) {
            continue;
        }
        let path = mods_dir.join(name);
        match fs::symlink_metadata(&path) {
            Ok(m) if m.is_file() => match fs::remove_file(&path) {
                Ok(()) => report.deleted.push(name.clone()),
                Err(e) => {
                    report
                        .failed
                        .push((name.clone(), format!("could not remove it: {e}")));
                    owned.insert(name.clone());
                }
            },
            // Already gone, or something that is not a plain file: not ours to touch, and no longer tracked.
            _ => {}
        }
    }

    write_manifest(mods_dir, &owned)?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::HashMap;

    /// A pack in memory. `files` are what the listing advertises; `served` is what a download returns (it can differ,
    /// to model a tampered or truncated transfer).
    #[derive(Default)]
    struct FakePack {
        files: Vec<(String, Vec<u8>)>,
        served: RefCell<HashMap<String, Vec<u8>>>,
        downloads: RefCell<Vec<String>>,
        raw_listing: Option<Vec<u8>>,
    }

    impl FakePack {
        fn with(files: &[(&str, &[u8])]) -> Self {
            Self {
                files: files
                    .iter()
                    .map(|(n, b)| (n.to_string(), b.to_vec()))
                    .collect(),
                ..Self::default()
            }
        }

        fn url(name: &str) -> String {
            format!(
                "https://raw.githubusercontent.com/SirDank/Spiral-Knights-Modpack/main/mods/{name}"
            )
        }

        fn tamper(&self, name: &str, bytes: &[u8]) {
            self.served
                .borrow_mut()
                .insert(Self::url(name), bytes.to_vec());
        }
    }

    impl PackHttp for FakePack {
        fn listing(&self) -> Result<Vec<u8>, String> {
            if let Some(raw) = &self.raw_listing {
                return Ok(raw.clone());
            }
            let entries: Vec<serde_json::Value> = self
                .files
                .iter()
                .map(|(name, bytes)| {
                    serde_json::json!({
                        "type": "file", "name": name, "sha": git_blob_sha1(bytes),
                        "size": bytes.len(), "download_url": Self::url(name),
                    })
                })
                .collect();
            Ok(serde_json::to_vec(&entries).unwrap())
        }

        fn download(&self, url: &str, dest: &Path, _max: u64) -> Result<(), String> {
            self.downloads.borrow_mut().push(url.to_owned());
            let bytes = match self.served.borrow().get(url) {
                Some(b) => b.clone(),
                None => self
                    .files
                    .iter()
                    .find(|(n, _)| Self::url(n) == url)
                    .map(|(_, b)| b.clone())
                    .ok_or_else(|| "404".to_owned())?,
            };
            fs::write(dest, bytes).map_err(|e| e.to_string())
        }
    }

    #[test]
    fn a_case_only_rename_is_the_same_file_when_names_are_compared_case_insensitively() {
        // Injected comparison, so this holds on a case-sensitive test machine too.
        let dir = tempfile::tempdir().unwrap();
        sync_with(
            dir.path(),
            &FakePack::with(&[("MyMod.zip", b"AAA")]),
            &mut |_| {},
            true,
        )
        .unwrap();
        let report = sync_with(
            dir.path(),
            &FakePack::with(&[("mymod.zip", b"AAA")]),
            &mut |_| {},
            true,
        )
        .unwrap();
        assert!(
            report.failed.is_empty() && report.deleted.is_empty(),
            "{report:?}"
        );
        // The pack's current spelling is the one tracked.
        assert_eq!(
            fs::read_to_string(dir.path().join(MANIFEST)).unwrap(),
            "mymod.zip\n"
        );
        assert!(dir.path().join("MyMod.zip").exists() || dir.path().join("mymod.zip").exists());
    }

    #[test]
    fn names_that_differ_in_case_are_different_files_when_compared_exactly() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("probe.TMP"), b"").unwrap();
        if dir.path().join("PROBE.tmp").exists() {
            return; // this file system cannot tell them apart
        }
        sync_with(
            dir.path(),
            &FakePack::with(&[("MyMod.zip", b"A")]),
            &mut |_| {},
            false,
        )
        .unwrap();
        let report = sync_with(
            dir.path(),
            &FakePack::with(&[("mymod.zip", b"A")]),
            &mut |_| {},
            false,
        )
        .unwrap();
        assert_eq!(report.deleted, ["MyMod.zip"]);
        assert!(dir.path().join("mymod.zip").exists());
    }

    #[test]
    fn a_case_only_rename_on_this_file_system_leaves_a_synced_file_behind() {
        // The real thing, where the temp folder happens to be case-insensitive (the default on macOS and Windows).
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("probe.TMP"), b"").unwrap();
        if !dir.path().join("PROBE.tmp").exists() {
            return;
        }
        run(dir.path(), &FakePack::with(&[("MyMod.zip", b"AAA")]));
        run(dir.path(), &FakePack::with(&[("mymod.zip", b"AAA")]));
        assert_eq!(fs::read(dir.path().join("mymod.zip")).unwrap(), b"AAA");
        run(dir.path(), &FakePack::with(&[("mymod.zip", b"BBB")]));
        assert_eq!(fs::read(dir.path().join("mymod.zip")).unwrap(), b"BBB");
    }

    fn run(dir: &Path, pack: &FakePack) -> SyncReport {
        sync(dir, pack, &mut |_| {}).unwrap()
    }

    #[test]
    fn git_blob_hashes_match_git() {
        assert_eq!(
            git_blob_sha1(b""),
            "e69de29bb2d1d6434b8b29ae775ad8c2e48c5391"
        );
        assert_eq!(
            git_blob_sha1(b"hello\n"),
            "ce013625030ba8dba906f756967f9e9ca394464a"
        );
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("f"), b"hello\n").unwrap();
        assert_eq!(
            git_blob_sha1_file(&dir.path().join("f")).unwrap(),
            "ce013625030ba8dba906f756967f9e9ca394464a"
        );
    }

    #[test]
    fn decisions_table() {
        let sha = "ab".repeat(20);
        let regular = |s: &str| Local::Regular { sha: s.to_owned() };
        let cases = [
            (Local::Absent, Decision::Download(Why::Missing)),
            (regular(&sha), Decision::Skip),
            (regular(&sha.to_uppercase()), Decision::Skip),
            (regular(&"cd".repeat(20)), Decision::Download(Why::Changed)),
            (regular(""), Decision::Download(Why::Changed)),
            (Local::Link, Decision::Download(Why::ReplacesLink)),
            (Local::Other, Decision::Blocked),
        ];
        for (local, expected) in cases {
            assert_eq!(decide(&local, &sha), expected, "{local:?}");
        }
    }

    #[test]
    fn safe_names_and_trusted_urls() {
        for ok in [
            "a.zip",
            "Some Mod (v2).zip",
            "x.JAR",
            "pack.modpack",
            "a.b.c.zip",
        ] {
            assert!(is_safe_mod_name(ok), "{ok}");
        }
        for bad in [
            "",
            ".hidden.zip",
            "../a.zip",
            "a/b.zip",
            "a\\b.zip",
            "a.txt",
            "a.zip.part",
            "a.zip ",
            "zip",
            ".modpack-synced",
            "CON.zip",
            "a\0.zip",
            "a\n.zip",
            "a:b.zip",
            "/abs.zip",
        ] {
            assert!(!is_safe_mod_name(bad), "{bad:?}");
        }
        assert!(is_trusted_download_url(
            "https://raw.githubusercontent.com/o/r/main/mods/a.zip"
        ));
        for bad in [
            "http://raw.githubusercontent.com/o/r/a.zip",
            "https://raw.githubusercontent.com.evil.com/a.zip",
            "https://evil.com/raw.githubusercontent.com/a.zip",
            "https://raw.githubusercontent.com@evil.com/a.zip",
            "https://user:pw@raw.githubusercontent.com/a.zip",
            "https://raw.githubusercontent.com:8443/a.zip",
            "https://raw.githubusercontent.com/",
            "https://raw.githubusercontent.com",
            "https://raw.githubusercontent.com/a/../../b.zip",
            "https://raw.githubusercontent.com/a b.zip",
            "https://raw.githubusercontent.com/a\\b.zip",
            "ftp://raw.githubusercontent.com/a.zip",
            "raw.githubusercontent.com/a.zip",
            "",
        ] {
            assert!(!is_trusted_download_url(bad), "{bad:?}");
        }
    }

    #[test]
    fn parses_a_listing_and_skips_hostile_entries() {
        let good_sha = "a".repeat(40);
        let url = FakePack::url("ok.zip");
        let listing = serde_json::json!([
            {"type": "file", "name": "ok.zip", "sha": good_sha, "download_url": url, "size": 10},
            {"type": "dir", "name": "sub", "sha": good_sha},
            {"type": "file", "name": "../evil.zip", "sha": good_sha, "download_url": url},
            {"type": "file", "name": "notes.txt", "sha": good_sha, "download_url": url},
            {"type": "file", "name": "badhost.zip", "sha": good_sha, "download_url": "https://evil.com/a.zip"},
            {"type": "file", "name": "http.zip", "sha": good_sha, "download_url": "http://raw.githubusercontent.com/a.zip"},
            {"type": "file", "name": "nourl.zip", "sha": good_sha, "download_url": null},
            {"type": "file", "name": "nosha.zip", "download_url": url},
            {"type": "file", "name": "shortsha.zip", "sha": "abc", "download_url": url},
            {"type": "file", "name": "nonhex.zip", "sha": "z".repeat(40), "download_url": url},
            {"type": "file", "name": "huge.zip", "sha": good_sha, "download_url": url, "size": 99999999999u64},
            {"type": "file", "name": "OK.ZIP", "sha": good_sha, "download_url": url},
            42,
            "string"
        ]);
        let (files, warnings) = parse_listing(&serde_json::to_vec(&listing).unwrap()).unwrap();
        assert_eq!(files.len(), 1, "{files:?}");
        assert_eq!(files[0].name, "ok.zip");
        assert!(warnings.len() >= 10, "{warnings:?}");
    }

    #[test]
    fn a_listing_that_is_not_a_list_is_an_error() {
        for bad in [
            &b"{\"message\": \"API rate limit exceeded\"}"[..],
            b"not json",
            b"null",
            b"3",
            b"",
        ] {
            assert!(
                matches!(parse_listing(bad), Err(SyncError::Listing(_))),
                "{bad:?}"
            );
        }
        let err = parse_listing(b"{\"message\": \"API rate limit exceeded\"}").unwrap_err();
        assert!(err.to_string().contains("rate limit"));
    }

    #[test]
    fn first_sync_downloads_everything_and_the_second_skips_everything() {
        let dir = tempfile::tempdir().unwrap();
        let pack = FakePack::with(&[("a.zip", b"AAA"), ("b.jar", b"BBB"), ("c.modpack", b"CCC")]);
        let first = run(dir.path(), &pack);
        assert_eq!(first.downloaded, ["a.zip", "b.jar", "c.modpack"]);
        assert!(first.is_complete());
        assert_eq!(fs::read(dir.path().join("b.jar")).unwrap(), b"BBB");
        assert_eq!(
            fs::read_to_string(dir.path().join(MANIFEST)).unwrap(),
            "a.zip\nb.jar\nc.modpack\n"
        );
        let second = run(dir.path(), &pack);
        assert_eq!(second.skipped.len(), 3);
        assert!(second.downloaded.is_empty());
        assert_eq!(pack.downloads.borrow().len(), 3, "no new downloads");
    }

    #[test]
    fn changed_missing_and_tampered_local_files_are_downloaded_again() {
        let dir = tempfile::tempdir().unwrap();
        let pack = FakePack::with(&[("a.zip", b"AAA"), ("b.zip", b"BBB"), ("c.zip", b"CCC")]);
        run(dir.path(), &pack);
        fs::write(dir.path().join("a.zip"), b"tampered").unwrap();
        fs::remove_file(dir.path().join("b.zip")).unwrap();
        let report = run(dir.path(), &pack);
        assert_eq!(report.downloaded, ["a.zip", "b.zip"]);
        assert_eq!(report.skipped, ["c.zip"]);
        assert_eq!(fs::read(dir.path().join("a.zip")).unwrap(), b"AAA");
        // The pack publishes a new version.
        let newer = FakePack::with(&[("a.zip", b"AAA2"), ("b.zip", b"BBB"), ("c.zip", b"CCC")]);
        let report = run(dir.path(), &newer);
        assert_eq!(report.downloaded, ["a.zip"]);
        assert_eq!(fs::read(dir.path().join("a.zip")).unwrap(), b"AAA2");
    }

    #[test]
    fn a_download_that_does_not_match_its_hash_never_replaces_anything() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.zip"), b"old good copy").unwrap();
        let pack = FakePack::with(&[("a.zip", b"AAA"), ("new.zip", b"NNN")]);
        pack.tamper("a.zip", b"<html>captive portal</html>");
        pack.tamper("new.zip", b"NN"); // truncated
        let report = run(dir.path(), &pack);
        assert!(report.downloaded.is_empty());
        assert_eq!(report.failed.len(), 2);
        assert!(report.failed[0].1.contains("hash"), "{report:?}");
        assert!(!report.is_complete());
        assert_eq!(
            fs::read(dir.path().join("a.zip")).unwrap(),
            b"old good copy"
        );
        assert!(!dir.path().join("new.zip").exists());
        let leftovers: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        assert!(
            leftovers.iter().all(|n| !n.contains("skradle-tmp")),
            "{leftovers:?}"
        );
    }

    #[test]
    fn one_failure_does_not_stop_the_other_files() {
        let dir = tempfile::tempdir().unwrap();
        let pack = FakePack::with(&[("a.zip", b"AAA"), ("b.zip", b"BBB")]);
        pack.tamper("a.zip", b"wrong");
        let report = run(dir.path(), &pack);
        assert_eq!(report.downloaded, ["b.zip"]);
        assert_eq!(report.failed.len(), 1);
        assert_eq!(
            fs::read_to_string(dir.path().join(MANIFEST)).unwrap(),
            "b.zip\n"
        );
    }

    #[test]
    fn only_files_a_sync_wrote_are_removed_when_the_pack_drops_them() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("mine.zip"), b"the player's own mod").unwrap();
        fs::write(dir.path().join("also_mine.zip"), b"another").unwrap();
        let pack = FakePack::with(&[("a.zip", b"AAA"), ("b.zip", b"BBB")]);
        run(dir.path(), &pack);
        // The pack drops b.zip and also never listed the player's files.
        let smaller = FakePack::with(&[("a.zip", b"AAA")]);
        let report = run(dir.path(), &smaller);
        assert_eq!(report.deleted, ["b.zip"]);
        assert!(!dir.path().join("b.zip").exists());
        assert!(dir.path().join("mine.zip").exists());
        assert!(dir.path().join("also_mine.zip").exists());
        assert_eq!(
            fs::read_to_string(dir.path().join(MANIFEST)).unwrap(),
            "a.zip\n"
        );
        // Dropped again, now untracked: nothing else is touched.
        let report = run(dir.path(), &smaller);
        assert!(report.deleted.is_empty());
    }

    #[test]
    fn a_player_file_that_merely_matches_the_pack_is_not_claimed() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.zip"), b"AAA").unwrap();
        let pack = FakePack::with(&[("a.zip", b"AAA")]);
        let report = run(dir.path(), &pack);
        assert_eq!(report.skipped, ["a.zip"]);
        assert!(!dir.path().join(MANIFEST).exists());
        // When the pack drops it, the player's copy stays.
        let other = FakePack::with(&[("z.zip", b"ZZZ")]);
        run(dir.path(), &other);
        assert!(dir.path().join("a.zip").exists());
    }

    #[test]
    fn a_hostile_manifest_cannot_aim_a_deletion_elsewhere() {
        let outer = tempfile::tempdir().unwrap();
        let dir = outer.path().join("mods");
        fs::create_dir_all(&dir).unwrap();
        fs::write(outer.path().join("victim.zip"), b"precious").unwrap();
        fs::write(dir.join("sub.zip"), b"x").unwrap();
        fs::create_dir(dir.join("folder.zip")).unwrap();
        fs::write(
            dir.join(MANIFEST),
            "../victim.zip\n/etc/hosts\na/b.zip\n..\n\nnotes.txt\nfolder.zip\n.hidden.zip\n",
        )
        .unwrap();
        let pack = FakePack::with(&[("a.zip", b"AAA")]);
        let report = run(&dir, &pack);
        assert!(report.deleted.is_empty(), "{report:?}");
        assert!(outer.path().join("victim.zip").exists());
        assert!(dir.join("folder.zip").is_dir());
        assert!(dir.join("sub.zip").exists());
        assert_eq!(fs::read_to_string(dir.join(MANIFEST)).unwrap(), "a.zip\n");
    }

    #[test]
    fn an_empty_or_broken_listing_changes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let pack = FakePack::with(&[("a.zip", b"AAA")]);
        run(dir.path(), &pack);
        let before = fs::read(dir.path().join("a.zip")).unwrap();
        for raw in [
            &b"[]"[..],
            b"{\"message\": \"rate limited\"}",
            b"garbage",
            b"[{\"type\": \"dir\"}]",
        ] {
            let broken = FakePack {
                raw_listing: Some(raw.to_vec()),
                ..FakePack::default()
            };
            assert!(sync(dir.path(), &broken, &mut |_| {}).is_err(), "{raw:?}");
            assert_eq!(fs::read(dir.path().join("a.zip")).unwrap(), before);
            assert!(dir.path().join(MANIFEST).exists(), "the manifest is kept");
        }
    }

    #[test]
    fn a_folder_in_the_way_is_reported_and_a_link_is_replaced_not_followed() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("a.zip")).unwrap();
        let pack = FakePack::with(&[("a.zip", b"AAA"), ("b.zip", b"BBB")]);
        let report = run(dir.path(), &pack);
        assert_eq!(report.failed.len(), 1);
        assert!(report.failed[0].1.contains("in the way"));
        assert_eq!(report.downloaded, ["b.zip"]);
        #[cfg(unix)]
        {
            let outside = tempfile::tempdir().unwrap();
            fs::write(outside.path().join("victim"), b"precious").unwrap();
            fs::remove_file(dir.path().join("b.zip")).unwrap();
            std::os::unix::fs::symlink(outside.path().join("victim"), dir.path().join("b.zip"))
                .unwrap();
            let report = run(dir.path(), &pack);
            assert_eq!(report.downloaded, ["b.zip"]);
            assert_eq!(
                fs::read(outside.path().join("victim")).unwrap(),
                b"precious"
            );
            assert!(!fs::symlink_metadata(dir.path().join("b.zip"))
                .unwrap()
                .file_type()
                .is_symlink());
            assert_eq!(fs::read(dir.path().join("b.zip")).unwrap(), b"BBB");
        }
    }

    #[test]
    fn progress_is_reported_for_every_file() {
        let dir = tempfile::tempdir().unwrap();
        let pack = FakePack::with(&[("a.zip", b"A"), ("b.zip", b"B")]);
        let mut seen = Vec::new();
        sync(dir.path(), &pack, &mut |p| {
            seen.push((p.current, p.total, p.name.clone()))
        })
        .unwrap();
        assert_eq!(
            seen,
            [(1, 2, "a.zip".to_owned()), (2, 2, "b.zip".to_owned())]
        );
    }

    #[test]
    fn the_real_http_adapter_refuses_untrusted_urls_without_a_request() {
        let http = GithubHttp::new();
        let dest = std::env::temp_dir().join("skradle-never-written.zip");
        assert!(http
            .download("http://raw.githubusercontent.com/a.zip", &dest, 10)
            .unwrap_err()
            .contains("refusing"));
        assert!(http
            .download("https://evil.example/a.zip", &dest, 10)
            .unwrap_err()
            .contains("refusing"));
        assert!(!dest.exists());
    }

    #[test]
    fn a_download_is_verified_on_disk_and_leaves_no_temporary_file_behind() {
        let dir = tempfile::tempdir().unwrap();
        let pack = FakePack::with(&[("good.zip", b"GOOD"), ("bad.zip", b"BAD")]);
        // The listing promises one hash, the server then delivers other bytes.
        pack.tamper("bad.zip", b"EVIL");
        let report = sync_with(dir.path(), &pack, &mut |_| {}, false).unwrap();
        assert_eq!(report.downloaded, ["good.zip"]);
        assert_eq!(report.failed.len(), 1);
        assert_eq!(report.failed[0].0, "bad.zip");
        assert_eq!(fs::read(dir.path().join("good.zip")).unwrap(), b"GOOD");
        assert!(!dir.path().join("bad.zip").exists());
        let leftovers: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".skradle-tmp") || n.ends_with(".part"))
            .collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");
    }

    #[test]
    fn a_cancelled_sync_stops_before_the_next_file_and_deletes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let first = FakePack::with(&[("old.zip", b"OLD")]);
        run(dir.path(), &first);
        let pack = FakePack::with(&[("a.zip", b"A"), ("b.zip", b"B")]);
        let asked = std::cell::Cell::new(0);
        let report = sync_cancellable(dir.path(), &pack, &mut |_| {}, &|| {
            asked.set(asked.get() + 1);
            asked.get() > 1
        })
        .unwrap();
        assert!(report.cancelled);
        assert_eq!(report.downloaded, ["a.zip"]);
        assert!(report.deleted.is_empty());
        assert!(dir.path().join("old.zip").exists(), "nothing is deleted");
        assert!(!dir.path().join("b.zip").exists());
        // The earlier claim survives, so a full sync later still removes the dropped file.
        let again = run(dir.path(), &pack);
        assert_eq!(again.deleted, ["old.zip"]);
        assert!(!again.cancelled);
    }

    #[test]
    fn status_reports_changes_without_touching_anything() {
        let dir = tempfile::tempdir().unwrap();
        let pack = FakePack::with(&[("a.zip", b"A"), ("b.zip", b"B")]);
        let fresh = status(dir.path(), &pack).unwrap();
        assert_eq!(fresh.total, 2);
        assert_eq!(fresh.to_download, ["a.zip", "b.zip"]);
        assert!(!fresh.installed && fresh.has_changes());
        assert!(pack.downloads.borrow().is_empty());
        assert!(fs::read_dir(dir.path()).unwrap().next().is_none());

        run(dir.path(), &pack);
        let current = status(dir.path(), &pack).unwrap();
        assert!(current.installed && !current.has_changes());

        let newer = FakePack::with(&[("a.zip", b"A2")]);
        let s = status(dir.path(), &newer).unwrap();
        assert_eq!(s.to_download, ["a.zip"]);
        assert_eq!(s.to_delete, ["b.zip"]);
        // A player's own file is never offered for deletion.
        fs::write(dir.path().join("mine.zip"), b"M").unwrap();
        assert!(!status(dir.path(), &newer)
            .unwrap()
            .to_delete
            .contains(&"mine.zip".to_owned()));
    }

    #[test]
    fn status_of_an_unusable_listing_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let pack = FakePack {
            raw_listing: Some(b"{\"message\":\"rate limit\"}".to_vec()),
            ..FakePack::default()
        };
        assert!(status(dir.path(), &pack).is_err());
    }

    /// Reads the real listing (network, api.github.com only) and prints it. Changes nothing on disk.
    #[test]
    #[ignore = "needs network access to api.github.com"]
    fn live_listing() {
        let bytes = GithubHttp::new().listing().unwrap();
        let (files, warnings) = parse_listing(&bytes).unwrap();
        println!(
            "{} mods in the pack, {} ignored entries",
            files.len(),
            warnings.len()
        );
        for f in &files {
            println!("{:>10} {} {}", f.size.unwrap_or(0), f.sha, f.name);
        }
        for w in &warnings {
            println!("warning: {w}");
        }
        assert!(!files.is_empty());
    }
}
