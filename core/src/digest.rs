//! The game updater's `digest2.txt`: which SHA-256 every file of the install must have.
//!
//! Format, checked against a real install: `path = sha256` lines (`code/config.jar = 1e3f...`), the last line is
//! `digest2.txt = <sha256 of every byte before that line>`. Paths are relative to the game folder and use `/`.
//!
//! The one thing that is not obvious: for a `.jar` the digest is **not** the hash of the file's bytes. It is the
//! SHA-256 over the uncompressed contents of every entry, taken in entry-name order (names compared as UTF-16 code
//! units, as Java sorts strings), with the names themselves, folder entries and the zip layout left out. Rewriting a
//! jar with different compression or entry order therefore gives the same digest, while changing any entry's
//! contents (or adding or removing a file entry) changes it. Every other file is hashed by its bytes. All 32
//! unmodified jars of a real install verify this way (a 33rd had been edited by another tool) and none verifies by
//! file bytes.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{Read, Seek};
use std::path::Path;

use sha2::{Digest, Sha256};

use crate::jar::{JarError, Limits};
use crate::net::{hex, sha256_file};

/// A parsed `digest2.txt`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Digests {
    map: BTreeMap<String, String>,
}

/// Longest `digest2.txt` accepted (the real one is 4 KB).
pub const MAX_DIGEST_FILE_BYTES: u64 = 1024 * 1024;

impl Digests {
    /// Reads the `path = sha256` lines. Lines that are not that (comments, blank lines, a malformed hash, a path with
    /// `..`) are ignored, so a damaged file yields fewer entries and the missing ones then count as unknown.
    pub fn parse(text: &str) -> Self {
        let mut map = BTreeMap::new();
        for line in text.lines() {
            let Some((path, hash)) = line.split_once(" = ") else {
                continue;
            };
            let (path, hash) = (path.trim(), hash.trim());
            if path.is_empty()
                || path.starts_with('#')
                || path.starts_with('/')
                || path.contains('\\')
                || path
                    .split('/')
                    .any(|p| p.is_empty() || p == "." || p == "..")
                || hash.len() != 64
                || !hash.bytes().all(|b| b.is_ascii_hexdigit())
            {
                continue;
            }
            // The first line for a path wins, as with the updater's own lookup of the first match.
            map.entry(path.to_owned())
                .or_insert_with(|| hash.to_ascii_lowercase());
        }
        Self { map }
    }

    pub fn read(path: &Path) -> std::io::Result<Self> {
        let bytes = crate::bounded::read_regular(path, MAX_DIGEST_FILE_BYTES)?;
        let text = String::from_utf8(bytes).map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "digest2.txt is not valid text",
            )
        })?;
        Ok(Self::parse(&text))
    }

    /// The expected SHA-256 (lower-case hex) of the file at `path`.
    pub fn get(&self, path: &str) -> Option<&str> {
        self.map.get(path).map(String::as_str)
    }

    pub fn paths(&self) -> impl Iterator<Item = &str> {
        self.map.keys().map(String::as_str)
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Compares the file `rel` under `root` with its digest.
    pub fn verify(&self, root: &Path, rel: &str) -> Verdict {
        let Some(expected) = self.get(rel) else {
            return Verdict::NoDigest;
        };
        let path = root.join(rel);
        // Never read through a link: a link inside the game folder is not the updater's file.
        match std::fs::symlink_metadata(&path) {
            Ok(m) if m.is_file() => {}
            Ok(_) => return Verdict::Unreadable("not a regular file".to_owned()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Verdict::Missing,
            Err(e) => return Verdict::Unreadable(e.to_string()),
        }
        let actual = if rel.ends_with(".jar") {
            match File::open(&path)
                .map_err(JarError::from)
                .and_then(|f| jar_digest(f, &Limits::BUNDLE))
            {
                Ok(d) => d,
                // A jar that cannot be read as a zip is simply not the game's.
                Err(e) => return Verdict::Unreadable(e.to_string()),
            }
        } else {
            match sha256_file(&path) {
                Ok(d) => d,
                Err(e) => return Verdict::Unreadable(e.to_string()),
            }
        };
        if actual == expected {
            Verdict::Match
        } else {
            Verdict::Mismatch { actual }
        }
    }
}

/// Whether the last line of `digest2.txt` (`digest2.txt = <hash of everything before it>`) holds, which catches a
/// truncated or hand-edited file. The text must be the file's exact bytes. (`Digests::verify` on `digest2.txt`
/// itself reports a mismatch by design: that line is not a hash of the whole file.)
pub fn verify_self(text: &str) -> bool {
    let Some(at) = text.rfind("digest2.txt = ") else {
        return false;
    };
    if at != 0 && !text[..at].ends_with('\n') {
        return false;
    }
    let claimed = text[at + "digest2.txt = ".len()..].trim();
    crate::net::sha256_hex(&text.as_bytes()[..at]) == claimed
}

/// How a file compares with its digest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Match,
    Mismatch {
        actual: String,
    },
    /// `digest2.txt` has no line for this path.
    NoDigest,
    Missing,
    /// Not a regular file, or a jar that is not a readable zip.
    Unreadable(String),
}

