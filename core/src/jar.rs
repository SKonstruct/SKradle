//! Safe reading and rewriting of zip and jar archives. Every archive the launcher opens is untrusted input: a mod
//! comes from the internet, and even a game jar may have been edited by something else. Nothing here trusts a name,
//! a size or a link inside an archive.
//!
//! What is enforced when an archive is opened ([`Archive::open`]):
//! - at most [`Limits::max_entries`] entries, each with a declared size of at most [`Limits::max_entry_bytes`] and a
//!   declared total of at most [`Limits::max_total_bytes`] (zip bombs);
//! - every entry name is a safe relative path ([`safe_entry_path`]): no `..`, no absolute path, no drive letter or
//!   `:`, no backslash, no control character, no Windows device name, no trailing dot or space;
//! - no symlink entries, no encrypted entries;
//! - no two entries with the same name (and, for mods, none that differ only in case, because the file systems of
//!   Windows and macOS would silently merge them);
//! - no file that is also the parent folder of another entry.
//!
//! A violation rejects the whole archive, except where a caller asks for [`Limits::skip_unsafe`] (the game's own
//! bundles: entries that cannot be written safely are left out of the index instead). Declared sizes are not
//! believed either: [`Archive::read`] reads at most the declared size plus one byte and fails when the entry turns
//! out to be a different size.
//!
//! [`Jar`] is an archive held in memory for rewriting (`code/config.jar`, `code/projectx-config.jar`): small files
//! only, bounded by the same limits. It is written back through a temporary file and a rename.

use std::collections::HashSet;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Cursor, Read, Seek, Write};
use std::path::{Path, PathBuf};

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipArchive, ZipWriter};

/// Longest entry name accepted, in bytes.
const MAX_NAME_BYTES: usize = 1024;

/// Size caps for one archive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub max_entries: usize,
    /// The most uncompressed bytes one entry may declare.
    pub max_entry_bytes: u64,
    /// The most uncompressed bytes all entries together may declare.
    pub max_total_bytes: u64,
    /// Reject entries whose names differ only in case.
    pub reject_case_collisions: bool,
    /// Leave entries with unsafe names (and symlinks) out instead of rejecting the archive.
    pub skip_unsafe: bool,
}

impl Limits {
    /// A mod archive (the biggest known mod has about 4,000 files).
    pub const MOD: Limits = Limits {
        max_entries: 50_000,
        max_entry_bytes: 256 * 1024 * 1024,
        max_total_bytes: 1024 * 1024 * 1024,
        reject_case_collisions: true,
        skip_unsafe: false,
    };
    /// One of the game's code jars (`config.jar`, `projectx-config.jar`), held in memory.
    pub const CODE_JAR: Limits = Limits {
        max_entries: 20_000,
        max_entry_bytes: 64 * 1024 * 1024,
        max_total_bytes: 128 * 1024 * 1024,
        reject_case_collisions: false,
        skip_unsafe: false,
    };
    /// One of the game's resource bundles (a few hundred MB, read entry by entry, never held in memory).
    pub const BUNDLE: Limits = Limits {
        max_entries: 500_000,
        max_entry_bytes: 1024 * 1024 * 1024,
        max_total_bytes: 16 * 1024 * 1024 * 1024,
        reject_case_collisions: false,
        skip_unsafe: true,
    };
}

/// Why an entry name was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathProblem {
    Empty,
    TooLong,
    /// A backslash, a NUL or another control character.
    BadCharacter,
    Absolute,
    /// A `:` (drive letter, alternate data stream).
    Colon,
    /// An empty, `.` or `..` component.
    BadComponent,
    /// A component that Windows would silently rewrite or treat as a device (`CON`, `a.`, `a `).
    WindowsName,
}

impl fmt::Display for PathProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            PathProblem::Empty => "empty name",
            PathProblem::TooLong => "name too long",
            PathProblem::BadCharacter => "backslash or control character",
            PathProblem::Absolute => "absolute path",
            PathProblem::Colon => "colon in name",
            PathProblem::BadComponent => "empty, `.` or `..` path component",
            PathProblem::WindowsName => "name Windows would rewrite",
        })
    }
}

