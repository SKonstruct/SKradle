//! Reading a small file of the launcher's own (a state file, a manifest, a digest list) without trusting its size
//! or what it is: the path must be a regular file (not a link to somewhere else, not a folder or a device), and at
//! most `limit` bytes are ever read, even when the file grows while it is being read.

use std::fs::{self, File};
use std::io::{self, Read};
use std::path::Path;

/// The bytes of the regular file at `path`, or `InvalidData` when it is not a regular file or longer than `limit`.
/// A missing file keeps its `NotFound` error so callers can treat it as empty.
pub fn read_regular(path: &Path, limit: u64) -> io::Result<Vec<u8>> {
    if !fs::symlink_metadata(path)?.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "not a regular file",
        ));
    }
    let mut bytes = Vec::new();
    File::open(path)?
        .take(limit.saturating_add(1))
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("larger than {limit} bytes"),
        ));
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_small_regular_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a");
        fs::write(&path, b"hello").unwrap();
        assert_eq!(read_regular(&path, 5).unwrap(), b"hello");
    }

    #[test]
    fn a_file_over_the_limit_is_refused_after_reading_at_most_one_byte_more() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a");
        fs::write(&path, vec![b'x'; 100]).unwrap();
        let error = read_regular(&path, 99).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(read_regular(&path, 100).is_ok());
    }

    #[test]
    fn a_missing_file_stays_not_found_and_a_folder_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            read_regular(&dir.path().join("no"), 10).unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
        assert_eq!(
            read_regular(dir.path(), 10).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_link_is_not_followed() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("target");
        fs::write(&target, b"secret").unwrap();
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert_eq!(
            read_regular(&link, 100).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
    }
}