impl Verdict {
    pub fn is_match(&self) -> bool {
        matches!(self, Verdict::Match)
    }
}

/// The updater's digest of a jar: SHA-256 over every entry's uncompressed bytes, entries ordered by name (UTF-16
/// code units). Entry names are not checked for safety here (nothing is extracted), but the entry-count and size caps
/// of `limits` apply, and an entry that does not read cleanly (bad checksum, truncated) is an error.
pub fn jar_digest<R: Read + Seek>(reader: R, limits: &Limits) -> Result<String, JarError> {
    let mut zip = zip::ZipArchive::new(reader)?;
    if zip.len() > limits.max_entries {
        return Err(JarError::TooManyEntries {
            limit: limits.max_entries,
        });
    }
    let mut names: Vec<(String, usize)> = Vec::with_capacity(zip.len());
    let mut total = 0u64;
    for index in 0..zip.len() {
        let file = zip.by_index_raw(index)?;
        if file.size() > limits.max_entry_bytes {
            return Err(JarError::EntryTooLarge {
                name: String::from_utf8_lossy(file.name_raw()).into_owned(),
            });
        }
        total = total.saturating_add(file.size());
        if total > limits.max_total_bytes {
            return Err(JarError::TotalTooLarge {
                limit: limits.max_total_bytes,
            });
        }
        names.push((String::from_utf8_lossy(file.name_raw()).into_owned(), index));
    }
    names.sort_by(|a, b| a.0.encode_utf16().cmp(b.0.encode_utf16()));
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    for (name, index) in names {
        let mut file = zip.by_index(index)?;
        let declared = file.size();
        let mut seen = 0u64;
        let mut reader = (&mut file).take(declared.saturating_add(1));
        loop {
            let n = reader
                .read(&mut buf)
                .map_err(|e| JarError::Zip(e.to_string()))?;
            if n == 0 {
                break;
            }
            seen += n as u64;
            if seen > declared {
                return Err(JarError::SizeMismatch { name });
            }
            hasher.update(&buf[..n]);
        }
        if seen != declared {
            return Err(JarError::SizeMismatch { name });
        }
    }
    Ok(hex(hasher.finalize().as_slice()))
}