#[derive(Debug)]
pub enum JarError {
    Io(io::Error),
    /// The zip structure itself is broken (truncated, bad signature, unsupported compression).
    Zip(String),
    TooManyEntries {
        limit: usize,
    },
    EntryTooLarge {
        name: String,
    },
    TotalTooLarge {
        limit: u64,
    },
    UnsafePath {
        name: String,
        problem: PathProblem,
    },
    Symlink {
        name: String,
    },
    Encrypted {
        name: String,
    },
    Duplicate {
        name: String,
    },
    /// An entry is both a file and the folder of another entry.
    FileFolderConflict {
        name: String,
    },
    /// The entry's real size differs from the size its header declares.
    SizeMismatch {
        name: String,
    },
}

impl fmt::Display for JarError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JarError::Io(e) => write!(f, "{e}"),
            JarError::Zip(why) => write!(f, "not a readable zip archive: {why}"),
            JarError::TooManyEntries { limit } => write!(f, "more than {limit} entries"),
            JarError::EntryTooLarge { name } => write!(f, "entry {name:?} is too large"),
            JarError::TotalTooLarge { limit } => {
                write!(f, "the entries add up to more than {limit} bytes")
            }
            JarError::UnsafePath { name, problem } => {
                write!(f, "unsafe entry name {name:?}: {problem}")
            }
            JarError::Symlink { name } => write!(f, "entry {name:?} is a symbolic link"),
            JarError::Encrypted { name } => write!(f, "entry {name:?} is encrypted"),
            JarError::Duplicate { name } => write!(f, "entry {name:?} appears more than once"),
            JarError::FileFolderConflict { name } => {
                write!(f, "entry {name:?} is both a file and a folder")
            }
            JarError::SizeMismatch { name } => {
                write!(f, "entry {name:?} is not the size its header says")
            }
        }
    }
}

impl std::error::Error for JarError {}

impl From<io::Error> for JarError {
    fn from(e: io::Error) -> Self {
        JarError::Io(e)
    }
}

impl From<zip::result::ZipError> for JarError {
    fn from(e: zip::result::ZipError) -> Self {
        match e {
            zip::result::ZipError::Io(io) => JarError::Io(io),
            other => JarError::Zip(other.to_string()),
        }
    }
}

/// Checks an archive entry name and returns it without a trailing `/`, plus whether it names a folder.
///
/// Names use `/` only. The checks are the same on every platform on purpose: a mod that is dangerous on Windows is
/// not accepted on Linux either.
pub fn safe_entry_path(name: &str) -> Result<(String, bool), PathProblem> {
    if name.is_empty() {
        return Err(PathProblem::Empty);
    }
    if name.len() > MAX_NAME_BYTES {
        return Err(PathProblem::TooLong);
    }
    if name.chars().any(|c| c == '\\' || c.is_control()) {
        return Err(PathProblem::BadCharacter);
    }
    if name.starts_with('/') {
        return Err(PathProblem::Absolute);
    }
    let (body, is_dir) = match name.strip_suffix('/') {
        Some(body) => (body, true),
        None => (name, false),
    };
    if body.is_empty() {
        return Err(PathProblem::Absolute);
    }
    for part in body.split('/') {
        if part.is_empty() || part == "." || part == ".." {
            return Err(PathProblem::BadComponent);
        }
        if part.contains(':') {
            return Err(PathProblem::Colon);
        }
        if part.ends_with('.') || part.ends_with(' ') || is_windows_device(part) {
            return Err(PathProblem::WindowsName);
        }
    }
    Ok((body.to_owned(), is_dir))
}

fn is_windows_device(part: &str) -> bool {
    let stem = part.split('.').next().unwrap_or("").to_ascii_lowercase();
    matches!(stem.as_str(), "con" | "prn" | "aux" | "nul")
        || ["com", "lpt"].iter().any(|p| {
            stem.strip_prefix(p)
                .is_some_and(|n| n.len() == 1 && n.as_bytes()[0].is_ascii_digit() && n != "0")
        })
}

/// One entry of an opened archive, after the safety checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryMeta {
    /// Position in the archive (what [`Archive::read`] takes).
    pub index: usize,
    /// The validated name, without a trailing slash.
    pub path: String,
    pub is_dir: bool,
    /// The size the header declares (verified when the entry is read).
    pub size: u64,
}

