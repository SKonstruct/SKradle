//! Applying and removing mods in a game folder.
//!
//! # What gets changed
//!
//! - `rsrc/`: a resource mod's files are written as loose files (`item/x.dat` becomes `rsrc/item/x.dat`).
//! - `code/config.jar`: a class mod's files are merged into the jar (a mod's file replaces the jar's entry).
//! - `code/projectx-config.jar`: locale changes from `mod.json` are merged into the named `.properties` files under
//!   `rsrc/i18n/` inside the jar; other lines of those files stay as they were.
//!
//! Mods apply in file-name order and a later mod wins. Disabled mods, mods that cannot be read, and class mods built
//! for another game version are left out and listed in the report. Files a mod may not replace are left out of that
//! mod and listed too ([`super::protected`]). A mod whose data turns out to be corrupt only when it is read is
//! excluded and the apply runs again without it.
//!
//! # What "pristine" means
//!
//! The updater's `digest2.txt` says what each file of the install must be ([`crate::digest`]; jars are compared by
//! the contents of their entries, not the bytes of the file). Both code jars are rebuilt from their pristine copy on
//! every apply, never from the previous output, so a mod that has been removed leaves nothing behind. The pristine
//! copy is, in order: the jar in `code/` if it matches its digest (a verified copy of it is then kept in
//! `.skradle/pristine/`); else that kept copy if it still matches; else a download from the game's own servers
//! for the installed version, accepted only if it matches the digest. With no verified original the apply stops
//! before changing anything. Loose files in `rsrc/` are restored from the game's resource bundles
//! (`rsrc/*.jar` listed in `digest2.txt`) for every path the previous apply wrote (it records them in
//! `.skradle/applied.json`), or deleted when no bundle has the path. With [`ApplyOptions::rebuild_resources`] every
//! loose file in `rsrc/` is rebuilt from the bundles instead, which also clears files left by other tools, at the
//! price of unpacking everything. The bundles are read, never changed, and are not re-verified on each apply.
//!
//! # Atomicity
//!
//! Everything that touches the game folder goes through one [`Txn`]: originals are kept until the last file is in
//! place, and any error rolls every file back; a crash is rolled back at the next start. Nothing is written until
//! all mods are scanned and the pristine jars are in hand.
//!
//! # How this meets the updater
//!
//! The updater compares every file in `digest2.txt` with the install and downloads any that differ. A modified
//! `config.jar` or `projectx-config.jar` therefore differs on purpose, and the updater would replace it on its next
//! run (loose files in `rsrc/` are not digest files, but the updater re-unpacks a bundle it refreshes over them).
//! So: run the updater first, apply right before starting the game, and apply again after every update. Do not
//! apply while the game is running. Two things the updater does with `.jarv` marker files next to jars were not
//! verified against the updater's code: whether a marker lets a modified jar pass unchecked. Do not rely on it.
//!
//! `code/config.jar` also carries per-server settings, which another module of the launcher rewrites. Because apply
//! rebuilds that jar from the original, such edits must be made after apply, never before.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt;
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::digest::{jar_digest_bytes, Digests};
use crate::getdown_txt::GetdownConfig;
use crate::jar::{safe_entry_path, write_atomic, Archive, Jar, JarError, Limits};

use super::list;
use super::metadata::{locale_jar_path, set_properties, ModInfo};
use super::model::{Kind, Mod, ModError};
use super::source::{HttpSource, NoSource, PristineSource};
use super::txn::{self, Txn, STATE_DIR};

/// Largest code jar read or kept.
const MAX_CODE_JAR_BYTES: u64 = 64 * 1024 * 1024;
const CODE_JARS: [&str; 2] = ["config.jar", "projectx-config.jar"];
const MANIFEST: &str = ".skradle/applied.json";
const MAX_SMALL_FILE: u64 = 1024 * 1024;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ApplyOptions {
    /// Rebuild every loose file of `rsrc/` from the game's bundles (slow; clears files other tools left behind).
    pub rebuild_resources: bool,
    /// File cleaning off: do not put modified originals back. The code jars are built on the jar that is in `code/`
    /// (the game's original only when that jar is missing or unreadable) and loose files that mods overwrote keep
    /// the version a previous apply wrote. Removing a mod then leaves its changes behind. `false` is the normal,
    /// clean behaviour.
    pub keep_modified_originals: bool,
    /// File purging off: loose files that earlier applies added (the game has no such file) stay in the game folder
    /// instead of being deleted. They stay recorded in `.skradle/applied.json`, so purging can remove them later.
    /// Only files in that record are ever purged. `false` purges.
    pub keep_added_files: bool,
}

/// A mod that was applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppliedMod {
    pub file_name: String,
    pub name: String,
    /// Mods inside it (1 unless it is a modpack).
    pub parts: usize,
}

/// A mod that was left out, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkippedMod {
    pub file_name: String,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JarOutcome {
    /// Already as it should be.
    Unchanged,
    /// Put back to the game's original.
    Restored,
    /// The original with mod changes merged in.
    Modified,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ApplyReport {
    pub applied: Vec<AppliedMod>,
    pub skipped: Vec<SkippedMod>,
    /// `file name: path` of every file a mod shipped that it may not replace.
    pub protected_left_out: Vec<String>,
    pub warnings: Vec<String>,
    /// Loose files written into `rsrc/` by mods.
    pub resource_files_written: usize,
    /// Loose files put back from the game's bundles.
    pub resource_files_restored: usize,
    /// Loose files removed because no bundle has them.
    pub resource_files_removed: usize,
    pub class_files_merged: usize,
    pub locale_changes_applied: usize,
    pub code_jars: Vec<(String, JarOutcome)>,
}

#[derive(Debug)]
pub enum ApplyError {
    /// The folder is not an installed game (no `rsrc/`, `code/`, `getdown.txt` or `digest2.txt`).
    GameNotInstalled(String),
    /// `digest2.txt` has no entry for a file that must be restored.
    NoDigest(String),
    /// A code jar is not the game's own and no verified original could be obtained.
    NoPristine {
        jar: String,
        reason: String,
    },
    /// A resource bundle could not be read.
    Bundle {
        bundle: String,
        error: JarError,
    },
    /// The mods folder or its state file could not be read.
    ModsFolder(io::Error),
    /// A mod could not be read while applying. (Applying retries without it; this is only returned from internals.)
    BadMod {
        file_name: String,
        error: ModError,
    },
    Jar(JarError),
    Io(io::Error),
    /// A write failed and the rollback failed too: the game folder may be half changed. The next apply or
    /// [`txn::recover`] tries the rollback again from the journal.
    RollbackFailed {
        cause: String,
        rollback: io::Error,
    },
}

impl fmt::Display for ApplyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ApplyError::GameNotInstalled(why) => write!(f, "the game is not installed here: {why}"),
            ApplyError::NoDigest(path) => write!(f, "digest2.txt has no entry for {path}"),
            ApplyError::NoPristine { jar, reason } => {
                write!(f, "no verified original of {jar}: {reason}")
            }
            ApplyError::Bundle { bundle, error } => write!(f, "cannot read {bundle}: {error}"),
            ApplyError::ModsFolder(e) => write!(f, "cannot read the mods folder: {e}"),
            ApplyError::BadMod { file_name, error } => write!(f, "{file_name}: {error}"),
            ApplyError::Jar(e) => write!(f, "{e}"),
            ApplyError::Io(e) => write!(f, "{e}"),
            ApplyError::RollbackFailed { cause, rollback } => write!(
                f,
                "{cause}; and undoing the changes failed too ({rollback}); the next apply will try again"
            ),
        }
    }
}

impl std::error::Error for ApplyError {}

impl From<io::Error> for ApplyError {
    fn from(e: io::Error) -> Self {
        ApplyError::Io(e)
    }
}

impl From<JarError> for ApplyError {
    fn from(e: JarError) -> Self {
        ApplyError::Jar(e)
    }
}

/// Failure inside the write phase: a mod's data, or a write.
enum Inner {
    Mod(ModError),
    Fail(ApplyError),
}

impl From<ModError> for Inner {
    fn from(e: ModError) -> Self {
        Inner::Mod(e)
    }
}

impl From<io::Error> for Inner {
    fn from(e: io::Error) -> Self {
        Inner::Fail(e.into())
    }
}

impl From<JarError> for Inner {
    fn from(e: JarError) -> Self {
        Inner::Fail(e.into())
    }
}