/// [`jar_digest`] of in-memory jar bytes.
pub fn jar_digest_bytes(bytes: &[u8], limits: &Limits) -> Result<String, JarError> {
    jar_digest(std::io::Cursor::new(bytes), limits)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jar::testutil::simple_zip;
    use crate::net::sha256_hex;

    fn expected(parts: &[&[u8]]) -> String {
        let mut h = Sha256::new();
        for p in parts {
            h.update(p);
        }
        hex(h.finalize().as_slice())
    }

    #[test]
    fn jar_digest_hashes_contents_in_name_order_without_names() {
        // Written out of order and with a folder entry; META-INF counts like any other entry.
        let jar = simple_zip(&[
            ("b.txt", b"B"),
            ("META-INF/MANIFEST.MF", b"M"),
            ("d/", b""),
            ("a.txt", b"A"),
        ]);
        // Order: META-INF/MANIFEST.MF, a.txt, b.txt, then folder d/ (empty).
        let want = expected(&[b"M", b"A", b"B"]);
        assert_eq!(jar_digest_bytes(&jar, &Limits::BUNDLE).unwrap(), want);
        // Not the hash of the file's bytes.
        assert_ne!(sha256_hex(&jar), want);
    }

    #[test]
    fn jar_digest_ignores_layout_but_not_contents() {
        let one = simple_zip(&[("a", b"1"), ("b", b"2")]);
        let reordered = simple_zip(&[("b", b"2"), ("a", b"1")]);
        let changed = simple_zip(&[("a", b"1"), ("b", b"3")]);
        let d = |z: &[u8]| jar_digest_bytes(z, &Limits::BUNDLE).unwrap();
        assert_eq!(d(&one), d(&reordered));
        assert_ne!(d(&one), d(&changed));
    }

    #[test]
    fn sorts_names_as_utf16_like_java() {
        // U+FF5E (BMP, above the surrogate range) sorts after U+1F600 in UTF-16 but before it in UTF-8.
        let jar = simple_zip(&[("\u{ff5e}", b"X"), ("\u{1f600}", b"Y")]);
        assert_eq!(
            jar_digest_bytes(&jar, &Limits::BUNDLE).unwrap(),
            expected(&[b"Y", b"X"])
        );
    }

    #[test]
    fn jar_digest_rejects_broken_and_oversized_jars() {
        assert!(jar_digest_bytes(b"not a zip", &Limits::BUNDLE).is_err());
        let jar = simple_zip(&[("a", &[0u8; 100])]);
        let small = Limits {
            max_entry_bytes: 10,
            ..Limits::BUNDLE
        };
        assert!(matches!(
            jar_digest_bytes(&jar, &small),
            Err(JarError::EntryTooLarge { .. })
        ));
        assert!(jar_digest_bytes(&jar[..jar.len() / 2], &Limits::BUNDLE).is_err());
    }

    #[test]
    fn parses_digest_lines_and_ignores_junk() {
        let h = "ab".repeat(32);
        let text = format!(
            "# comment\ngetdown.txt = {h}\ncode/config.jar = {}\nbad = xyz\n../evil = {h}\n/abs = {h}\n\
             a//b = {h}\ncode\\x = {h}\nnohash\ncode/config.jar = {h}\n",
            "CD".repeat(32)
        );
        let d = Digests::parse(&text);
        assert_eq!(d.get("getdown.txt"), Some(h.as_str()));
        assert_eq!(
            d.get("code/config.jar"),
            Some("cd".repeat(32).as_str()),
            "first line wins, hash lower-cased"
        );
        assert_eq!(d.len(), 2);
        assert!(d.get("../evil").is_none());
        assert!(Digests::parse("").is_empty());
    }

    #[test]
    fn verify_self_detects_truncation_and_edits() {
        let body = format!("a = {}\nb = {}\n", "11".repeat(32), "22".repeat(32));
        let text = format!("{body}digest2.txt = {}\n", sha256_hex(body.as_bytes()));
        assert!(verify_self(&text));
        assert!(!verify_self(&text.replace("11", "12")));
        assert!(!verify_self(&body));
        assert!(!verify_self(&text[..text.len() - 10]));
    }

    #[test]
    fn verifies_files_in_a_folder() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("code")).unwrap();
        let jar = simple_zip(&[("x", b"1")]);
        std::fs::write(dir.path().join("code/a.jar"), &jar).unwrap();
        std::fs::write(dir.path().join("plain.txt"), b"hello").unwrap();
        std::fs::write(dir.path().join("code/bad.jar"), b"junk").unwrap();
        let text = format!(
            "code/a.jar = {}\nplain.txt = {}\ncode/gone.jar = {}\ncode/bad.jar = {}\n",
            jar_digest_bytes(&jar, &Limits::BUNDLE).unwrap(),
            sha256_hex(b"hello"),
            "00".repeat(32),
            "00".repeat(32),
        );
        let d = Digests::parse(&text);
        assert!(d.verify(dir.path(), "code/a.jar").is_match());
        assert!(d.verify(dir.path(), "plain.txt").is_match());
        assert_eq!(d.verify(dir.path(), "code/gone.jar"), Verdict::Missing);
        assert_eq!(d.verify(dir.path(), "other"), Verdict::NoDigest);
        assert!(matches!(
            d.verify(dir.path(), "code/bad.jar"),
            Verdict::Unreadable(_)
        ));
        std::fs::write(dir.path().join("plain.txt"), b"hellO").unwrap();
        assert!(matches!(
            d.verify(dir.path(), "plain.txt"),
            Verdict::Mismatch { .. }
        ));
    }

    #[cfg(unix)]
    #[test]
    fn a_link_is_not_the_updaters_file() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("real"), b"hello").unwrap();
        std::os::unix::fs::symlink(dir.path().join("real"), dir.path().join("link")).unwrap();
        let d = Digests::parse(&format!("link = {}\n", sha256_hex(b"hello")));
        assert!(matches!(
            d.verify(dir.path(), "link"),
            Verdict::Unreadable(_)
        ));
    }

    /// Reads a real install (read only): set SKRADLE_REAL_GAME to its folder. Every jar that still has its original
    /// contents must verify entry-wise, and digest2.txt must verify itself.
    #[test]
    #[ignore = "needs a real game folder in SKRADLE_REAL_GAME"]
    fn real_install_digests_verify() {
        let Ok(root) = std::env::var("SKRADLE_REAL_GAME") else {
            return;
        };
        let root = Path::new(&root);
        let text = std::fs::read_to_string(root.join("digest2.txt")).unwrap();
        assert!(verify_self(&text));
        let d = Digests::parse(&text);
        let (mut ok, mut other) = (0, Vec::new());
        for path in d.paths() {
            match d.verify(root, path) {
                Verdict::Match => ok += 1,
                v => other.push((path.to_owned(), v)),
            }
        }
        println!("{ok} of {} files verify; others: {other:?}", d.len());
        assert!(ok > 20);
    }
}