/// A zip archive whose entries were checked when it was opened.
pub struct Archive<R: Read + Seek> {
    zip: ZipArchive<R>,
    entries: Vec<EntryMeta>,
    /// Entries left out because [`Limits::skip_unsafe`] was set.
    skipped: usize,
}

impl<R: Read + Seek> Archive<R> {
    pub fn open(reader: R, limits: &Limits) -> Result<Self, JarError> {
        let mut zip = ZipArchive::new(reader)?;
        if zip.len() > limits.max_entries {
            return Err(JarError::TooManyEntries {
                limit: limits.max_entries,
            });
        }
        // The crate refuses to read any entry of an archive whose central directory repeats a name, so say why.
        if zip.has_duplicate_names() {
            return Err(JarError::Duplicate {
                name: "(central directory)".to_owned(),
            });
        }
        let mut entries = Vec::with_capacity(zip.len());
        let mut skipped = 0;
        let mut total = 0u64;
        let mut seen = HashSet::new();
        for index in 0..zip.len() {
            let file = zip.by_index_raw(index)?;
            let raw_name = file.name().map(|n| n.into_owned()).unwrap_or_default();
            let size = file.size();
            let is_link = file
                .unix_mode()
                .is_some_and(|mode| mode & 0o170000 == 0o120000);
            let unsafe_entry: Option<JarError> = if is_link {
                Some(JarError::Symlink {
                    name: raw_name.clone(),
                })
            } else {
                safe_entry_path(&raw_name)
                    .err()
                    .map(|problem| JarError::UnsafePath {
                        name: raw_name.clone(),
                        problem,
                    })
            };
            if let Some(err) = unsafe_entry {
                if limits.skip_unsafe {
                    skipped += 1;
                    continue;
                }
                return Err(err);
            }
            if file.encrypted() {
                return Err(JarError::Encrypted { name: raw_name });
            }
            let (path, is_dir) = safe_entry_path(&raw_name).expect("checked above");
            if size > limits.max_entry_bytes {
                return Err(JarError::EntryTooLarge { name: path });
            }
            total = total.saturating_add(size);
            if total > limits.max_total_bytes {
                return Err(JarError::TotalTooLarge {
                    limit: limits.max_total_bytes,
                });
            }
            let key = if limits.reject_case_collisions {
                path.to_lowercase()
            } else {
                path.clone()
            };
            // A folder entry and a file entry of one name also collide.
            if !seen.insert(key) {
                return Err(JarError::Duplicate { name: path });
            }
            entries.push(EntryMeta {
                index,
                path,
                is_dir,
                size,
            });
        }
        check_file_folder_conflicts(&entries, limits.reject_case_collisions)?;
        Ok(Self {
            zip,
            entries,
            skipped,
        })
    }

    pub fn entries(&self) -> &[EntryMeta] {
        &self.entries
    }

    /// How many entries were left out under [`Limits::skip_unsafe`].
    pub fn skipped(&self) -> usize {
        self.skipped
    }

    /// The entry with exactly this path.
    pub fn find(&self, path: &str) -> Option<&EntryMeta> {
        self.entries.iter().find(|e| e.path == path)
    }

    /// Reads an entry into memory, at most its declared size.
    pub fn read(&mut self, index: usize) -> Result<Vec<u8>, JarError> {
        let mut out = Vec::new();
        self.copy_to(index, &mut out)?;
        Ok(out)
    }

    /// Streams an entry into `out`. Fails if the entry is not exactly the size it declares.
    pub fn copy_to(&mut self, index: usize, out: &mut dyn Write) -> Result<u64, JarError> {
        let meta = self
            .entries
            .iter()
            .find(|e| e.index == index)
            .ok_or_else(|| JarError::Zip(format!("no entry {index}")))?
            .clone();
        let mut file = self.zip.by_index(index)?;
        let mut reader = (&mut file).take(meta.size + 1);
        let mut buf = [0u8; 64 * 1024];
        let mut done = 0u64;
        loop {
            let n = reader.read(&mut buf).map_err(|e| {
                // The crate reports a failed checksum or a truncated stream as an io error.
                JarError::Zip(e.to_string())
            })?;
            if n == 0 {
                break;
            }
            done += n as u64;
            if done > meta.size {
                return Err(JarError::SizeMismatch { name: meta.path });
            }
            out.write_all(&buf[..n])?;
        }
        if done != meta.size {
            return Err(JarError::SizeMismatch { name: meta.path });
        }
        Ok(done)
    }
}