impl From<ApplyError> for Inner {
    fn from(e: ApplyError) -> Self {
        Inner::Fail(e)
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Manifest {
    #[serde(default)]
    version: u32,
    /// Paths under `rsrc/` that mods wrote.
    #[serde(default)]
    rsrc_files: Vec<String>,
}

/// Applies the enabled mods of `mods_dir` to the game in `game_root`, using the game's servers if an original code
/// jar has to be fetched. See the module docs for the pipeline.
pub fn apply(game_root: &Path, mods_dir: &Path) -> Result<ApplyReport, ApplyError> {
    apply_with(
        game_root,
        Some(mods_dir),
        ApplyOptions::default(),
        default_source(game_root).as_ref(),
    )
}

/// [`apply`] with the file cleaning and purging choices.
pub fn apply_with_options(
    game_root: &Path,
    mods_dir: &Path,
    options: ApplyOptions,
) -> Result<ApplyReport, ApplyError> {
    apply_with(
        game_root,
        Some(mods_dir),
        options,
        default_source(game_root).as_ref(),
    )
}

/// Removes every mod: both code jars go back to the game's original and every loose file a previous apply wrote is
/// restored or removed. Mods stay in their folder.
pub fn restore_pristine(game_root: &Path) -> Result<ApplyReport, ApplyError> {
    apply_with(
        game_root,
        None,
        ApplyOptions::default(),
        default_source(game_root).as_ref(),
    )
}

/// Where an original code jar comes from when none is on disk: the game's own servers, named by its `getdown.txt`.
pub(crate) fn default_source(game_root: &Path) -> Box<dyn PristineSource> {
    let text = read_small(&game_root.join("getdown.txt")).unwrap_or_default();
    match HttpSource::from_getdown(&GetdownConfig::parse(&String::from_utf8_lossy(&text))) {
        Some(source) => Box::new(source),
        None => Box::new(NoSource(
            "getdown.txt does not name a trusted game server".to_owned(),
        )),
    }
}

/// [`apply`] with every input explicit: `mods_dir` is `None` to remove all mods, and `source` supplies original jars
/// that are not on disk.
pub fn apply_with(
    game_root: &Path,
    mods_dir: Option<&Path>,
    options: ApplyOptions,
    source: &dyn PristineSource,
) -> Result<ApplyReport, ApplyError> {
    let mut excluded: BTreeMap<String, String> = BTreeMap::new();
    loop {
        match run(game_root, mods_dir, options, source, &excluded, None) {
            Err(ApplyError::BadMod { file_name, error }) if !excluded.contains_key(&file_name) => {
                excluded.insert(file_name, error.to_string());
            }
            Ok(mut report) => {
                for (file_name, reason) in excluded {
                    report.skipped.push(SkippedMod { file_name, reason });
                }
                return Ok(report);
            }
            Err(e) => return Err(e),
        }
    }
}

fn read_small(path: &Path) -> io::Result<Vec<u8>> {
    let meta = fs::symlink_metadata(path)?;
    if !meta.is_file() || meta.len() > MAX_SMALL_FILE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "not a regular file of plausible size",
        ));
    }
    fs::read(path)
}

/// The bytes of a regular file of at most `cap` bytes; `None` for anything else (missing, link, folder, too big).
fn read_regular(path: &Path, cap: u64) -> io::Result<Option<Vec<u8>>> {
    match fs::symlink_metadata(path) {
        Ok(m) if m.is_file() && m.len() <= cap => Ok(Some(fs::read(path)?)),
        Ok(_) => Ok(None),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

fn is_real_dir(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|m| m.is_dir())
}

fn check_game(root: &Path) -> Result<(), ApplyError> {
    for dir in ["rsrc", "code"] {
        if !is_real_dir(&root.join(dir)) {
            return Err(ApplyError::GameNotInstalled(format!(
                "{dir}/ is missing; start the game once to install it"
            )));
        }
    }
    for file in ["getdown.txt", "digest2.txt"] {
        if read_small(&root.join(file)).is_err() {
            return Err(ApplyError::GameNotInstalled(format!(
                "{file} is missing or unreadable"
            )));
        }
    }
    let state = root.join(STATE_DIR);
    if fs::symlink_metadata(&state).is_ok_and(|m| !m.is_dir()) {
        return Err(ApplyError::GameNotInstalled(format!(
            "{STATE_DIR} is not a folder"
        )));
    }
    Ok(())
}

fn lower(s: &str) -> String {
    s.to_lowercase()
}

/// A verified original of `code/<name>`. See the module docs for the order of preference.
fn load_pristine(
    root: &Path,
    name: &str,
    digests: &Digests,
    source: &dyn PristineSource,
) -> Result<Vec<u8>, ApplyError> {
    let rel = format!("code/{name}");
    let expected = digests
        .get(&rel)
        .ok_or_else(|| ApplyError::NoDigest(rel.clone()))?;
    let matches = |bytes: &[u8]| {
        jar_digest_bytes(bytes, &Limits::CODE_JAR).is_ok_and(|digest| digest == expected)
    };
    let backup = root.join(STATE_DIR).join("pristine").join(&rel);
    let keep = |bytes: &[u8]| -> io::Result<()> {
        if let Some(parent) = backup.parent() {
            fs::create_dir_all(parent)?;
        }
        write_atomic(&backup, bytes)
    };
    if let Some(bytes) = read_regular(&root.join(&rel), MAX_CODE_JAR_BYTES)? {
        if matches(&bytes) {
            if read_regular(&backup, MAX_CODE_JAR_BYTES)?.as_deref() != Some(&bytes[..]) {
                keep(&bytes)?;
            }
            return Ok(bytes);
        }
    }
    if let Some(bytes) = read_regular(&backup, MAX_CODE_JAR_BYTES)? {
        if matches(&bytes) {
            return Ok(bytes);
        }
    }
    let bytes =
        source
            .fetch(&rel, MAX_CODE_JAR_BYTES)
            .map_err(|reason| ApplyError::NoPristine {
                jar: rel.clone(),
                reason,
            })?;
    if !matches(&bytes) {
        return Err(ApplyError::NoPristine {
            jar: rel,
            reason: "the downloaded copy does not match digest2.txt".to_owned(),
        });
    }
    keep(&bytes)?;
    Ok(bytes)
}

/// The game's resource bundles, indexed by the paths they unpack to.
struct Bundles {
    archives: Vec<Archive<File>>,
    /// Path to (archive, entry); a later bundle wins.
    index: HashMap<String, (usize, usize)>,
    /// Lower-cased path to the spelling in `index`, to find a file that a case-folding file system treats as one.
    folded: HashMap<String, String>,
    /// Lower-cased folders the bundles contain (every parent of every entry, and folder entries).
    dirs: HashSet<String>,
}

impl Bundles {
    fn open(root: &Path, digests: &Digests) -> Result<Bundles, ApplyError> {
        let mut archives = Vec::new();
        let mut index = HashMap::new();
        let mut folded = HashMap::new();
        let mut dirs = HashSet::new();
        for rel in digests.paths() {
            let is_bundle = rel
                .strip_prefix("rsrc/")
                .is_some_and(|rest| rest.ends_with(".jar") && !rest.contains('/'));
            if !is_bundle || !fs::symlink_metadata(root.join(rel)).is_ok_and(|m| m.is_file()) {
                continue;
            }
            let archive =
                Archive::open_path(&root.join(rel), &Limits::BUNDLE).map_err(|error| {
                    ApplyError::Bundle {
                        bundle: rel.to_owned(),
                        error,
                    }
                })?;
            for e in archive.entries() {
                let lowered = lower(&e.path);
                let mut parts: Vec<&str> = lowered.split('/').collect();
                if !e.is_dir {
                    parts.pop();
                }
                while !parts.is_empty() {
                    dirs.insert(parts.join("/"));
                    parts.pop();
                }
                if !e.is_dir {
                    index.insert(e.path.clone(), (archives.len(), e.index));
                    folded.insert(lowered, e.path.clone());
                }
            }
            archives.push(archive);
        }
        Ok(Bundles {
            archives,
            index,
            folded,
            dirs,
        })
    }

    fn read(&mut self, path: &str) -> Result<Option<Vec<u8>>, ApplyError> {
        let Some(&(a, i)) = self.index.get(path) else {
            return Ok(None);
        };
        Ok(Some(self.archives[a].read(i)?))
    }

    /// The bundle's spelling of `path` when it differs from `path` only in case.
    fn case_variant(&self, path: &str) -> Option<String> {
        self.folded
            .get(&lower(path))
            .filter(|spelled| spelled.as_str() != path)
            .cloned()
    }

    fn has_dir(&self, path: &str) -> bool {
        self.dirs.contains(&lower(path))
    }

    fn paths(&self) -> Vec<String> {
        let mut paths: Vec<String> = self.index.keys().cloned().collect();
        paths.sort();
        paths
    }
}

struct LazyBundles<'a> {
    root: &'a Path,
    digests: &'a Digests,
    opened: Option<Bundles>,
}

