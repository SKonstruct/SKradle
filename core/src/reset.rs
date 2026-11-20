//! "Reset game files": remove every applied mod and make the game's own updater download every game file again.
//!
//! How it works, from the digests and the updater's behaviour (see `docs/launching.md` and `docs/mods.md`):
//!
//! 1. All mods are removed with [`crate::mods::apply_with`] (no mods folder): the code jars go back to the game's
//!    original and the loose files the launcher recorded are restored or deleted. This is best effort; when it fails
//!    (for example no verified original and no network) the reset goes on, because the next step replaces those jars
//!    anyway, and the failure is reported as a warning.
//! 2. Every file `digest2.txt` lists is deleted, together with its `<name>v` marker. The updater downloads every file
//!    that is missing or differs on its next run, and the launcher's planner sends the next launch through the
//!    updater because the game's code jars are missing.
//!
//! What is never touched: anything outside the game folder, the player's mods folder (it is not even a parameter),
//! folders, `getdown.txt` and the digest files themselves (the updater needs them to know what to download), the
//! updater (`getdown-pro.jar`, and any `code/getdown*` file), and files reached through a link that leaves the
//! folder. Paths come only from `digest2.txt`, which [`Digests`] already restricts to plain relative paths; they are
//! checked again here.

use std::fmt;
use std::fs;
use std::path::Path;

use crate::digest::Digests;
use crate::jar::safe_entry_path;
use crate::mods::apply::apply_with;
use crate::mods::source::PristineSource;
use crate::mods::ApplyOptions;

#[derive(Debug)]
pub enum ResetError {
    /// The folder has no `getdown.txt` and `digest2.txt`, so there is nothing to reset against.
    NotInstalled(String),
    /// `digest2.txt` lists no file the reset may remove; nothing was changed.
    NothingListed,
    Io(std::io::Error),
}

impl fmt::Display for ResetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ResetError::NotInstalled(why) => write!(f, "the game is not installed here: {why}"),
            ResetError::NothingListed => write!(f, "digest2.txt lists no game files"),
            ResetError::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for ResetError {}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct ResetReport {
    /// Files and markers removed.
    pub removed: usize,
    /// Files the digests list that were not there (nothing to remove).
    pub already_missing: usize,
    /// Why mods could not be removed first, if that failed.
    pub warnings: Vec<String>,
}

/// True for files the reset keeps: the updater's own files and the files that tell it what to download.
fn is_kept(rel: &str) -> bool {
    let name = rel.rsplit('/').next().unwrap_or(rel);
    rel == "getdown.txt"
        || name.starts_with("getdown")
        || name == "digest.txt"
        || name == "digest2.txt"
        || name.starts_with("digest.txt.")
        || name.starts_with("digest2.txt.")
}

/// The relative paths a reset would remove, in digest order (no marker files, which are derived).
pub fn files_to_remove(digests: &Digests) -> Vec<String> {
    digests
        .paths()
        .filter(|rel| !is_kept(rel))
        .filter(|rel| safe_entry_path(rel).is_ok_and(|(_, is_dir)| !is_dir))
        .map(str::to_owned)
        .collect()
}

/// True when no folder between `root` and the file is a link (a link could lead out of the game folder).
fn parents_are_plain(root: &Path, rel: &str) -> bool {
    let mut path = root.to_path_buf();
    let mut parts: Vec<&str> = rel.split('/').collect();
    parts.pop();
    for part in parts {
        path.push(part);
        match fs::symlink_metadata(&path) {
            Ok(m) if m.is_dir() => {}
            Ok(_) => return false,
            // A missing folder means the file is missing too.
            Err(_) => return true,
        }
    }
    true
}

/// Removes one file if it is a plain file or a link to one (the link itself goes); folders are never removed.
fn remove_file_at(root: &Path, rel: &str) -> Result<Option<bool>, ResetError> {
    if !parents_are_plain(root, rel) {
        return Ok(None);
    }
    let path = root.join(rel);
    match fs::symlink_metadata(&path) {
        Ok(m) if m.is_dir() => Ok(None),
        Ok(_) => {
            fs::remove_file(&path).map_err(ResetError::Io)?;
            Ok(Some(true))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Some(false)),
        Err(e) => Err(ResetError::Io(e)),
    }
}

/// [`reset_game_files`] with the game's own servers as the source of originals.
pub fn reset(root: &Path) -> Result<ResetReport, ResetError> {
    reset_game_files(root, crate::mods::apply::default_source(root).as_ref())
}

