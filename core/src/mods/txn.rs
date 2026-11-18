//! All-or-nothing changes to files inside the game folder.
//!
//! A [`Txn`] is a journal plus backups. Before a file is replaced or deleted, the original is moved (renamed, so it is
//! kept byte for byte, and a symlink would be kept as the link) into `.skradle/txn/b/`, and the intent is appended to
//! `.skradle/txn/journal`. New content is written to `<file>.skradle-tmp`, synced and renamed into place. If anything
//! fails, [`Txn::rollback`] puts every original back and removes every file and folder the transaction created. If the
//! process dies half way, the journal is still there, and [`recover`] (which [`Txn::begin`] insists on) rolls back at
//! the next start. [`Txn::commit`] renames the journal to `committed` first, so a crash while the backups are being
//! deleted is never mistaken for an unfinished change.
//!
//! Guarantees and limits: a crash of the launcher or an error leaves the files as they were. A power cut can lose
//! journal lines that were still in the operating system's cache (the journal is not fsynced per entry, which would
//! make a few thousand small writes crawl), in which case the files touched last may stay changed; the next apply
//! corrects them, because it works from the digests, not from the journal alone.
//!
//! Safety: every path is a validated relative path (see [`crate::jar::safe_entry_path`]); no existing folder on the
//! way may be a link; a link or file where a folder is needed is an error; nothing is ever written through a link.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::jar::{safe_entry_path, write_atomic};

/// Launcher-owned folder inside the game folder.
pub const STATE_DIR: &str = ".skradle";
const TXN_DIR: &str = "txn";
const JOURNAL: &str = "journal";
const COMMITTED: &str = "committed";
const BACKUPS: &str = "b";

fn other(msg: impl Into<String>) -> io::Error {
    io::Error::other(msg.into())
}

fn txn_dir(root: &Path) -> PathBuf {
    root.join(STATE_DIR).join(TXN_DIR)
}