impl LazyBundles<'_> {
    fn get(&mut self) -> Result<&mut Bundles, ApplyError> {
        if self.opened.is_none() {
            self.opened = Some(Bundles::open(self.root, self.digests)?);
        }
        Ok(self.opened.as_mut().expect("just opened"))
    }
}

fn file_equals(path: &Path, bytes: &[u8]) -> bool {
    match fs::symlink_metadata(path) {
        Ok(m) if m.is_file() && m.len() == bytes.len() as u64 => {
            fs::read(path).is_ok_and(|current| current == bytes)
        }
        _ => false,
    }
}

/// Adds a mod's locale changes to `locale`, keyed by the jar path of each bundle; a bundle name that is not a plain
/// path is skipped with a warning.
fn merge_locale(
    info: &ModInfo,
    file_name: &str,
    locale: &mut BTreeMap<String, BTreeMap<String, String>>,
    warnings: &mut Vec<String>,
) {
    for (bundle, changes) in &info.locale {
        match locale_jar_path(bundle) {
            Some(jar_path) => locale
                .entry(jar_path)
                .or_default()
                .extend(changes.iter().map(|(k, v)| (k.clone(), v.clone()))),
            None => warnings.push(format!(
                "{file_name}: locale bundle {bundle:?} is not a plain path, ignored"
            )),
        }
    }
}

fn read_manifest(root: &Path, warnings: &mut Vec<String>) -> BTreeSet<String> {
    let bytes = match read_small(&root.join(MANIFEST)) {
        Ok(b) => b,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return BTreeSet::new(),
        Err(e) => {
            warnings.push(format!("the record of earlier changes is unreadable ({e}); use a full rebuild to be sure rsrc/ is clean"));
            return BTreeSet::new();
        }
    };
    match serde_json::from_slice::<Manifest>(&bytes) {
        Ok(m) => m
            .rsrc_files
            .into_iter()
            .filter(|p| matches!(safe_entry_path(p), Ok((_, false))))
            .collect(),
        Err(e) => {
            warnings.push(format!("the record of earlier changes is damaged ({e}); use a full rebuild to be sure rsrc/ is clean"));
            BTreeSet::new()
        }
    }
}

/// Every file (and link) under `rsrc/`, as paths relative to `rsrc/`, except the bundles and markers at its top.
fn walk_rsrc(root: &Path, warnings: &mut Vec<String>) -> io::Result<Vec<String>> {
    fn walk(
        dir: &Path,
        rel: &str,
        out: &mut Vec<String>,
        warnings: &mut Vec<String>,
    ) -> io::Result<()> {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                warnings.push(format!(
                    "{}: name is not text, left alone",
                    entry.path().display()
                ));
                continue;
            };
            let path = if rel.is_empty() {
                name.clone()
            } else {
                format!("{rel}/{name}")
            };
            let meta = fs::symlink_metadata(entry.path())?;
            if meta.is_dir() {
                walk(&entry.path(), &path, out, warnings)?;
                continue;
            }
            let top = rel.is_empty();
            let lname = name.to_lowercase();
            if top
                && (lname.ends_with(".jar") || lname.ends_with(".zip") || lname.ends_with(".jarv"))
            {
                continue;
            }
            if safe_entry_path(&path).is_err() {
                warnings.push(format!(
                    "rsrc/{path}: name cannot be handled safely, left alone"
                ));
                continue;
            }
            out.push(path);
        }
        Ok(())
    }
    let mut out = Vec::new();
    walk(&root.join("rsrc"), "", &mut out, warnings)?;
    out.sort();
    Ok(out)
}

#[derive(Default)]
struct Counters {
    written: usize,
    restored: usize,
    removed: usize,
}

/// One apply, as a single transaction. `fault` makes the n-th write step fail (tests).
pub(crate) fn run(
    root: &Path,
    mods_dir: Option<&Path>,
    options: ApplyOptions,
    source: &dyn PristineSource,
    excluded: &BTreeMap<String, String>,
    fault: Option<usize>,
) -> Result<ApplyReport, ApplyError> {
    check_game(root)?;
    txn::recover(root)?;
    let version = {
        let text = read_small(&root.join("getdown.txt"))?;
        GetdownConfig::parse(&String::from_utf8_lossy(&text)).version
    };
    let digests = Digests::read(&root.join("digest2.txt"))?;
    let mut report = ApplyReport::default();

    // 1. Scan the mods. Nothing is written yet.
    struct Chosen {
        path: PathBuf,
        m: Mod,
        parts: Vec<usize>,
    }
    let mut chosen: Vec<Chosen> = Vec::new();
    if let Some(dir) = mods_dir {
        for installed in list::list(dir).map_err(ApplyError::ModsFolder)? {
            let file_name = installed.file_name;
            if excluded.contains_key(&file_name) {
                continue;
            }
            if !installed.enabled {
                report.skipped.push(SkippedMod {
                    file_name,
                    reason: "disabled".to_owned(),
                });
                continue;
            }
            let m = match installed.parsed {
                Ok(m) => m,
                Err(e) => {
                    report.skipped.push(SkippedMod {
                        file_name,
                        reason: e.to_string(),
                    });
                    continue;
                }
            };
            for w in &m.warnings {
                report.warnings.push(format!("{file_name}: {w}"));
            }
            let mut parts = Vec::new();
            for (i, part) in m.parts.iter().enumerate() {
                for w in &part.warnings {
                    report
                        .warnings
                        .push(format!("{file_name}: {}: {w}", part.name));
                }
                if part.compatible_with(version.as_deref()) {
                    parts.push(i);
                } else {
                    report.skipped.push(SkippedMod {
                        file_name: if m.parts.len() > 1 {
                            format!("{file_name}: {}", part.name)
                        } else {
                            file_name.clone()
                        },
                        reason: format!(
                            "class mod built for game version {}, installed is {}",
                            part.info.px_version.as_deref().unwrap_or("?"),
                            version.as_deref().unwrap_or("?")
                        ),
                    });
                }
            }
            if !parts.is_empty() {
                report.applied.push(AppliedMod {
                    file_name: file_name.clone(),
                    name: m.display_name().to_owned(),
                    parts: parts.len(),
                });
            }
            chosen.push(Chosen {
                path: dir.join(&file_name),
                m,
                parts,
            });
        }
    }

    // 2. What the mods want: resource winners, class files, locale changes.
    let mut winners: BTreeMap<String, (usize, usize, String)> = BTreeMap::new();
    let mut class_files: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let mut locale: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    for (ci, c) in chosen.iter().enumerate() {
        for &pi in &c.parts {
            let part = &c.m.parts[pi];
            for f in part.files.iter().filter(|f| f.protected) {
                report
                    .protected_left_out
                    .push(format!("{}: {}", c.m.file_name, f.path));
            }
            if part.kind == Kind::Resource {
                for f in part.applicable_files() {
                    winners.insert(lower(&f.path), (ci, pi, f.path.clone()));
                }
            } else {
                let mut local = BTreeMap::new();
                c.m.each_file::<Inner>(&c.path, part, |f, bytes| {
                    local.insert(f.path.clone(), bytes);
                    Ok(())
                })
                .map_err(|e| bad_mod(&c.m.file_name, e))?;
                class_files.extend(local);
            }
            merge_locale(
                &part.info,
                &c.m.file_name,
                &mut locale,
                &mut report.warnings,
            );
        }
        // A pack's own locale changes come after its mods', once.
        if let Some(info) = c.m.pack_info().filter(|_| !c.parts.is_empty()) {
            merge_locale(info, &c.m.file_name, &mut locale, &mut report.warnings);
        }
    }

    // 3. The code jars as they should end up. Originals are fetched here if needed, before anything is written.
    let mut desired_jars: Vec<(String, Vec<u8>, bool)> = Vec::new();
    for name in CODE_JARS {
        let pristine = if options.keep_modified_originals {
            // Build on what is there; ask for the original only when there is nothing usable.
            match read_regular(&root.join("code").join(name), MAX_CODE_JAR_BYTES)? {
                Some(bytes) if Jar::from_bytes(&bytes, &Limits::CODE_JAR).is_ok() => bytes,
                _ => load_pristine(root, name, &digests, source)?,
            }
        } else {
            load_pristine(root, name, &digests, source)?
        };
        let wants_class = name == "config.jar" && !class_files.is_empty();
        let wants_locale = name == "projectx-config.jar" && !locale.is_empty();
        if !wants_class && !wants_locale {
            desired_jars.push((name.to_owned(), pristine, false));
            continue;
        }
        let mut jar = Jar::from_bytes(&pristine, &Limits::CODE_JAR)?;
        if wants_class {
            for (path, bytes) in &class_files {
                match jar.set(path, bytes.clone()) {
                    Ok(()) => report.class_files_merged += 1,
                    // A class file that would make a folder of a file (or the reverse) would leave a jar the game
                    // cannot open: leave that one out.
                    Err(JarError::FileFolderConflict { name }) => report.warnings.push(format!(
                        "{path}: conflicts with {name} in config.jar, ignored"
                    )),
                    Err(e) => return Err(e.into()),
                }
            }
        }
        if wants_locale {
            for (jar_path, changes) in &locale {
                match jar.get(jar_path) {
                    Some(existing) => {
                        let updated = set_properties(existing, changes);
                        jar.set(jar_path, updated)?;
                        report.locale_changes_applied += changes.len();
                    }
                    None => report.warnings.push(format!(
                        "locale bundle {jar_path} does not exist in the game, ignored"
                    )),
                }
            }
        }
        desired_jars.push((name.to_owned(), jar.to_bytes()?, true));
    }

    // 4. Write everything in one transaction.
    let previous = read_manifest(root, &mut report.warnings);
    let mut existing_files = Vec::new();
    if options.rebuild_resources {
        existing_files = walk_rsrc(root, &mut report.warnings)?;
    }
    let mut txn = Txn::begin_with_fault(root, fault)?;
    let written = write_phase(
        root,
        &mut txn,
        &digests,
        &chosen
            .iter()
            .map(|c| (c.path.as_path(), &c.m))
            .collect::<Vec<_>>(),
        &winners,
        &desired_jars,
        &previous,
        &existing_files,
        options,
        &mut report,
    );
    match written {
        Ok(counters) => {
            txn.commit()?;
            report.resource_files_written = counters.written;
            report.resource_files_restored = counters.restored;
            report.resource_files_removed = counters.removed;
            Ok(report)
        }
        Err(error) => {
            if let Err(rollback) = txn.rollback() {
                return Err(ApplyError::RollbackFailed {
                    cause: error.to_string(),
                    rollback,
                });
            }
            Err(error)
        }
    }
}