impl Archive<File> {
    pub fn open_path(path: &Path, limits: &Limits) -> Result<Self, JarError> {
        Self::open(File::open(path)?, limits)
    }
}

impl Archive<Cursor<Vec<u8>>> {
    pub fn from_bytes(bytes: Vec<u8>, limits: &Limits) -> Result<Self, JarError> {
        Self::open(Cursor::new(bytes), limits)
    }
}

fn check_file_folder_conflicts(entries: &[EntryMeta], fold_case: bool) -> Result<(), JarError> {
    let key = |p: &str| {
        if fold_case {
            p.to_lowercase()
        } else {
            p.to_owned()
        }
    };
    let files: HashSet<String> = entries
        .iter()
        .filter(|e| !e.is_dir)
        .map(|e| key(&e.path))
        .collect();
    for entry in entries {
        let mut prefix = String::new();
        let parts: Vec<&str> = entry.path.split('/').collect();
        for part in &parts[..parts.len() - 1] {
            if !prefix.is_empty() {
                prefix.push('/');
            }
            prefix.push_str(part);
            if files.contains(&key(&prefix)) {
                return Err(JarError::FileFolderConflict { name: prefix });
            }
        }
    }
    Ok(())
}

/// One entry of a [`Jar`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JarEntry {
    pub path: String,
    pub is_dir: bool,
    pub data: Vec<u8>,
}

/// A jar held in memory so entries can be replaced, added or removed. Entry order is kept; new entries go last, and
/// `META-INF/MANIFEST.MF` is written first as jars conventionally have it. Written archives use a fixed timestamp,
/// so the same content always gives the same bytes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Jar {
    entries: Vec<JarEntry>,
}

impl Jar {
    pub fn read<R: Read + Seek>(reader: R, limits: &Limits) -> Result<Self, JarError> {
        let mut archive = Archive::open(reader, limits)?;
        let metas: Vec<EntryMeta> = archive.entries().to_vec();
        let mut entries = Vec::with_capacity(metas.len());
        for meta in metas {
            let data = if meta.is_dir {
                Vec::new()
            } else {
                archive.read(meta.index)?
            };
            entries.push(JarEntry {
                path: meta.path,
                is_dir: meta.is_dir,
                data,
            });
        }
        Ok(Self { entries })
    }

    pub fn read_path(path: &Path, limits: &Limits) -> Result<Self, JarError> {
        Self::read(File::open(path)?, limits)
    }

    pub fn from_bytes(bytes: &[u8], limits: &Limits) -> Result<Self, JarError> {
        Self::read(Cursor::new(bytes), limits)
    }

    pub fn entries(&self) -> &[JarEntry] {
        &self.entries
    }

    pub fn get(&self, path: &str) -> Option<&[u8]> {
        self.entries
            .iter()
            .find(|e| !e.is_dir && e.path == path)
            .map(|e| e.data.as_slice())
    }

    pub fn contains(&self, path: &str) -> bool {
        self.get(path).is_some()
    }

    /// Replaces the file at `path` or adds it. The name is checked like an archive entry name.
    pub fn set(&mut self, path: &str, data: Vec<u8>) -> Result<(), JarError> {
        let (path, is_dir) = safe_entry_path(path).map_err(|problem| JarError::UnsafePath {
            name: path.to_owned(),
            problem,
        })?;
        if is_dir {
            return Err(JarError::UnsafePath {
                name: path,
                problem: PathProblem::BadComponent,
            });
        }
        // A file may not share its name with a folder, sit inside a file, or hold other entries: the archive
        // would be rejected when read back.
        let inside = format!("{path}/");
        let conflict = self.entries.iter().find(|e| {
            e.path.starts_with(&inside) || (e.is_dir && e.path == path) || {
                // `e` is a file that is a folder on the way to `path`.
                !e.is_dir && path.starts_with(&format!("{}/", e.path))
            }
        });
        if let Some(e) = conflict {
            return Err(JarError::FileFolderConflict {
                name: e.path.clone(),
            });
        }
        match self
            .entries
            .iter_mut()
            .find(|e| !e.is_dir && e.path == path)
        {
            Some(entry) => entry.data = data,
            None => self.entries.push(JarEntry {
                path,
                is_dir: false,
                data,
            }),
        }
        Ok(())
    }