/// Resets the game in `root`. `source` supplies original jars for the mod removal (the real one downloads from the
/// game's servers; tests pass a fake).
pub fn reset_game_files(
    root: &Path,
    source: &dyn PristineSource,
) -> Result<ResetReport, ResetError> {
    for file in ["getdown.txt", "digest2.txt"] {
        if !fs::symlink_metadata(root.join(file)).is_ok_and(|m| m.is_file()) {
            return Err(ResetError::NotInstalled(format!("{file} is missing")));
        }
    }
    let digests = Digests::read(&root.join("digest2.txt")).map_err(ResetError::Io)?;
    let targets = files_to_remove(&digests);
    if targets.is_empty() {
        return Err(ResetError::NothingListed);
    }
    let mut report = ResetReport::default();
    if let Err(e) = apply_with(root, None, ApplyOptions::default(), source) {
        report
            .warnings
            .push(format!("mods could not be removed first: {e}"));
    }
    for rel in &targets {
        for name in [rel.clone(), format!("{rel}v")] {
            match remove_file_at(root, &name)? {
                Some(true) => report.removed += 1,
                Some(false) if name == *rel => report.already_missing += 1,
                _ => {}
            }
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct NoSource;
    impl PristineSource for NoSource {
        fn fetch(&self, _: &str, _: u64) -> Result<Vec<u8>, String> {
            Err("offline".into())
        }
    }

    fn hash(c: char) -> String {
        c.to_string().repeat(64)
    }

    fn write(root: &Path, rel: &str, bytes: &[u8]) {
        let path = root.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }

    /// A game folder, a mods folder and an unrelated folder next to it, all in one temporary folder.
    fn world() -> (tempfile::TempDir, std::path::PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("game");
        let digest = format!(
            "getdown.txt = {}\ncode/config.jar = {}\ncode/lwjgl.jar = {}\ncode/getdown-pro-new.jar = {}\n\
             native/libx.so = {}\nrsrc/intro-bundle.jar = {}\nmissing.png = {}\ndigest2.txt = {}\n",
            hash('a'),
            hash('b'),
            hash('c'),
            hash('d'),
            hash('e'),
            hash('f'),
            hash('1'),
            hash('2')
        );
        write(&root, "digest2.txt", digest.as_bytes());
        write(&root, "digest2.txt.sig", b"sig");
        write(&root, "getdown.txt", b"version = 1\n");
        write(&root, "getdown-pro.jar", b"updater");
        write(&root, "code/config.jar", b"c");
        write(&root, "code/lwjgl.jar", b"l");
        write(&root, "code/getdown-pro-new.jar", b"u");
        write(&root, "native/libx.so", b"n");
        write(&root, "rsrc/intro-bundle.jar", b"b");
        write(&root, "rsrc/intro-bundle.jarv", b"");
        write(&root, "rsrc/loose/unlisted.png", b"not in the digests");
        write(&root, "extra.txt", b"-Xmx2g\n");
        write(&tmp.path().join("mods"), "My Mod.zip", b"player's mod");
        write(
            &tmp.path().join("elsewhere"),
            "code/config.jar",
            b"someone else's",
        );
        (tmp, root)
    }

    fn exists(root: &Path, rel: &str) -> bool {
        root.join(rel).exists()
    }

    #[test]
    fn removes_listed_game_files_and_markers_and_keeps_everything_else() {
        let (tmp, root) = world();
        let report = reset_game_files(&root, &NoSource).unwrap();
        for gone in [
            "code/config.jar",
            "code/lwjgl.jar",
            "native/libx.so",
            "rsrc/intro-bundle.jar",
            "rsrc/intro-bundle.jarv",
        ] {
            assert!(!exists(&root, gone), "{gone} should be gone");
        }
        for kept in [
            "getdown.txt",
            "digest2.txt",
            "digest2.txt.sig",
            "getdown-pro.jar",
            "code/getdown-pro-new.jar",
            "rsrc/loose/unlisted.png",
            "extra.txt",
        ] {
            assert!(exists(&root, kept), "{kept} must stay");
        }
        assert_eq!(report.removed, 5);
        assert_eq!(report.already_missing, 1, "missing.png was never there");
        // Outside the game folder nothing changes, in particular not the player's mods.
        assert_eq!(
            fs::read(tmp.path().join("mods/My Mod.zip")).unwrap(),
            b"player's mod"
        );
        assert_eq!(
            fs::read(tmp.path().join("elsewhere/code/config.jar")).unwrap(),
            b"someone else's"
        );
        // Mod removal failed (no original, offline): reported, not fatal.
        assert_eq!(report.warnings.len(), 1, "{:?}", report.warnings);
    }

    #[cfg(unix)]
    #[test]
    fn never_deletes_through_a_link_that_leaves_the_game_folder() {
        let (tmp, root) = world();
        // `code` is replaced by a link to another folder holding a file with a listed name.
        fs::remove_dir_all(root.join("code")).unwrap();
        std::os::unix::fs::symlink(tmp.path().join("elsewhere/code"), root.join("code")).unwrap();
        reset_game_files(&root, &NoSource).unwrap();
        assert_eq!(
            fs::read(tmp.path().join("elsewhere/code/config.jar")).unwrap(),
            b"someone else's"
        );
        // A link at the file itself is removed as a link; its target stays.
        let (tmp, root) = world();
        fs::remove_file(root.join("native/libx.so")).unwrap();
        std::os::unix::fs::symlink(
            tmp.path().join("mods/My Mod.zip"),
            root.join("native/libx.so"),
        )
        .unwrap();
        reset_game_files(&root, &NoSource).unwrap();
        assert!(!exists(&root, "native/libx.so"));
        assert!(tmp.path().join("mods/My Mod.zip").exists());
    }

    #[test]
    fn a_folder_with_a_listed_name_is_never_removed() {
        let (_tmp, root) = world();
        fs::remove_file(root.join("native/libx.so")).unwrap();
        fs::create_dir_all(root.join("native/libx.so/inner")).unwrap();
        reset_game_files(&root, &NoSource).unwrap();
        assert!(exists(&root, "native/libx.so/inner"));
    }

    #[test]
    fn refuses_without_the_digests_and_changes_nothing() {
        let (_tmp, root) = world();
        fs::remove_file(root.join("digest2.txt")).unwrap();
        assert!(matches!(
            reset_game_files(&root, &NoSource),
            Err(ResetError::NotInstalled(_))
        ));
        assert!(exists(&root, "code/config.jar"));
    }

    #[test]
    fn the_list_skips_the_updater_and_the_digest_files_and_unsafe_names() {
        let digests = Digests::parse(&format!(
            "getdown.txt = {a}\ndigest.txt = {a}\ndigest2.txt = {a}\ncode/getdown-pro-new.jar = {a}\n\
             ../outside = {a}\n/abs = {a}\ncode/ok.jar = {a}\n",
            a = hash('a')
        ));
        assert_eq!(files_to_remove(&digests), ["code/ok.jar"]);
    }
}