fn bad_mod(file_name: &str, inner: Inner) -> ApplyError {
    match inner {
        Inner::Mod(error) => ApplyError::BadMod {
            file_name: file_name.to_owned(),
            error,
        },
        Inner::Fail(e) => e,
    }
}

#[allow(clippy::too_many_arguments)]
fn write_phase(
    root: &Path,
    txn: &mut Txn,
    digests: &Digests,
    chosen: &[(&Path, &Mod)],
    winners: &BTreeMap<String, (usize, usize, String)>,
    desired_jars: &[(String, Vec<u8>, bool)],
    previous: &BTreeSet<String>,
    existing_files: &[String],
    options: ApplyOptions,
    report: &mut ApplyReport,
) -> Result<Counters, ApplyError> {
    let mut counters = Counters::default();

    // Code jars.
    for (name, desired, modified) in desired_jars {
        let rel = format!("code/{name}");
        let current = read_regular(&root.join(&rel), MAX_CODE_JAR_BYTES)?;
        let outcome = if current.as_deref() == Some(&desired[..]) {
            JarOutcome::Unchanged
        } else {
            txn.put(&rel, desired)?;
            if *modified {
                JarOutcome::Modified
            } else {
                JarOutcome::Restored
            }
        };
        report.code_jars.push((name.clone(), outcome));
    }

    // Loose files: first put back what earlier applies changed and mods no longer change, then write the mods'.
    let winner_paths: BTreeSet<&str> = winners.values().map(|(_, _, p)| p.as_str()).collect();
    let winner_lower: BTreeSet<&str> = winners.keys().map(String::as_str).collect();
    let mut bundles = LazyBundles {
        root,
        digests,
        opened: None,
    };
    let mut stale: BTreeSet<String> = previous
        .iter()
        .filter(|p| !winner_paths.contains(p.as_str()))
        .cloned()
        .collect();
    if options.rebuild_resources {
        stale.extend(
            existing_files
                .iter()
                .filter(|p| !winner_lower.contains(lower(p).as_str()))
                .cloned(),
        );
        stale.extend(
            bundles
                .get()?
                .paths()
                .into_iter()
                .filter(|p| !winner_lower.contains(lower(p).as_str())),
        );
    }
    // Paths left alone on purpose stay in the record.
    let mut retained: BTreeSet<String> = BTreeSet::new();
    for path in stale {
        let rel = format!("rsrc/{path}");
        match bundles.get()?.read(&path)? {
            Some(bytes) => {
                if options.keep_modified_originals {
                    retained.insert(path);
                } else if !file_equals(&root.join(&rel), &bytes) {
                    txn.put(&rel, &bytes)?;
                    counters.restored += 1;
                }
            }
            None if options.keep_added_files => {
                if fs::symlink_metadata(root.join(&rel)).is_ok() {
                    retained.insert(path);
                }
            }
            None => {
                if fs::symlink_metadata(root.join(&rel)).is_ok() {
                    txn.delete(&rel)?;
                    counters.removed += 1;
                    // Folders a mod created and that are now empty go too; the game's own folders stay.
                    let mut parts: Vec<&str> = path.split('/').collect();
                    parts.pop();
                    while !parts.is_empty() {
                        let dir = parts.join("/");
                        if bundles.get()?.has_dir(&dir)
                            || !txn.remove_dir_if_empty(&format!("rsrc/{dir}"))?
                        {
                            break;
                        }
                        parts.pop();
                    }
                }
                // A file system that folds case may have held the game's own file under this name: put it back.
                let variant = bundles.get()?.case_variant(&path);
                if let Some(original) = variant.filter(|_| !options.keep_modified_originals) {
                    let bytes = bundles.get()?.read(&original)?.unwrap_or_default();
                    let rel = format!("rsrc/{original}");
                    if !file_equals(&root.join(&rel), &bytes) {
                        txn.put(&rel, &bytes)?;
                        counters.restored += 1;
                    }
                }
            }
        }
    }
    for (ci, (mod_path, m)) in chosen.iter().enumerate() {
        for (pi, part) in m.parts.iter().enumerate() {
            let wanted = |f: &super::model::PartFile| {
                winners
                    .get(&lower(&f.path))
                    .is_some_and(|(c, p, path)| *c == ci && *p == pi && *path == f.path)
            };
            if !part.applicable_files().any(wanted) {
                continue;
            }
            let mut written = 0;
            m.each_file_where::<Inner>(mod_path, part, wanted, |f, bytes| {
                let rel = format!("rsrc/{}", f.path);
                if !file_equals(&root.join(&rel), &bytes) {
                    txn.put(&rel, &bytes)?;
                    written += 1;
                }
                Ok(())
            })
            .map_err(|e| bad_mod(&m.file_name, e))?;
            counters.written += written;
        }
    }

    // The record of what was written, in the same transaction.
    let mut rsrc_files: Vec<String> = winners.values().map(|(_, _, p)| p.clone()).collect();
    rsrc_files.extend(retained);
    rsrc_files.sort();
    rsrc_files.dedup();
    if rsrc_files.is_empty() {
        txn.delete(MANIFEST)?;
        return Ok(counters);
    }
    let mut json = serde_json::to_vec_pretty(&Manifest {
        version: 1,
        rsrc_files,
    })
    .map_err(|e| io::Error::other(e.to_string()))?;
    json.push(b'\n');
    if !file_equals(&root.join(MANIFEST), &json) {
        txn.put(MANIFEST, &json)?;
    }
    Ok(counters)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jar::testutil::{simple_zip, zip_with};
    use crate::mods::txn::testutil::snapshot;
    use crate::net::sha256_hex;
    use std::cell::Cell;

    /// Hands out originals from memory and counts how often it was asked.
    #[derive(Default)]
    struct FakeSource {
        files: HashMap<String, Vec<u8>>,
        calls: Cell<usize>,
    }

    impl PristineSource for FakeSource {
        fn fetch(&self, rel: &str, _max: u64) -> Result<Vec<u8>, String> {
            self.calls.set(self.calls.get() + 1);
            self.files
                .get(rel)
                .cloned()
                .ok_or_else(|| "offline".to_owned())
        }
    }

    struct Fixture {
        _tmp: tempfile::TempDir,
        root: PathBuf,
        mods: PathBuf,
        config_jar: Vec<u8>,
        px_jar: Vec<u8>,
    }

    impl Fixture {
        fn source(&self) -> FakeSource {
            let mut files = HashMap::new();
            files.insert("code/config.jar".to_owned(), self.config_jar.clone());
            files.insert("code/projectx-config.jar".to_owned(), self.px_jar.clone());
            FakeSource {
                files,
                calls: Cell::new(0),
            }
        }

        fn add_mod(&self, name: &str, bytes: &[u8]) {
            fs::write(self.mods.join(name), bytes).unwrap();
        }

        fn apply(&self) -> Result<ApplyReport, ApplyError> {
            let source = FakeSource::default();
            apply_with(
                &self.root,
                Some(&self.mods),
                ApplyOptions::default(),
                &source,
            )
        }

        /// The game folder without the launcher's own pristine copies.
        fn snap(&self) -> BTreeMap<String, Vec<u8>> {
            let mut s = snapshot(&self.root);
            s.retain(|k, _| !k.starts_with(".skradle/pristine") && k != ".skradle/");
            s
        }

        fn read(&self, rel: &str) -> Vec<u8> {
            fs::read(self.root.join(rel)).unwrap()
        }
    }

    fn put(root: &Path, rel: &str, bytes: &[u8]) {
        let path = root.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }

    fn fixture() -> Fixture {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("game");
        let mods = tmp.path().join("mods");
        fs::create_dir_all(&mods).unwrap();
        let config_jar = simple_zip(&[
            ("META-INF/MANIFEST.MF", b"Manifest-Version: 1.0\n"),
            ("build.properties", b"version=1\n"),
            ("deployment.properties", b"server=official\n"),
        ]);
        let px_jar = simple_zip(&[
            ("META-INF/MANIFEST.MF", b"Manifest-Version: 1.0\n"),
            (
                "rsrc/i18n/messages.properties",
                b"# messages\nhello=Hello\nbye=Bye\n",
            ),
            ("rsrc/i18n/other.properties", b"x=1\n"),
        ]);
        let intro = simple_zip(&[
            ("META-INF/MANIFEST.MF", b"m"),
            ("config/item.dat", b"item-original"),
            ("texture/a.png", b"png-a"),
            ("item/x/model.dat", b"model-x"),
        ]);
        let rest = simple_zip(&[("sound/s.ogg", b"ogg"), ("texture/b.png", b"png-b")]);
        put(&root, "code/config.jar", &config_jar);
        put(&root, "code/projectx-config.jar", &px_jar);
        put(&root, "rsrc/intro-bundle.jar", &intro);
        put(&root, "rsrc/intro-bundle.jarv", b"");
        put(&root, "rsrc/full-rest-bundle.jar", &rest);
        for (rel, bytes) in [
            ("rsrc/META-INF/MANIFEST.MF", &b"m"[..]),
            ("rsrc/config/item.dat", b"item-original"),
            ("rsrc/texture/a.png", b"png-a"),
            ("rsrc/item/x/model.dat", b"model-x"),
            ("rsrc/sound/s.ogg", b"ogg"),
            ("rsrc/texture/b.png", b"png-b"),
        ] {
            put(&root, rel, bytes);
        }
        let getdown = b"version = 20261110044614\nappbase = http://gamemedia.spiralknights.com/spiral/%VERSION%\n";
        put(&root, "getdown.txt", getdown);
        let jar_digest = |b: &[u8]| jar_digest_bytes(b, &Limits::BUNDLE).unwrap();
        let digests = format!(
            "getdown.txt = {}\ncode/config.jar = {}\ncode/projectx-config.jar = {}\nrsrc/intro-bundle.jar = {}\nrsrc/full-rest-bundle.jar = {}\n",
            sha256_hex(getdown),
            jar_digest(&config_jar),
            jar_digest(&px_jar),
            jar_digest(&intro),
            jar_digest(&rest),
        );
        put(&root, "digest2.txt", digests.as_bytes());
        Fixture {
            _tmp: tmp,
            root,
            mods,
            config_jar,
            px_jar,
        }
    }

    fn resource_mod(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut all: Vec<(&str, &[u8])> = vec![("mod.json", br#"{"mod": {"name": "res",}}"#)];
        all.extend_from_slice(files);
        simple_zip(&all)
    }

    fn class_mod(px: &str, files: &[(&str, &[u8])]) -> Vec<u8> {
        let json = format!(r#"{{"mod": {{"name": "cls", "type": "class", "pxVersion": "{px}"}}}}"#);
        let mut all: Vec<(&str, &[u8])> = vec![("mod.json", json.as_bytes())];
        all.extend_from_slice(files);
        simple_zip(&all)
    }

    #[test]
    fn a_class_file_that_clashes_with_the_jar_layout_is_left_out_not_written() {
        let fx = fixture();
        fx.add_mod(
            "1_class.zip",
            &class_mod(
                "20261110044614",
                &[
                    // `deployment.properties` is a file in the jar: it cannot also be a folder.
                    ("deployment.properties/Y.class", b"clash"),
                    ("com/x/Ok.class", b"ok"),
                ],
            ),
        );
        let report = fx.apply().unwrap();
        assert_eq!(report.class_files_merged, 1, "{report:#?}");
        assert!(
            report
                .warnings
                .iter()
                .any(|w| w.contains("deployment.properties")),
            "{report:#?}"
        );
        let cfg = Jar::from_bytes(&fx.read("code/config.jar"), &Limits::CODE_JAR).unwrap();
        assert_eq!(cfg.get("com/x/Ok.class"), Some(&b"ok"[..]));
        assert!(cfg.get("deployment.properties").is_some());
    }

    /// A game with one mod applied that overwrote a bundled file, added a new one and changed a class; then the mod
    /// is removed (restore) with the given options. Returns the fixture.
    fn mod_applied_then_removed(options: ApplyOptions) -> Fixture {
        let fx = fixture();
        fx.add_mod(
            "1_res.zip",
            &resource_mod(&[("texture/a.png", b"MOD-A"), ("new/n.png", b"new")]),
        );
        fx.add_mod(
            "2_class.zip",
            &class_mod("20261110044614", &[("com/x/Y.class", b"class!")]),
        );
        fx.apply().unwrap();
        fs::remove_file(fx.mods.join("1_res.zip")).unwrap();
        fs::remove_file(fx.mods.join("2_class.zip")).unwrap();
        let source = FakeSource::default();
        apply_with(&fx.root, Some(&fx.mods), options, &source).unwrap();
        fx
    }

    #[test]
    fn cleaning_and_purging_on_remove_every_trace_of_a_removed_mod() {
        let fx = mod_applied_then_removed(ApplyOptions::default());
        assert_eq!(fx.read("rsrc/texture/a.png"), b"png-a");
        assert!(!fx.root.join("rsrc/new/n.png").exists());
        assert_eq!(fx.read("code/config.jar"), fx.config_jar);
    }

    #[test]
    fn with_cleaning_off_a_removed_mod_keeps_its_overwrites() {
        let fx = mod_applied_then_removed(ApplyOptions {
            keep_modified_originals: true,
            ..ApplyOptions::default()
        });
        assert_eq!(fx.read("rsrc/texture/a.png"), b"MOD-A", "not restored");
        assert!(
            !fx.root.join("rsrc/new/n.png").exists(),
            "purging is a separate switch and removes the recorded added file"
        );
    }

    #[test]
    fn with_purging_off_added_files_stay_recorded_and_a_later_purge_removes_only_those() {
        let fx = mod_applied_then_removed(ApplyOptions {
            keep_added_files: true,
            ..ApplyOptions::default()
        });
        assert_eq!(
            fx.read("rsrc/texture/a.png"),
            b"png-a",
            "cleaning restored it"
        );
        assert_eq!(fx.read("rsrc/new/n.png"), b"new", "kept");
        assert!(fx.root.join(".skradle/applied.json").exists());
        // A file the launcher never recorded is never touched, purge or not.
        put(&fx.root, "rsrc/someone-elses.txt", b"theirs");
        let source = FakeSource::default();
        apply_with(&fx.root, Some(&fx.mods), ApplyOptions::default(), &source).unwrap();
        assert!(!fx.root.join("rsrc/new/n.png").exists(), "purged now");
        assert_eq!(fx.read("rsrc/someone-elses.txt"), b"theirs");
        assert!(!fx.root.join(".skradle/applied.json").exists());
    }

    #[test]
    fn with_cleaning_off_class_mods_build_on_the_current_jar_and_need_no_original() {
        let fx = fixture();
        fx.add_mod(
            "1_class.zip",
            &class_mod("20261110044614", &[("com/x/Y.class", b"class!")]),
        );
        fx.apply().unwrap();
        // The jar in code/ is now modified; with no source an ordinary apply of another mod still works only
        // because the kept pristine copy exists. Remove it: cleaning off must still succeed.
        fs::remove_dir_all(fx.root.join(".skradle/pristine")).unwrap();
        fx.add_mod(
            "2_class.zip",
            &class_mod("20261110044614", &[("com/x/Z.class", b"z")]),
        );
        let source = FakeSource::default();
        let options = ApplyOptions {
            keep_modified_originals: true,
            ..ApplyOptions::default()
        };
        apply_with(&fx.root, Some(&fx.mods), options, &source).unwrap();
        let cfg = Jar::from_bytes(&fx.read("code/config.jar"), &Limits::CODE_JAR).unwrap();
        assert_eq!(cfg.get("com/x/Y.class"), Some(&b"class!"[..]));
        assert_eq!(cfg.get("com/x/Z.class"), Some(&b"z"[..]));
        // Cleaning on cannot rebuild without an original: it refuses rather than guess.
        assert!(apply_with(&fx.root, Some(&fx.mods), ApplyOptions::default(), &source).is_err());
    }

    #[test]
    fn apply_then_restore_round_trips_byte_for_byte() {
        let fx = fixture();
        let before = fx.snap();
        fx.add_mod(
            "1_res.zip",
            &resource_mod(&[("texture/a.png", b"MOD-A"), ("new/n.png", b"new")]),
        );
        fx.add_mod(
            "2_class.zip",
            &class_mod(
                "20261110044614",
                &[
                    ("com/x/Y.class", b"class!"),
                    ("build.properties", b"version=mod\n"),
                ],
            ),
        );
        let locale = simple_zip(&[(
            "mod.json",
            br#"{"mod": {"name": "loc", "locale": {"messages.properties": {"hello": "Howdy", "new.key": "caf\u00e9"}}}}"#,
        )]);
        fx.add_mod("3_locale.zip", &locale);

        let report = fx.apply().unwrap();
        assert_eq!(report.applied.len(), 3, "{report:#?}");
        assert_eq!(fx.read("rsrc/texture/a.png"), b"MOD-A");
        assert_eq!(fx.read("rsrc/new/n.png"), b"new");
        assert_eq!(
            fx.read("rsrc/texture/b.png"),
            b"png-b",
            "untouched files stay"
        );
        assert!(!fx.root.join("rsrc/mod.json").exists());
        let cfg = Jar::from_bytes(&fx.read("code/config.jar"), &Limits::CODE_JAR).unwrap();
        assert_eq!(cfg.get("com/x/Y.class"), Some(&b"class!"[..]));
        assert_eq!(cfg.get("build.properties"), Some(&b"version=mod\n"[..]));
        assert_eq!(
            cfg.get("deployment.properties"),
            Some(&b"server=official\n"[..])
        );
        let px = Jar::from_bytes(&fx.read("code/projectx-config.jar"), &Limits::CODE_JAR).unwrap();
        assert_eq!(
            px.get("rsrc/i18n/messages.properties"),
            Some(&b"# messages\nhello=Howdy\nbye=Bye\nnew.key=caf\\u00e9\n"[..])
        );
        assert_eq!(px.get("rsrc/i18n/other.properties"), Some(&b"x=1\n"[..]));
        assert_eq!(report.class_files_merged, 2);
        assert_eq!(report.locale_changes_applied, 2);
        assert_eq!(report.resource_files_written, 2);
        assert!(!txn_leftovers(&fx.root));

        // Undo everything: the game folder is byte for byte what it was.
        let restore_source = FakeSource::default();
        let r = apply_with(&fx.root, None, ApplyOptions::default(), &restore_source).unwrap();
        assert_eq!(
            r.resource_files_restored, 1,
            "a.png comes back from its bundle"
        );
        assert_eq!(r.resource_files_removed, 1, "n.png is not in any bundle");
        let after = fx.snap();
        for key in before.keys().chain(after.keys()) {
            assert_eq!(before.get(key), after.get(key), "{key} differs");
        }
        assert_eq!(
            restore_source.calls.get(),
            0,
            "no network: the verified copy is on disk"
        );
    }

    fn txn_leftovers(root: &Path) -> bool {
        root.join(".skradle/txn").exists() || root.join("code/config.jar.skradle-tmp").exists()
    }

    #[test]
    fn applying_twice_changes_nothing_the_second_time() {
        let fx = fixture();
        fx.add_mod("a.zip", &resource_mod(&[("texture/a.png", b"MOD")]));
        fx.add_mod(
            "b.zip",
            &class_mod("20261110044614", &[("c/D.class", b"1")]),
        );
        fx.apply().unwrap();
        let once = fx.snap();
        let report = fx.apply().unwrap();
        assert_eq!(fx.snap(), once);
        assert_eq!(report.resource_files_written, 0);
        assert!(
            report
                .code_jars
                .iter()
                .all(|(_, o)| *o == JarOutcome::Unchanged),
            "{:?}",
            report.code_jars
        );
    }

    #[test]
    fn a_later_mod_wins_and_a_removed_mod_leaves_nothing_behind() {
        let fx = fixture();
        fx.add_mod(
            "a.zip",
            &resource_mod(&[("texture/a.png", b"FROM-A"), ("only/a.png", b"a")]),
        );
        fx.add_mod("b.zip", &resource_mod(&[("TEXTURE/A.PNG", b"FROM-B")]));
        fx.apply().unwrap();
        // Compare through the case-insensitive spelling the later mod used when the file system folds case.
        let on_disk: Vec<u8> = ["rsrc/TEXTURE/A.PNG", "rsrc/texture/a.png"]
            .iter()
            .filter_map(|p| fs::read(fx.root.join(p)).ok())
            .next()
            .unwrap();
        assert_eq!(on_disk, b"FROM-B");
        fs::remove_file(fx.mods.join("b.zip")).unwrap();
        fs::remove_file(fx.mods.join("a.zip")).unwrap();
        let before_mods = fx.snap();
        let report = fx.apply().unwrap();
        assert!(report.applied.is_empty());
        assert_eq!(fx.read("rsrc/texture/a.png"), b"png-a");
        assert!(!fx.root.join("rsrc/only/a.png").exists());
        assert_ne!(before_mods, fx.snap());
    }

    #[test]
    fn protected_files_are_left_out_but_the_rest_of_the_mod_applies() {
        let fx = fixture();
        fx.add_mod(
            "k.zip",
            &resource_mod(&[("config/Item.dat", b"EVIL"), ("texture/a.png", b"ok")]),
        );
        let report = fx.apply().unwrap();
        assert_eq!(fx.read("rsrc/config/item.dat"), b"item-original");
        assert_eq!(fx.read("rsrc/texture/a.png"), b"ok");
        assert_eq!(report.protected_left_out, ["k.zip: config/Item.dat"]);
    }

    #[test]
    fn disabled_broken_and_incompatible_mods_are_skipped_with_reasons() {
        let fx = fixture();
        fx.add_mod("1_off.zip", &resource_mod(&[("texture/a.png", b"OFF")]));
        list::set_enabled(&fx.mods, "1_off.zip", false).unwrap();
        fx.add_mod("2_broken.zip", b"not a zip");
        fx.add_mod(
            "3_old.zip",
            &class_mod("19990101000000", &[("c/Old.class", b"1")]),
        );
        fx.add_mod("4_slip.zip", &simple_zip(&[("../../evil.txt", b"x")]));
        fx.add_mod("5_ok.zip", &resource_mod(&[("texture/b.png", b"ok")]));
        let report = fx.apply().unwrap();
        let reasons: BTreeMap<_, _> = report
            .skipped
            .iter()
            .map(|s| (s.file_name.as_str(), s.reason.as_str()))
            .collect();
        assert_eq!(reasons.len(), 4, "{reasons:?}");
        assert_eq!(reasons["1_off.zip"], "disabled");
        assert!(reasons["3_old.zip"].contains("19990101000000"));
        assert!(
            reasons["4_slip.zip"].contains("unsafe"),
            "{}",
            reasons["4_slip.zip"]
        );
        assert_eq!(fx.read("rsrc/texture/a.png"), b"png-a");
        assert_eq!(fx.read("rsrc/texture/b.png"), b"ok");
        assert!(!fx.root.join("evil.txt").exists());
        assert_eq!(
            fx.read("code/config.jar"),
            fx.config_jar,
            "no class mod, so the original jar"
        );
    }

    #[test]
    fn a_modpack_applies_its_parts_and_its_own_locale() {
        let fx = fixture();
        let inner_res = resource_mod(&[("texture/a.png", b"PACK")]);
        let inner_cls = class_mod("20261110044614", &[("p/Q.class", b"q")]);
        let pack = simple_zip(&[
            (
                "mod.json",
                br#"{"mod": {"name": "pack", "locale": {"other.properties": {"x": "2"}}}}"#,
            ),
            ("r.zip", &inner_res),
            ("c.zip", &inner_cls),
        ]);
        fx.add_mod("pack.modpack", &pack);
        let report = fx.apply().unwrap();
        assert_eq!(report.applied[0].parts, 2);
        assert_eq!(fx.read("rsrc/texture/a.png"), b"PACK");
        let cfg = Jar::from_bytes(&fx.read("code/config.jar"), &Limits::CODE_JAR).unwrap();
        assert!(cfg.contains("p/Q.class"));
        let px = Jar::from_bytes(&fx.read("code/projectx-config.jar"), &Limits::CODE_JAR).unwrap();
        assert_eq!(px.get("rsrc/i18n/other.properties"), Some(&b"x=2\n"[..]));
    }

    #[test]
    fn a_packs_own_locale_is_applied_once_not_once_per_part() {
        let fx = fixture();
        let inner_res = resource_mod(&[("texture/a.png", b"PACK")]);
        let inner_cls = class_mod("20261110044614", &[("p/Q.class", b"q")]);
        let pack = simple_zip(&[
            (
                "mod.json",
                br#"{"mod": {"name": "pack", "locale": {"../../../steam.creds": {"a": "b"}, "other.properties": {"x": "2"}}}}"#,
            ),
            ("r.zip", &inner_res),
            ("c.zip", &inner_cls),
        ]);
        fx.add_mod("pack.modpack", &pack);
        let report = fx.apply().unwrap();
        assert_eq!(report.applied[0].parts, 2);
        let plain = report
            .warnings
            .iter()
            .filter(|w| w.contains("not a plain path"))
            .count();
        assert_eq!(plain, 1, "{:?}", report.warnings);
        let px = Jar::from_bytes(&fx.read("code/projectx-config.jar"), &Limits::CODE_JAR).unwrap();
        assert_eq!(px.get("rsrc/i18n/other.properties"), Some(&b"x=2\n"[..]));
    }

    #[test]
    fn unusable_locale_bundles_only_warn() {
        let fx = fixture();
        let m = simple_zip(&[(
            "mod.json",
            br#"{"locale": {"../../../steam.creds": {"a": "b"}, "missing.properties": {"a": "b"}}}"#,
        )]);
        fx.add_mod("l.zip", &m);
        let report = fx.apply().unwrap();
        assert!(
            report
                .warnings
                .iter()
                .any(|w| w.contains("not a plain path")),
            "{:?}",
            report.warnings
        );
        assert!(
            report.warnings.iter().any(|w| w.contains("does not exist")),
            "{:?}",
            report.warnings
        );
        assert_eq!(fx.read("code/projectx-config.jar"), fx.px_jar);
    }

    #[test]
    fn a_mod_corrupt_only_when_read_is_excluded_and_the_rest_applies() {
        let fx = fixture();
        let payload: Vec<u8> = {
            let mut x = 0x2545_f491u32;
            (0..4000)
                .map(|_| {
                    x ^= x << 13;
                    x ^= x >> 17;
                    x ^= x << 5;
                    x as u8
                })
                .collect()
        };
        let mut bad = resource_mod(&[("texture/a.png", &payload)]);
        let data_at = bad
            .windows(4)
            .position(|w| w == [0x50, 0x4b, 0x03, 0x04])
            .unwrap();
        // Damage the compressed data of the last entry (the payload) in the middle.
        let last = bad
            .windows(4)
            .rposition(|w| w == [0x50, 0x4b, 0x03, 0x04])
            .unwrap();
        let _ = data_at;
        let mid = last + 30 + "texture/a.png".len() + 1500;
        for b in &mut bad[mid..mid + 40] {
            *b ^= 0xA5;
        }
        fx.add_mod("1_bad.zip", &bad);
        fx.add_mod("2_good.zip", &resource_mod(&[("texture/b.png", b"good")]));
        let before = fx.snap();
        let report = fx.apply().unwrap();
        assert_eq!(fx.read("rsrc/texture/b.png"), b"good");
        assert_eq!(
            fx.read("rsrc/texture/a.png"),
            b"png-a",
            "nothing of the bad mod was written"
        );
        assert!(
            report.skipped.iter().any(|s| s.file_name == "1_bad.zip"),
            "{report:#?}"
        );
        assert!(report.applied.iter().all(|a| a.file_name != "1_bad.zip"));
        assert_ne!(before, fx.snap());
    }

    #[test]
    fn a_huge_declared_size_is_rejected_before_anything_is_written() {
        let fx = fixture();
        let mut bomb = resource_mod(&[("texture/a.png", &[0u8; 64])]);
        for sig in [[0x50u8, 0x4b, 0x03, 0x04], [0x50, 0x4b, 0x01, 0x02]] {
            let offset = if sig[2] == 3 { 22 } else { 24 };
            let mut from = 0;
            while let Some(at) = bomb[from..].windows(4).position(|w| w == sig) {
                let at = from + at;
                bomb[at + offset..at + offset + 4].copy_from_slice(&0x7fff_ffffu32.to_le_bytes());
                from = at + 4;
            }
        }
        fx.add_mod("bomb.zip", &bomb);
        let before = fx.snap();
        let report = fx.apply().unwrap();
        assert_eq!(report.skipped.len(), 1, "{report:#?}");
        assert_eq!(fx.snap(), before);
    }

    #[test]
    fn every_failure_point_leaves_the_game_exactly_as_it_was() {
        let fx = fixture();
        fx.add_mod(
            "1_res.zip",
            &resource_mod(&[("texture/a.png", b"MOD"), ("deep/new/n.png", b"n")]),
        );
        fx.add_mod(
            "2_class.zip",
            &class_mod("20261110044614", &[("c/D.class", b"1")]),
        );
        fx.add_mod(
            "3_loc.zip",
            &simple_zip(&[(
                "mod.json",
                br#"{"locale": {"messages.properties": {"hello": "Hi"}}}"#,
            )]),
        );
        // A previous apply, so the stale-file path is exercised too.
        fx.apply().unwrap();
        fs::remove_file(fx.mods.join("1_res.zip")).unwrap();
        fx.add_mod(
            "4_res.zip",
            &resource_mod(&[("texture/b.png", b"B2"), ("item/x/model.dat", b"M2")]),
        );
        let before = fx.snap();
        let source = FakeSource::default();
        let mut k = 0;
        loop {
            match run(
                &fx.root,
                Some(&fx.mods),
                ApplyOptions::default(),
                &source,
                &BTreeMap::new(),
                Some(k),
            ) {
                Ok(_) => break,
                Err(e) => {
                    assert!(e.to_string().contains("injected"), "step {k}: {e}");
                    assert_eq!(fx.snap(), before, "failure at step {k}");
                    assert!(!txn_leftovers(&fx.root), "step {k}");
                }
            }
            k += 1;
        }
        assert!(k >= 8, "only {k} failure points were exercised");
        assert_ne!(
            fx.snap(),
            before,
            "the run past the last fault point applied"
        );
        assert_eq!(fx.read("rsrc/texture/b.png"), b"B2");
        assert_eq!(fx.read("rsrc/texture/a.png"), b"png-a");
    }

    #[test]
    fn a_crashed_apply_is_rolled_back_by_the_next_one() {
        let fx = fixture();
        fx.add_mod("a.zip", &resource_mod(&[("texture/a.png", b"MOD")]));
        let before = fx.snap();
        // Simulate a crash: a transaction that changed files and was never finished.
        let mut t = Txn::begin(&fx.root).unwrap();
        t.put("rsrc/texture/a.png", b"HALF").unwrap();
        t.put("code/config.jar", b"torn").unwrap();
        std::mem::forget(t);
        fx.apply().unwrap();
        assert_eq!(fx.read("rsrc/texture/a.png"), b"MOD");
        assert_eq!(fx.read("code/config.jar"), fx.config_jar);
        assert_ne!(fx.snap(), before);
        assert!(!txn_leftovers(&fx.root));
    }

    #[test]
    fn a_modified_code_jar_is_restored_from_the_kept_copy_without_the_network() {
        let fx = fixture();
        fx.apply().unwrap(); // sees the real jars and keeps verified copies
                             // Something else edits config.jar (a per-server setting, say).
        let mut jar = Jar::from_bytes(&fx.read("code/config.jar"), &Limits::CODE_JAR).unwrap();
        jar.set("deployment.properties", b"server=private\n".to_vec())
            .unwrap();
        jar.write_atomic(&fx.root.join("code/config.jar")).unwrap();
        assert_ne!(fx.read("code/config.jar"), fx.config_jar);
        let source = FakeSource::default();
        let r = apply_with(&fx.root, Some(&fx.mods), ApplyOptions::default(), &source).unwrap();
        assert_eq!(fx.read("code/config.jar"), fx.config_jar);
        assert_eq!(source.calls.get(), 0);
        assert_eq!(
            r.code_jars[0],
            ("config.jar".to_owned(), JarOutcome::Restored)
        );
    }

    #[test]
    fn with_no_kept_copy_the_original_is_fetched_and_verified() {
        let fx = fixture();
        let mut jar = Jar::from_bytes(&fx.config_jar, &Limits::CODE_JAR).unwrap();
        jar.set("deployment.properties", b"server=private\n".to_vec())
            .unwrap();
        jar.write_atomic(&fx.root.join("code/config.jar")).unwrap();
        let modified = fx.read("code/config.jar");

        // Offline: nothing changes.
        let before = fx.snap();
        let err = apply_with(
            &fx.root,
            Some(&fx.mods),
            ApplyOptions::default(),
            &FakeSource::default(),
        )
        .unwrap_err();
        assert!(matches!(err, ApplyError::NoPristine { .. }), "{err}");
        assert_eq!(fx.snap(), before);

        // A server that sends something else: refused, nothing changes.
        let mut liar = fx.source();
        liar.files
            .insert("code/config.jar".into(), modified.clone());
        let err = apply_with(&fx.root, Some(&fx.mods), ApplyOptions::default(), &liar).unwrap_err();
        assert!(err.to_string().contains("does not match"), "{err}");
        assert_eq!(fx.snap(), before);
        assert!(
            !fx.root.join(".skradle/pristine/code/config.jar").exists(),
            "a bad copy is never kept"
        );

        // The right bytes: used, and kept for next time.
        let honest = fx.source();
        apply_with(&fx.root, Some(&fx.mods), ApplyOptions::default(), &honest).unwrap();
        assert_eq!(fx.read("code/config.jar"), fx.config_jar);
        assert_eq!(honest.calls.get(), 1);
        assert!(fx.root.join(".skradle/pristine/code/config.jar").exists());
    }

    #[test]
    fn a_game_folder_that_is_not_installed_is_refused() {
        let tmp = tempfile::tempdir().unwrap();
        let source = FakeSource::default();
        let go =
            |root: &Path| apply_with(root, None, ApplyOptions::default(), &source).unwrap_err();
        assert!(matches!(go(tmp.path()), ApplyError::GameNotInstalled(_)));
        let fx = fixture();
        fs::remove_file(fx.root.join("digest2.txt")).unwrap();
        assert!(matches!(go(&fx.root), ApplyError::GameNotInstalled(_)));
        let fx = fixture();
        put(
            &fx.root,
            "digest2.txt",
            format!("getdown.txt = {}\n", "00".repeat(32)).as_bytes(),
        );
        assert!(matches!(go(&fx.root), ApplyError::NoDigest(_)));
    }

    #[test]
    fn rebuild_resources_clears_what_other_tools_left_behind() {
        let fx = fixture();
        put(
            &fx.root,
            "rsrc/junk/leftover.png",
            b"left by something else",
        );
        put(&fx.root, "rsrc/texture/a.png", b"hand edited");
        fs::remove_file(fx.root.join("rsrc/sound/s.ogg")).unwrap();
        fx.add_mod("m.zip", &resource_mod(&[("texture/b.png", b"MOD")]));
        let source = FakeSource::default();
        let options = ApplyOptions {
            rebuild_resources: true,
            ..ApplyOptions::default()
        };
        let report = apply_with(&fx.root, Some(&fx.mods), options, &source).unwrap();
        assert!(!fx.root.join("rsrc/junk/leftover.png").exists());
        assert_eq!(fx.read("rsrc/texture/a.png"), b"png-a");
        assert_eq!(fx.read("rsrc/sound/s.ogg"), b"ogg");
        assert_eq!(fx.read("rsrc/texture/b.png"), b"MOD");
        assert!(
            fx.root.join("rsrc/intro-bundle.jar").exists(),
            "bundles are never touched"
        );
        assert!(fx.root.join("rsrc/intro-bundle.jarv").exists());
        assert_eq!(report.resource_files_removed, 1);
        assert_eq!(report.resource_files_restored, 2);
    }

    #[cfg(unix)]
    #[test]
    fn mods_never_write_through_links_in_the_game_folder() {
        let fx = fixture();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("victim.png"), b"precious").unwrap();
        fs::remove_file(fx.root.join("rsrc/texture/a.png")).unwrap();
        std::os::unix::fs::symlink(
            outside.path().join("victim.png"),
            fx.root.join("rsrc/texture/a.png"),
        )
        .unwrap();
        fs::remove_dir_all(fx.root.join("rsrc/item")).unwrap();
        std::os::unix::fs::symlink(outside.path(), fx.root.join("rsrc/item")).unwrap();
        fx.add_mod(
            "m.zip",
            &resource_mod(&[("texture/a.png", b"MOD"), ("item/x/model.dat", b"MOD2")]),
        );
        let before = fx.snap();
        let err = fx.apply().unwrap_err();
        assert!(err.to_string().contains("link"), "{err}");
        assert_eq!(fx.snap(), before);
        assert_eq!(
            fs::read(outside.path().join("victim.png")).unwrap(),
            b"precious"
        );
        assert!(!outside.path().join("x").exists());
        // Without the folder link the file link alone is replaced, never followed.
        fs::remove_file(fx.root.join("rsrc/item")).unwrap();
        fx.add_mod("m.zip", &resource_mod(&[("texture/a.png", b"MOD")]));
        fx.apply().unwrap();
        assert_eq!(fx.read("rsrc/texture/a.png"), b"MOD");
        assert_eq!(
            fs::read(outside.path().join("victim.png")).unwrap(),
            b"precious"
        );
    }

    #[test]
    fn mods_with_special_names_round_trip() {
        let fx = fixture();
        fx.add_mod(
            "Mod With Spaces & (parens).zip",
            &resource_mod(&[("RVH README.txt", b"hello"), ("a b/c d.png", b"x")]),
        );
        fx.apply().unwrap();
        assert_eq!(fx.read("rsrc/RVH README.txt"), b"hello");
        restore_with_fake(&fx);
        assert!(!fx.root.join("rsrc/RVH README.txt").exists());
        assert!(!fx.root.join("rsrc/a b/c d.png").exists());
    }

    fn restore_with_fake(fx: &Fixture) {
        apply_with(
            &fx.root,
            None,
            ApplyOptions::default(),
            &FakeSource::default(),
        )
        .unwrap();
    }

    #[test]
    fn symlink_entries_inside_a_mod_reject_the_mod() {
        let fx = fixture();
        fx.add_mod(
            "link.zip",
            &zip_with(&[("texture/a.png", b"/etc/passwd", Some(0o120777))]),
        );
        let before = fx.snap();
        let report = fx.apply().unwrap();
        assert_eq!(report.skipped.len(), 1);
        assert!(report.skipped[0].reason.contains("symbolic link"));
        assert_eq!(fx.snap(), before);
    }

    #[test]
    fn a_damaged_record_of_earlier_changes_only_warns() {
        let fx = fixture();
        put(&fx.root, ".skradle/applied.json", b"{ broken");
        let report = fx.apply().unwrap();
        assert!(
            report.warnings.iter().any(|w| w.contains("damaged")),
            "{:?}",
            report.warnings
        );
        let record = fs::read_to_string(fx.root.join(".skradle/applied.json")).unwrap_or_default();
        assert!(
            record.is_empty(),
            "the damaged record is replaced: {record}"
        );
    }

    /// Runs against a real install, read only: copies nothing and writes nothing to it. It checks that the whole
    /// planning side (digests, pristine jars, bundle index) works on real data by applying to a copy of the small
    /// files only. Set SKRADLE_REAL_GAME to the game folder.
    #[test]
    #[ignore = "needs a real game folder in SKRADLE_REAL_GAME"]
    fn real_bundles_index_cleanly() {
        let Ok(root) = std::env::var("SKRADLE_REAL_GAME") else {
            return;
        };
        let root = Path::new(&root);
        let digests = Digests::read(&root.join("digest2.txt")).unwrap();
        let mut bundles = Bundles::open(root, &digests).unwrap();
        let n = bundles.paths().len();
        println!(
            "{n} files in the real bundles, {} skipped as unsafe",
            bundles.archives.iter().map(Archive::skipped).sum::<usize>()
        );
        assert!(n > 10_000);
        assert!(bundles.read("config/item.dat").unwrap().is_some());
    }
}