    /// Removes the file at `path`; true if there was one.
    pub fn remove(&mut self, path: &str) -> bool {
        let before = self.entries.len();
        self.entries.retain(|e| e.is_dir || e.path != path);
        self.entries.len() != before
    }

    pub fn write<W: Write + Seek>(&self, out: W) -> Result<W, JarError> {
        let mut zip = ZipWriter::new(out);
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .last_modified_time(DateTime::default());
        let manifest = self
            .entries
            .iter()
            .filter(|e| !e.is_dir && e.path == "META-INF/MANIFEST.MF");
        let rest = self
            .entries
            .iter()
            .filter(|e| e.is_dir || e.path != "META-INF/MANIFEST.MF");
        for entry in manifest.chain(rest) {
            if entry.is_dir {
                zip.add_directory(format!("{}/", entry.path), options)?;
            } else {
                zip.start_file(entry.path.as_str(), options)?;
                zip.write_all(&entry.data)?;
            }
        }
        Ok(zip.finish()?)
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, JarError> {
        Ok(self.write(Cursor::new(Vec::new()))?.into_inner())
    }

    /// Writes the jar to `dest` through a temporary file next to it and a rename, so `dest` is either the old file
    /// or the complete new one.
    pub fn write_atomic(&self, dest: &Path) -> Result<(), JarError> {
        write_atomic(dest, &self.to_bytes()?)?;
        Ok(())
    }
}

/// Writes `bytes` to `<dest>.skradle-tmp`, syncs it and renames it over `dest`. A leftover temporary file (or a link
/// placed there) is removed first, never followed.
pub fn write_atomic(dest: &Path, bytes: &[u8]) -> io::Result<()> {
    let tmp = temp_path(dest);
    match fs::remove_file(&tmp) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
        _ => {}
    }
    let result = (|| {
        let mut file = OpenOptions::new().write(true).create_new(true).open(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&tmp, dest)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

/// `<dest>.skradle-tmp`, the temporary name used beside a file being replaced.
pub fn temp_path(dest: &Path) -> PathBuf {
    let mut name = dest
        .file_name()
        .map(|n| n.to_os_string())
        .unwrap_or_default();
    name.push(".skradle-tmp");
    dest.with_file_name(name)
}

#[cfg(test)]
pub(crate) mod testutil {
    use super::*;

    /// Builds a zip in memory. `mode` sets the unix mode bits (a symlink is 0o120777).
    pub fn zip_with(entries: &[(&str, &[u8], Option<u32>)]) -> Vec<u8> {
        let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
        for (name, data, mode) in entries {
            let mut options =
                SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
            if *mode == Some(0o120777) {
                let target = std::str::from_utf8(data).unwrap();
                zip.add_symlink(*name, target, options).unwrap();
                continue;
            }
            if let Some(mode) = mode {
                options = options.unix_permissions(*mode);
            }
            if name.ends_with('/') {
                zip.add_directory(*name, options).unwrap();
            } else {
                zip.start_file(*name, options).unwrap();
                zip.write_all(data).unwrap();
            }
        }
        zip.finish().unwrap().into_inner()
    }

    pub fn simple_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let entries: Vec<(&str, &[u8], Option<u32>)> =
            entries.iter().map(|(n, d)| (*n, *d, None)).collect();
        zip_with(&entries)
    }
}

#[cfg(test)]
mod tests {
    use super::testutil::*;
    use super::*;

    fn open(bytes: Vec<u8>) -> Result<Archive<Cursor<Vec<u8>>>, JarError> {
        Archive::from_bytes(bytes, &Limits::MOD)
    }

    #[test]
    fn accepts_ordinary_paths() {
        assert_eq!(
            safe_entry_path("a/b/c.png"),
            Ok(("a/b/c.png".into(), false))
        );
        assert_eq!(safe_entry_path("a/b/"), Ok(("a/b".into(), true)));
        assert_eq!(safe_entry_path("mod.json"), Ok(("mod.json".into(), false)));
        assert_eq!(
            safe_entry_path("RVH README.txt"),
            Ok(("RVH README.txt".into(), false))
        );
        assert!(safe_entry_path("a/.hidden").is_ok());
        assert!(safe_entry_path("config.dat/x").is_ok());
        assert!(
            safe_entry_path("console/x").is_ok(),
            "only whole device names are refused"
        );
        assert!(safe_entry_path("com10.txt").is_ok());
    }

    #[test]
    fn refuses_hostile_paths() {
        let cases = [
            ("", PathProblem::Empty),
            ("/etc/passwd", PathProblem::Absolute),
            ("/", PathProblem::Absolute),
            ("../x", PathProblem::BadComponent),
            ("a/../../x", PathProblem::BadComponent),
            ("a/./b", PathProblem::BadComponent),
            ("a//b", PathProblem::BadComponent),
            ("..", PathProblem::BadComponent),
            ("a\\b", PathProblem::BadCharacter),
            ("..\\x", PathProblem::BadCharacter),
            ("a\0b", PathProblem::BadCharacter),
            ("a\nb", PathProblem::BadCharacter),
            ("C:/x", PathProblem::Colon),
            ("C:x", PathProblem::Colon),
            ("a/b:stream", PathProblem::Colon),
            ("a/CON", PathProblem::WindowsName),
            ("nul.txt", PathProblem::WindowsName),
            ("COM1", PathProblem::WindowsName),
            ("lpt9.log", PathProblem::WindowsName),
            ("a/b.", PathProblem::WindowsName),
            ("a/b ", PathProblem::WindowsName),
        ];
        for (name, expected) in cases {
            assert_eq!(safe_entry_path(name), Err(expected), "{name:?}");
        }
        let long = "a".repeat(MAX_NAME_BYTES + 1);
        assert_eq!(safe_entry_path(&long), Err(PathProblem::TooLong));
    }

    #[test]
    fn opens_a_normal_archive_and_reads_entries() {
        let mut a = open(simple_zip(&[
            ("d/", b""),
            ("d/a.txt", b"hello"),
            ("b.txt", b""),
        ]))
        .unwrap();
        assert_eq!(a.entries().len(), 3);
        let meta = a.find("d/a.txt").unwrap().clone();
        assert_eq!(meta.size, 5);
        assert_eq!(a.read(meta.index).unwrap(), b"hello");
        assert!(a.find("d").unwrap().is_dir);
        assert_eq!(a.skipped(), 0);
    }

    #[test]
    fn rejects_zip_slip_and_absolute_entries() {
        for name in [
            "../evil.txt",
            "a/../../evil.txt",
            "/abs.txt",
            "C:/win.txt",
            "a\\..\\b",
        ] {
            let err = open(simple_zip(&[(name, b"x")])).err().expect(name);
            assert!(matches!(err, JarError::UnsafePath { .. }), "{name}: {err}");
        }
    }

    #[test]
    fn rejects_symlink_entries() {
        let err = open(zip_with(&[("link", b"/etc/passwd", Some(0o120777))]))
            .err()
            .unwrap();
        assert!(matches!(err, JarError::Symlink { .. }), "{err}");
        // A regular file with ordinary permissions is fine.
        assert!(open(zip_with(&[("f", b"x", Some(0o100644))])).is_ok());
    }

    #[test]
    fn duplicate_names_are_rejected() {
        // The writer in this crate refuses duplicates, so patch a second name to equal the first.
        let mut bytes = simple_zip(&[("aa.txt", b"1"), ("bb.txt", b"2")]);
        let needle = b"bb.txt";
        let mut replaced = 0;
        let mut i = 0;
        while i + needle.len() <= bytes.len() {
            if &bytes[i..i + needle.len()] == needle {
                bytes[i..i + needle.len()].copy_from_slice(b"aa.txt");
                replaced += 1;
            }
            i += 1;
        }
        assert_eq!(replaced, 2, "local header and central directory");
        let err = open(bytes).err().unwrap();
        assert!(matches!(err, JarError::Duplicate { .. }), "{err}");
    }

    #[test]
    fn case_collisions_follow_the_limit() {
        let bytes = simple_zip(&[("Item/A.png", b"1"), ("item/a.png", b"2")]);
        let err = Archive::from_bytes(bytes.clone(), &Limits::MOD)
            .err()
            .unwrap();
        assert!(matches!(err, JarError::Duplicate { .. }), "{err}");
        assert!(Archive::from_bytes(bytes, &Limits::CODE_JAR).is_ok());
    }

    #[test]
    fn rejects_a_file_that_is_also_a_folder() {
        let err = open(simple_zip(&[("a", b"file"), ("a/b.txt", b"x")]))
            .err()
            .unwrap();
        assert!(matches!(err, JarError::FileFolderConflict { .. }), "{err}");
    }

    #[test]
    fn enforces_the_size_and_count_caps() {
        let bytes = simple_zip(&[("a", &[0u8; 100]), ("b", &[0u8; 100])]);
        let small_entry = Limits {
            max_entry_bytes: 99,
            ..Limits::MOD
        };
        assert!(matches!(
            Archive::from_bytes(bytes.clone(), &small_entry)
                .err()
                .unwrap(),
            JarError::EntryTooLarge { .. }
        ));
        let small_total = Limits {
            max_total_bytes: 150,
            ..Limits::MOD
        };
        assert!(matches!(
            Archive::from_bytes(bytes.clone(), &small_total)
                .err()
                .unwrap(),
            JarError::TotalTooLarge { .. }
        ));
        let few = Limits {
            max_entries: 1,
            ..Limits::MOD
        };
        assert!(matches!(
            Archive::from_bytes(bytes, &few).err().unwrap(),
            JarError::TooManyEntries { .. }
        ));
    }

    #[test]
    fn a_header_that_understates_the_size_is_caught_on_read() {
        // Highly compressible data, then patch both size fields down so the stream is bigger than declared.
        let mut bytes = simple_zip(&[("big.bin", &vec![0u8; 4000])]);
        let sig_local = [0x50, 0x4b, 0x03, 0x04];
        let sig_central = [0x50, 0x4b, 0x01, 0x02];
        let local = bytes.windows(4).position(|w| w == sig_local).unwrap();
        let central = bytes.windows(4).position(|w| w == sig_central).unwrap();
        // Uncompressed size: local header offset 22, central directory offset 24.
        bytes[local + 22..local + 26].copy_from_slice(&10u32.to_le_bytes());
        bytes[central + 24..central + 28].copy_from_slice(&10u32.to_le_bytes());
        let mut a = open(bytes).unwrap();
        let idx = a.entries()[0].index;
        let err = a.read(idx).unwrap_err();
        assert!(
            matches!(err, JarError::SizeMismatch { .. } | JarError::Zip(_)),
            "{err}"
        );
    }

    #[test]
    fn truncated_and_garbage_input_is_an_error_not_a_panic() {
        let good = simple_zip(&[("a.txt", b"hello world"), ("b.txt", b"more")]);
        for cut in [0, 1, 10, good.len() / 2, good.len() - 1] {
            assert!(open(good[..cut].to_vec()).is_err(), "cut at {cut}");
        }
        assert!(open(b"this is not a zip file at all".to_vec()).is_err());
        assert!(open(vec![0xff; 4096]).is_err());
    }

    #[test]
    fn corrupted_data_fails_the_read() {
        let mut bytes = simple_zip(&[("a.txt", b"hello hello hello hello hello")]);
        let local = bytes
            .windows(4)
            .position(|w| w == [0x50, 0x4b, 0x03, 0x04])
            .unwrap();
        let data_at = local + 30 + "a.txt".len();
        bytes[data_at + 2] ^= 0xff;
        if let Ok(mut a) = open(bytes) {
            let idx = a.entries()[0].index;
            assert!(a.read(idx).is_err());
        }
    }

    #[test]
    fn unsafe_entries_can_be_skipped_for_game_bundles() {
        let bytes = simple_zip(&[("ok.txt", b"1"), ("../bad.txt", b"2"), ("fine/x", b"3")]);
        let a = Archive::from_bytes(bytes, &Limits::BUNDLE).unwrap();
        assert_eq!(a.entries().len(), 2);
        assert_eq!(a.skipped(), 1);
    }

    #[test]
    fn jar_edits_and_round_trips() {
        let source = simple_zip(&[
            ("a.txt", b"A"),
            ("META-INF/MANIFEST.MF", b"M"),
            ("dir/", b""),
            ("dir/b.txt", b"B"),
        ]);
        let mut jar = Jar::from_bytes(&source, &Limits::CODE_JAR).unwrap();
        assert_eq!(jar.get("a.txt"), Some(&b"A"[..]));
        jar.set("a.txt", b"A2".to_vec()).unwrap();
        jar.set("new/c.txt", b"C".to_vec()).unwrap();
        assert!(jar.remove("dir/b.txt"));
        assert!(!jar.remove("dir/b.txt"));
        let bytes = jar.to_bytes().unwrap();
        let again = Jar::from_bytes(&bytes, &Limits::CODE_JAR).unwrap();
        let sorted = |j: &Jar| {
            let mut v = j.entries().to_vec();
            v.sort_by(|a, b| a.path.cmp(&b.path));
            v
        };
        assert_eq!(sorted(&again), sorted(&jar));
        assert_eq!(
            again.entries()[0].path,
            "META-INF/MANIFEST.MF",
            "manifest first"
        );
        assert_eq!(again.get("a.txt"), Some(&b"A2"[..]));
        assert_eq!(again.get("new/c.txt"), Some(&b"C"[..]));
        assert!(again.entries().iter().any(|e| e.is_dir && e.path == "dir"));
        assert_eq!(jar.to_bytes().unwrap(), bytes, "deterministic output");
    }

    #[test]
    fn jar_set_refuses_names_that_clash_with_files_and_folders() {
        let source = simple_zip(&[("f", b"F"), ("dir/", b""), ("deep/x/y.txt", b"Y")]);
        let mut jar = Jar::from_bytes(&source, &Limits::CODE_JAR).unwrap();
        let before = jar.clone();
        for name in ["dir", "deep", "deep/x", "f/inner", "deep/x/y.txt/z"] {
            let err = jar.set(name, vec![1]).unwrap_err();
            assert!(
                matches!(err, JarError::FileFolderConflict { .. }),
                "{name}: {err}"
            );
        }
        assert_eq!(jar, before, "a refused set changes nothing");
        // Siblings and new folders are fine, and what is written can be read back.
        jar.set("dir/ok.txt", vec![1]).unwrap();
        jar.set("deep/x/z.txt", vec![2]).unwrap();
        jar.set("f2", vec![3]).unwrap();
        Jar::from_bytes(&jar.to_bytes().unwrap(), &Limits::CODE_JAR).unwrap();
    }

    #[test]
    fn jar_set_refuses_unsafe_names() {
        let mut jar = Jar::default();
        for name in ["../x", "/x", "a\\b", "dir/", ""] {
            assert!(jar.set(name, vec![1]).is_err(), "{name:?}");
        }
        assert!(jar.entries().is_empty());
    }

    #[test]
    fn atomic_write_replaces_the_file_and_leaves_no_temp() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("x.jar");
        fs::write(&dest, b"old").unwrap();
        let mut jar = Jar::default();
        jar.set("a", b"1".to_vec()).unwrap();
        jar.write_atomic(&dest).unwrap();
        assert!(Jar::read_path(&dest, &Limits::CODE_JAR)
            .unwrap()
            .contains("a"));
        assert!(!temp_path(&dest).exists());
    }

    #[cfg(unix)]
    #[test]
    fn atomic_write_does_not_follow_a_planted_temp_link() {
        let dir = tempfile::tempdir().unwrap();
        let victim = dir.path().join("victim");
        fs::write(&victim, b"precious").unwrap();
        let dest = dir.path().join("x.bin");
        std::os::unix::fs::symlink(&victim, temp_path(&dest)).unwrap();
        write_atomic(&dest, b"new").unwrap();
        assert_eq!(fs::read(&victim).unwrap(), b"precious");
        assert_eq!(fs::read(&dest).unwrap(), b"new");
    }
}
