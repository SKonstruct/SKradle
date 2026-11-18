//! Small file helpers shared by the server-host modules: atomic writes and tree copies that refuse symlinks.

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// The sibling path `<name>.<suffix>`, for temporary and retired files.
pub(crate) fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path
        .file_name()
        .map(|n| n.to_os_string())
        .unwrap_or_default();
    name.push(".");
    name.push(suffix);
    path.with_file_name(name)
}

/// Writes `bytes` to `path` through a temporary sibling that is synced and then renamed over it, so a crash leaves
/// either the old file or the new one, never half of one.
pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = sibling(path, "tmp");
    let result = (|| {
        let mut file = File::create(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

/// Removes a file or a whole directory tree; a missing path is fine. Symlinks are removed, never followed.
pub(crate) fn remove_all(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() => fs::remove_dir_all(path),
        Ok(_) => fs::remove_file(path),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

/// Copies the directory tree `from` into `to` (created). Regular files and directories only: a symlink or another
/// special file inside the tree is an error, so a copy never reaches outside the tree. Files for which `skip`
/// returns true (by file name) are left out. A missing `from` copies nothing. Returns the bytes copied.
pub(crate) fn copy_tree(from: &Path, to: &Path, skip: &dyn Fn(&str) -> bool) -> io::Result<u64> {
    match fs::symlink_metadata(from) {
        Ok(meta) if meta.is_dir() => {}
        Ok(_) => {
            return Err(io::Error::other(format!(
                "{} is not a folder",
                from.display()
            )))
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(0),
        Err(e) => return Err(e),
    }
    fs::create_dir_all(to)?;
    let mut bytes = 0;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let name = entry.file_name();
        let name_text = name.to_string_lossy();
        let kind = entry.file_type()?;
        let target = to.join(&name);
        if kind.is_dir() {
            bytes += copy_tree(&entry.path(), &target, skip)?;
        } else if kind.is_file() {
            if skip(&name_text) {
                continue;
            }
            bytes += fs::copy(entry.path(), &target)?;
        } else {
            return Err(io::Error::other(format!(
                "{} is a link or special file; refusing to copy it",
                entry.path().display()
            )));
        }
    }
    Ok(bytes)
}

/// The total size in bytes of the regular files under `path` (0 when missing).
pub(crate) fn tree_size(path: &Path) -> u64 {
    let Ok(meta) = fs::symlink_metadata(path) else {
        return 0;
    };
    if meta.is_file() {
        return meta.len();
    }
    if !meta.is_dir() {
        return 0;
    }
    fs::read_dir(path)
        .map(|entries| entries.flatten().map(|e| tree_size(&e.path())).sum::<u64>())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_write_replaces_and_leaves_no_temporary() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("a/b.txt");
        write_atomic(&file, b"one").unwrap();
        write_atomic(&file, b"two").unwrap();
        assert_eq!(fs::read(&file).unwrap(), b"two");
        assert!(!sibling(&file, "tmp").exists());
    }

    #[test]
    fn copy_tree_copies_files_and_honours_skip() {
        let dir = tempfile::tempdir().unwrap();
        let from = dir.path().join("from");
        fs::create_dir_all(from.join("sub")).unwrap();
        fs::write(from.join("a"), b"12345").unwrap();
        fs::write(from.join("sub/b"), b"123").unwrap();
        fs::write(from.join("server.lock"), b"x").unwrap();
        let to = dir.path().join("to");
        let n = copy_tree(&from, &to, &|name| name == "server.lock").unwrap();
        assert_eq!(n, 8);
        assert!(to.join("sub/b").is_file());
        assert!(!to.join("server.lock").exists());
        assert_eq!(
            copy_tree(&dir.path().join("none"), &to, &|_| false).unwrap(),
            0
        );
    }

    #[cfg(unix)]
    #[test]
    fn copy_tree_refuses_symlinks() {
        let dir = tempfile::tempdir().unwrap();
        let from = dir.path().join("from");
        fs::create_dir_all(&from).unwrap();
        std::os::unix::fs::symlink("/etc/passwd", from.join("link")).unwrap();
        assert!(copy_tree(&from, &dir.path().join("to"), &|_| false).is_err());
    }

    #[test]
    fn remove_all_handles_missing_files_and_trees() {
        let dir = tempfile::tempdir().unwrap();
        remove_all(&dir.path().join("none")).unwrap();
        fs::create_dir_all(dir.path().join("t/u")).unwrap();
        fs::write(dir.path().join("t/u/f"), b"x").unwrap();
        remove_all(&dir.path().join("t")).unwrap();
        assert!(!dir.path().join("t").exists());
    }
}