/// Checks `rel`, makes sure no existing folder on its way is a link, and returns the full path plus the folders that
/// do not exist yet (outermost first).
fn resolve(root: &Path, rel: &str) -> io::Result<(PathBuf, Vec<PathBuf>)> {
    let (clean, is_dir) =
        safe_entry_path(rel).map_err(|p| other(format!("unsafe path {rel:?}: {p}")))?;
    if is_dir {
        return Err(other(format!("{rel:?} names a folder")));
    }
    let parts: Vec<&str> = clean.split('/').collect();
    let mut current = root.to_path_buf();
    let mut missing = Vec::new();
    for part in &parts[..parts.len() - 1] {
        current.push(part);
        if !missing.is_empty() {
            missing.push(current.clone());
            continue;
        }
        match fs::symlink_metadata(&current) {
            Ok(m) if m.is_dir() => {}
            Ok(_) => {
                return Err(other(format!(
                    "{} is a link or a file where a folder is needed",
                    current.display()
                )))
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => missing.push(current.clone()),
            Err(e) => return Err(e),
        }
    }
    current.push(parts[parts.len() - 1]);
    Ok((current, missing))
}

/// What is at `path`, without following links: `None` when nothing is there.
fn existing(path: &Path) -> io::Result<Option<fs::Metadata>> {
    match fs::symlink_metadata(path) {
        Ok(m) => Ok(Some(m)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

/// A transaction. Dropping it without [`Txn::commit`] rolls it back.
pub struct Txn {
    root: PathBuf,
    journal: fs::File,
    next_backup: u64,
    finished: bool,
    /// Test hook: fail the n-th (0-based) step. Steps are the points inside `put` and `delete` where a real write
    /// could fail.
    fail_at: Option<usize>,
    steps: usize,
}

impl Txn {
    /// Starts a transaction in the game folder `root`. A transaction left over by a crash is rolled back first.
    pub fn begin(root: &Path) -> io::Result<Txn> {
        Self::begin_with_fault(root, None)
    }

    pub(crate) fn begin_with_fault(root: &Path, fail_at: Option<usize>) -> io::Result<Txn> {
        recover(root)?;
        let dir = txn_dir(root);
        fs::create_dir_all(dir.join(BACKUPS))?;
        let journal = OpenOptions::new()
            .create_new(true)
            .append(true)
            .open(dir.join(JOURNAL))?;
        Ok(Txn {
            root: root.to_path_buf(),
            journal,
            next_backup: 0,
            finished: false,
            fail_at,
            steps: 0,
        })
    }

    /// How many fault points have been passed (for tests that fail each one in turn).
    #[cfg(test)]
    pub(crate) fn steps(&self) -> usize {
        self.steps
    }

    fn fault(&mut self) -> io::Result<()> {
        let n = self.steps;
        self.steps += 1;
        if self.fail_at == Some(n) {
            return Err(other(format!("injected failure at step {n}")));
        }
        Ok(())
    }

    fn log(&mut self, line: &str) -> io::Result<()> {
        debug_assert!(!line.contains('\n'));
        self.journal.write_all(format!("{line}\n").as_bytes())
    }

    /// Makes the file at `rel` (relative to the game folder) contain `bytes`, keeping the original for a rollback.
    pub fn put(&mut self, rel: &str, bytes: &[u8]) -> io::Result<()> {
        let (path, missing) = resolve(&self.root, rel)?;
        let original = existing(&path)?;
        if original.as_ref().is_some_and(|m| m.is_dir()) {
            return Err(other(format!("{} is a folder", path.display())));
        }
        for dir in missing {
            let rel_dir = dir
                .strip_prefix(&self.root)
                .map_err(|_| other("path outside the game folder"))?
                .to_string_lossy()
                .replace('\\', "/");
            self.log(&format!("D\t{rel_dir}"))?;
            self.fault()?;
            fs::create_dir(&dir)?;
        }
        self.replace_with(rel, &path, original.is_some(), Some(bytes))
    }

    /// Removes the file at `rel`, keeping it for a rollback. Nothing happens if it is not there.
    pub fn delete(&mut self, rel: &str) -> io::Result<()> {
        let (path, missing) = resolve(&self.root, rel)?;
        if !missing.is_empty() {
            return Ok(());
        }
        match existing(&path)? {
            None => Ok(()),
            Some(m) if m.is_dir() => Err(other(format!("{} is a folder", path.display()))),
            Some(_) => self.replace_with(rel, &path, true, None),
        }
    }

    fn replace_with(
        &mut self,
        rel: &str,
        path: &Path,
        had_original: bool,
        bytes: Option<&[u8]>,
    ) -> io::Result<()> {
        let id = self.next_backup;
        self.next_backup += 1;
        self.log(&format!("F\t{}\t{id}\t{rel}", u8::from(had_original)))?;
        self.fault()?;
        if had_original {
            fs::rename(path, txn_dir(&self.root).join(BACKUPS).join(id.to_string()))?;
        }
        self.fault()?;
        if let Some(bytes) = bytes {
            write_atomic(path, bytes)?;
        }
        Ok(())
    }

    /// Keeps the changes and deletes the backups.
    pub fn commit(mut self) -> io::Result<()> {
        self.finished = true;
        let dir = txn_dir(&self.root);
        fs::rename(dir.join(JOURNAL), dir.join(COMMITTED))?;
        fs::remove_dir_all(&dir)?;
        remove_state_dir_if_empty(&self.root);
        Ok(())
    }

    /// Undoes every change.
    pub fn rollback(mut self) -> io::Result<()> {
        self.finished = true;
        rollback_dir(&self.root)
    }
}

impl Drop for Txn {
    fn drop(&mut self) {
        if !self.finished {
            let _ = rollback_dir(&self.root);
        }
    }
}

/// Rolls back a transaction a crash left behind. Returns whether there was one to roll back.
pub fn recover(root: &Path) -> io::Result<bool> {
    let dir = txn_dir(root);
    if existing(&dir)?.is_none() {
        return Ok(false);
    }
    if existing(&dir.join(COMMITTED))?.is_some() {
        // The change was complete; only the clean-up was interrupted.
        fs::remove_dir_all(&dir)?;
        remove_state_dir_if_empty(root);
        return Ok(false);
    }
    rollback_dir(root)?;
    Ok(true)
}

fn rollback_dir(root: &Path) -> io::Result<()> {
    let dir = txn_dir(root);
    let journal = match fs::read(dir.join(JOURNAL)) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == io::ErrorKind::NotFound => Vec::new(),
        Err(e) => return Err(e),
    };
    let text = String::from_utf8_lossy(&journal);
    // A last line without its newline was being written when the process died: that step never began.
    let complete = match text.rfind('\n') {
        Some(at) => &text[..=at],
        None => "",
    };
    let mut first_error: Option<io::Error> = None;
    let mut note = |r: io::Result<()>| {
        if let Err(e) = r {
            first_error.get_or_insert(e);
        }
    };
    for line in complete.lines().rev() {
        let fields: Vec<&str> = line.splitn(4, '\t').collect();
        match fields.as_slice() {
            ["F", had, id, rel] => note(undo_file(root, rel, *had == "1", id)),
            // Only an empty folder this transaction created is removed; anything else is left alone.
            ["D", rel] if safe_entry_path(rel).is_ok() => {
                let path = root.join(rel);
                if existing(&path).is_ok_and(|m| m.is_some_and(|m| m.is_dir())) {
                    let _ = fs::remove_dir(&path);
                }
            }
            _ => {}
        }
    }
    match first_error {
        // Keep the journal and the backups: the next start tries again.
        Some(e) => Err(e),
        None => {
            match fs::remove_dir_all(&dir) {
                Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
                _ => {}
            }
            remove_state_dir_if_empty(root);
            Ok(())
        }
    }
}

/// Removes `.skradle` when nothing else lives in it, so a finished or undone change leaves no trace.
fn remove_state_dir_if_empty(root: &Path) {
    let _ = fs::remove_dir(root.join(STATE_DIR));
}

fn undo_file(root: &Path, rel: &str, had_original: bool, id: &str) -> io::Result<()> {
    let (path, missing) = resolve(root, rel)?;
    let backup = txn_dir(root).join(BACKUPS).join(id);
    let backup_exists = existing(&backup)?.is_some();
    let _ = fs::remove_file(crate::jar::temp_path(&path));
    if had_original && !backup_exists {
        // The step was logged but the original was never moved away: it is still in place.
        return Ok(());
    }
    if missing.is_empty() {
        if let Some(m) = existing(&path)? {
            if m.is_dir() {
                return Err(other(format!("{} is a folder", path.display())));
            }
            fs::remove_file(&path)?;
        }
    }
    if had_original {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::rename(&backup, &path)?;
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod testutil {
    use super::*;
    use std::collections::BTreeMap;

    /// Every file and symlink under `root`, with its content (a link shows as `link -> target`), for exact
    /// before and after comparisons. Empty folders are listed as `dir/`.
    pub fn snapshot(root: &Path) -> BTreeMap<String, Vec<u8>> {
        fn walk(dir: &Path, root: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
            for entry in fs::read_dir(dir).unwrap() {
                let entry = entry.unwrap();
                let path = entry.path();
                let rel = path
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                let meta = fs::symlink_metadata(&path).unwrap();
                if meta.file_type().is_symlink() {
                    let target = fs::read_link(&path).unwrap();
                    out.insert(rel, format!("link -> {}", target.display()).into_bytes());
                } else if meta.is_dir() {
                    if fs::read_dir(&path).unwrap().next().is_none() {
                        out.insert(format!("{rel}/"), Vec::new());
                    }
                    walk(&path, root, out);
                } else {
                    out.insert(rel, fs::read(&path).unwrap());
                }
            }
        }
        let mut out = BTreeMap::new();
        walk(root, root, &mut out);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::testutil::snapshot;
    use super::*;

    fn game() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("rsrc/a")).unwrap();
        fs::write(dir.path().join("rsrc/a/one.txt"), b"one").unwrap();
        fs::write(dir.path().join("rsrc/two.txt"), b"two").unwrap();
        dir
    }

    #[test]
    fn commit_keeps_changes_and_cleans_up() {
        let dir = game();
        let mut txn = Txn::begin(dir.path()).unwrap();
        txn.put("rsrc/a/one.txt", b"ONE").unwrap();
        txn.put("rsrc/new/deep/file.txt", b"new").unwrap();
        txn.delete("rsrc/two.txt").unwrap();
        txn.delete("rsrc/never-existed.txt").unwrap();
        txn.commit().unwrap();
        assert_eq!(fs::read(dir.path().join("rsrc/a/one.txt")).unwrap(), b"ONE");
        assert_eq!(
            fs::read(dir.path().join("rsrc/new/deep/file.txt")).unwrap(),
            b"new"
        );
        assert!(!dir.path().join("rsrc/two.txt").exists());
        assert!(!txn_dir(dir.path()).exists());
    }

    #[test]
    fn rollback_restores_everything_byte_for_byte() {
        let dir = game();
        let before = snapshot(dir.path());
        let mut txn = Txn::begin(dir.path()).unwrap();
        txn.put("rsrc/a/one.txt", b"ONE").unwrap();
        txn.put("rsrc/new/deep/file.txt", b"new").unwrap();
        txn.delete("rsrc/two.txt").unwrap();
        txn.put("rsrc/two.txt", b"again").unwrap();
        txn.rollback().unwrap();
        assert_eq!(snapshot(dir.path()), before);
    }

    #[test]
    fn dropping_without_commit_rolls_back() {
        let dir = game();
        let before = snapshot(dir.path());
        {
            let mut txn = Txn::begin(dir.path()).unwrap();
            txn.put("rsrc/two.txt", b"changed").unwrap();
        }
        assert_eq!(snapshot(dir.path()), before);
    }

    #[test]
    fn a_crashed_transaction_is_rolled_back_by_the_next_one() {
        let dir = game();
        let before = snapshot(dir.path());
        let mut txn = Txn::begin(dir.path()).unwrap();
        txn.put("rsrc/two.txt", b"changed").unwrap();
        txn.put("rsrc/x/y.txt", b"new").unwrap();
        std::mem::forget(txn); // the process "dies": no rollback runs
        assert_ne!(snapshot(dir.path()), before);
        assert!(recover(dir.path()).unwrap());
        assert_eq!(snapshot(dir.path()), before);
        assert!(!recover(dir.path()).unwrap());
    }

    #[test]
    fn begin_recovers_a_leftover_transaction_itself() {
        let dir = game();
        let before = snapshot(dir.path());
        let mut txn = Txn::begin(dir.path()).unwrap();
        txn.delete("rsrc/two.txt").unwrap();
        std::mem::forget(txn);
        let second = Txn::begin(dir.path()).unwrap();
        second.rollback().unwrap();
        assert_eq!(snapshot(dir.path()), before);
    }

    #[test]
    fn a_torn_last_journal_line_and_a_committed_marker_are_handled() {
        let dir = game();
        let before = snapshot(dir.path());
        let mut txn = Txn::begin(dir.path()).unwrap();
        txn.put("rsrc/two.txt", b"changed").unwrap();
        std::mem::forget(txn);
        let mut journal = fs::OpenOptions::new()
            .append(true)
            .open(txn_dir(dir.path()).join(JOURNAL))
            .unwrap();
        journal.write_all(b"F\t1\t7\trsrc/a/one.t").unwrap();
        assert!(recover(dir.path()).unwrap());
        assert_eq!(snapshot(dir.path()), before);

        // A change that finished but whose clean-up was interrupted stays applied.
        let mut txn = Txn::begin(dir.path()).unwrap();
        txn.put("rsrc/two.txt", b"final").unwrap();
        std::mem::forget(txn);
        let d = txn_dir(dir.path());
        fs::rename(d.join(JOURNAL), d.join(COMMITTED)).unwrap();
        assert!(!recover(dir.path()).unwrap());
        assert_eq!(fs::read(dir.path().join("rsrc/two.txt")).unwrap(), b"final");
        assert!(!d.exists());
    }

    #[test]
    fn unsafe_paths_are_refused() {
        let dir = game();
        let mut txn = Txn::begin(dir.path()).unwrap();
        for bad in ["../x", "/abs", "a\\b", "rsrc/", "", "rsrc/../../x", "C:/x"] {
            assert!(txn.put(bad, b"x").is_err(), "{bad:?}");
            assert!(txn.delete(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn a_folder_where_a_file_goes_is_an_error_and_changes_nothing() {
        let dir = game();
        let before = snapshot(dir.path());
        let mut txn = Txn::begin(dir.path()).unwrap();
        assert!(txn.put("rsrc/a", b"x").is_err());
        assert!(txn.delete("rsrc/a").is_err());
        // A file where a folder is needed.
        assert!(txn.put("rsrc/two.txt/inner", b"x").is_err());
        txn.rollback().unwrap();
        assert_eq!(snapshot(dir.path()), before);
    }

    #[cfg(unix)]
    #[test]
    fn nothing_is_written_through_links() {
        let dir = game();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("victim.txt"), b"precious").unwrap();
        // A folder link in the way.
        std::os::unix::fs::symlink(outside.path(), dir.path().join("rsrc/linkdir")).unwrap();
        // A file link as the target.
        std::os::unix::fs::symlink(
            outside.path().join("victim.txt"),
            dir.path().join("rsrc/link.txt"),
        )
        .unwrap();
        let before = snapshot(dir.path());
        let mut txn = Txn::begin(dir.path()).unwrap();
        assert!(txn.put("rsrc/linkdir/new.txt", b"evil").is_err());
        assert!(txn.put("rsrc/linkdir/victim.txt", b"evil").is_err());
        // Replacing the link itself is fine and leaves the link's target alone; rollback restores the link.
        txn.put("rsrc/link.txt", b"replaced").unwrap();
        assert_eq!(
            fs::read(outside.path().join("victim.txt")).unwrap(),
            b"precious"
        );
        assert!(!fs::symlink_metadata(dir.path().join("rsrc/link.txt"))
            .unwrap()
            .file_type()
            .is_symlink());
        txn.rollback().unwrap();
        assert_eq!(snapshot(dir.path()), before);
        assert_eq!(
            fs::read(outside.path().join("victim.txt")).unwrap(),
            b"precious"
        );
        assert!(!outside.path().join("new.txt").exists());
    }

    #[test]
    fn every_failure_point_rolls_back_cleanly() {
        let before_dir = game();
        let before = snapshot(before_dir.path());
        // Count the steps of a successful run, then fail each one in turn.
        let total = {
            let dir = game();
            let mut txn = Txn::begin_with_fault(dir.path(), None).unwrap();
            run(&mut txn).unwrap();
            let n = txn.steps();
            txn.commit().unwrap();
            n
        };
        assert!(total >= 8, "{total}");
        for k in 0..total {
            let dir = game();
            let mut txn = Txn::begin_with_fault(dir.path(), Some(k)).unwrap();
            assert!(run(&mut txn).is_err(), "step {k} should fail");
            drop(txn);
            assert_eq!(snapshot(dir.path()), before, "failure at step {k}");
        }
        fn run(txn: &mut Txn) -> io::Result<()> {
            txn.put("rsrc/a/one.txt", b"ONE")?;
            txn.put("rsrc/new/deep/file.txt", b"new")?;
            txn.delete("rsrc/two.txt")?;
            txn.put("rsrc/three.txt", b"3")
        }
    }
}
