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

//! "Reset game files": put the game folder back to its original, unmodified state, offline.
//!
//! This is the apply engine run with no mods and with every loose file of `rsrc/` rebuilt from the game's own bundles
//! ([`ApplyOptions::rebuild_resources`], file cleaning and purging on):
//!
//! 1. the code jars go back to the game's original (the verified copy the launcher keeps; only a game folder that
//!    was modified by something else before the launcher first saw it needs the game's server, see
//!    `mods::source`);
//! 2. the loose files mods wrote or replaced are restored from the bundles, and the files mods added (the ones the
//!    launcher recorded) are removed;
//! 3. every other loose file of `rsrc/` is rebuilt from the bundles too, which clears what other tools left behind.
//!
//! Nothing is downloaded and no game file is deleted for the updater to fetch again. The player's mods folder is
//! not even a parameter, and nothing outside the game folder is touched. The next launch applies the mods again,
//! because the code jars no longer match what the last apply left.

use std::fmt;
use std::fs;
use std::path::Path;

use crate::mods::apply::apply_with;
use crate::mods::source::PristineSource;
use crate::mods::ApplyOptions;

#[derive(Debug)]
pub enum ResetError {
    /// The folder has no `getdown.txt` and `digest2.txt`, so there is nothing to reset against.
    NotInstalled(String),
    /// The restore failed (nothing is left half done: the apply is one transaction).
    Restore(String),
}

impl fmt::Display for ResetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ResetError::NotInstalled(why) => write!(f, "the game is not installed here: {why}"),
            ResetError::Restore(why) => write!(f, "{why}"),
        }
    }
}

impl std::error::Error for ResetError {}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct ResetReport {
    /// Loose files put back, written or removed.
    pub changed: usize,
    pub warnings: Vec<String>,
}

/// Resets the game in `root`; originals the launcher does not have come from the game's own server.
pub fn reset(root: &Path) -> Result<ResetReport, ResetError> {
    reset_game_files(root, crate::mods::apply::default_source(root).as_ref())
}

/// [`reset`] with the source of original jars supplied.
pub fn reset_game_files(
    root: &Path,
    source: &dyn PristineSource,
) -> Result<ResetReport, ResetError> {
    for file in ["getdown.txt", "digest2.txt"] {
        if !fs::symlink_metadata(root.join(file)).is_ok_and(|m| m.is_file()) {
            return Err(ResetError::NotInstalled(format!("{file} is missing")));
        }
    }
    let options = ApplyOptions {
        rebuild_resources: true,
        ..ApplyOptions::default()
    };
    let report =
        apply_with(root, None, options, source).map_err(|e| ResetError::Restore(e.to_string()))?;
    Ok(ResetReport {
        changed: report.resource_files_written
            + report.resource_files_restored
            + report.resource_files_removed,
        warnings: report.warnings,
    })
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

    #[test]
    fn refuses_without_the_digests_and_changes_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("getdown.txt"), b"version = 1\n").unwrap();
        fs::write(tmp.path().join("keep.txt"), b"keep").unwrap();
        assert!(matches!(
            reset_game_files(tmp.path(), &NoSource),
            Err(ResetError::NotInstalled(_))
        ));
        assert_eq!(fs::read(tmp.path().join("keep.txt")).unwrap(), b"keep");
    }

    #[test]
    fn a_folder_that_is_not_a_complete_game_fails_without_deleting_anything() {
        let tmp = tempfile::tempdir().unwrap();
        for (rel, bytes) in [
            ("getdown.txt", &b"version = 1\n"[..]),
            ("digest2.txt", b"getdown.txt = 00\n"),
            ("code/config.jar", b"jar"),
            ("rsrc/loose/a.png", b"png"),
        ] {
            let path = tmp.path().join(rel);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, bytes).unwrap();
        }
        assert!(matches!(
            reset_game_files(tmp.path(), &NoSource),
            Err(ResetError::Restore(_))
        ));
        for rel in ["code/config.jar", "rsrc/loose/a.png", "digest2.txt"] {
            assert!(tmp.path().join(rel).exists(), "{rel} must stay");
        }
    }
}
