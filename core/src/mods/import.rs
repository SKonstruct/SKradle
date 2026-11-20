//! Adding mod files to the mods folder: each file is scanned first, and only a file that scans cleanly is copied.
//!
//! The copy goes to a hidden temporary name next to its final one, is scanned again (the source could change while it
//! is read), and is then linked into place, which fails rather than replaces when the name is taken. A name already
//! in use gets a number (`Name (2).zip`): an existing mod is never overwritten.

use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};

use super::list::is_mod_file_name;
use super::model::{Container, Mod, MAX_MOD_FILE_BYTES};

/// A file that was copied in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Imported {
    /// The name it has in the mods folder.
    pub file_name: String,
    /// True when the name was taken and a number was added.
    pub renamed: bool,
}

/// Why a file was not added, in words for the player.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal(pub String);

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

fn refuse<T>(why: impl Into<String>) -> Result<T, Refusal> {
    Err(Refusal(why.into()))
}

/// Checks `src` as a mod without copying it. The error says why it is not one.
pub fn validate(src: &Path) -> Result<Mod, Refusal> {
    let meta = match fs::symlink_metadata(src) {
        Ok(m) => m,
        Err(e) => return refuse(format!("cannot be read: {e}")),
    };
    if meta.file_type().is_symlink() {
        return refuse("it is a link, not a file");
    }
    if meta.is_dir() {
        return refuse("it is a folder; add the mod files inside it");
    }
    if !meta.is_file() {
        return refuse("it is not a regular file");
    }
    let Some(name) = src.file_name().and_then(|n| n.to_str()) else {
        return refuse("its name is not valid text");
    };
    if Container::from_file_name(name).is_none() {
        return refuse("not a .zip, .jar or .modpack file");
    }
    if name.starts_with('.') {
        return refuse("a name that starts with a dot is hidden and would be ignored");
    }
    if !is_mod_file_name(name) {
        return refuse("its name cannot be used as a mod file name");
    }
    if meta.len() > MAX_MOD_FILE_BYTES {
        return refuse("it is too large (over 1 GiB)");
    }
    let scanned = match Mod::open(src) {
        Ok(m) => m,
        Err(e) => return refuse(format!("not a usable mod: {e}")),
    };
    if scanned.parts.is_empty() {
        return refuse("it is a modpack with no readable mods inside");
    }
    if scanned
        .parts
        .iter()
        .all(|p| p.applicable_files().next().is_none())
    {
        return refuse("it holds no files a mod could change");
    }
    Ok(scanned)
}

/// `Name.zip`, `Name (2).zip`, `Name (3).zip`, ...
fn numbered(name: &str, n: u32) -> String {
    if n <= 1 {
        return name.to_owned();
    }
    match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => format!("{stem} ({n}).{ext}"),
        _ => format!("{name} ({n})"),
    }
}

fn copy_to(src: &Path, dest: &Path) -> io::Result<()> {
    let mut from = File::open(src)?;
    let mut to = OpenOptions::new().write(true).create_new(true).open(dest)?;
    io::copy(&mut from, &mut to)?;
    to.sync_all()
}

/// Validates `src` and copies it into `mods_dir` (created when missing). Never replaces an existing file.
pub fn import_file(src: &Path, mods_dir: &Path) -> Result<Imported, Refusal> {
    validate(src)?;
    let name = src
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .to_owned();
    if let Err(e) = fs::create_dir_all(mods_dir) {
        return refuse(format!("the mods folder cannot be created: {e}"));
    }
    let tmp: PathBuf = mods_dir.join(format!(".adding-{}-{name}", std::process::id()));
    let _ = fs::remove_file(&tmp);
    let result = (|| -> Result<Imported, Refusal> {
        if let Err(e) = copy_to(src, &tmp) {
            return refuse(format!("could not be copied: {e}"));
        }
        // The copy must be a mod too: the source may have changed since the first look.
        let copied = Mod::open(&tmp).map_err(|e| Refusal(format!("not a usable mod: {e}")))?;
        if copied.parts.is_empty() {
            return refuse("it is a modpack with no readable mods inside");
        }
        for n in 1..=999 {
            let candidate = numbered(&name, n);
            let dest = mods_dir.join(&candidate);
            match fs::hard_link(&tmp, &dest) {
                Ok(()) => {
                    return Ok(Imported {
                        file_name: candidate,
                        renamed: n > 1,
                    })
                }
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(_) => {
                    // No hard links here (some file systems): a check, then a rename.
                    if fs::symlink_metadata(&dest).is_ok() {
                        continue;
                    }
                    return match fs::rename(&tmp, &dest) {
                        Ok(()) => Ok(Imported {
                            file_name: candidate,
                            renamed: n > 1,
                        }),
                        Err(e) => refuse(format!("could not be saved: {e}")),
                    };
                }
            }
        }
        refuse("too many mods with this name")
    })();
    let _ = fs::remove_file(&tmp);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jar::testutil::simple_zip;

    fn write(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
        let p = dir.join(name);
        fs::write(&p, bytes).unwrap();
        p
    }

    fn good() -> Vec<u8> {
        simple_zip(&[("ui/a.png", b"1")])
    }

    #[test]
    fn a_valid_mod_is_copied_and_nothing_is_left_behind() {
        let src = tempfile::tempdir().unwrap();
        let mods = tempfile::tempdir().unwrap();
        let mods_dir = mods.path().join("mods");
        let p = write(src.path(), "Nice.zip", &good());
        let got = import_file(&p, &mods_dir).unwrap();
        assert_eq!(
            got,
            Imported {
                file_name: "Nice.zip".into(),
                renamed: false
            }
        );
        assert_eq!(fs::read(mods_dir.join("Nice.zip")).unwrap(), good());
        assert!(p.exists(), "the source stays");
        let names: Vec<_> = fs::read_dir(&mods_dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        assert_eq!(names, ["Nice.zip"]);
    }

    #[test]
    fn an_existing_name_is_never_overwritten() {
        let src = tempfile::tempdir().unwrap();
        let mods = tempfile::tempdir().unwrap();
        write(mods.path(), "Nice.zip", b"theirs");
        let p = write(src.path(), "Nice.zip", &good());
        let first = import_file(&p, mods.path()).unwrap();
        assert_eq!(first.file_name, "Nice (2).zip");
        assert!(first.renamed);
        let second = import_file(&p, mods.path()).unwrap();
        assert_eq!(second.file_name, "Nice (3).zip");
        assert_eq!(fs::read(mods.path().join("Nice.zip")).unwrap(), b"theirs");
    }

    #[test]
    fn refusals_say_why() {
        let src = tempfile::tempdir().unwrap();
        let mods = tempfile::tempdir().unwrap();
        let cases: Vec<(PathBuf, &str)> = vec![
            (write(src.path(), "notes.txt", b"x"), "not a .zip"),
            (
                write(src.path(), "broken.zip", b"not a zip"),
                "not a usable mod",
            ),
            (write(src.path(), ".hidden.zip", &good()), "hidden"),
            (
                write(src.path(), "empty.zip", &simple_zip(&[("mod.json", b"{}")])),
                "no files",
            ),
            (
                write(
                    src.path(),
                    "evil.zip",
                    &simple_zip(&[("../escape.png", b"x")]),
                ),
                "not a usable mod",
            ),
            (src.path().join("missing.zip"), "cannot be read"),
        ];
        for (path, want) in cases {
            let err = import_file(&path, mods.path()).unwrap_err();
            assert!(err.0.contains(want), "{path:?}: {err}");
        }
        fs::create_dir(src.path().join("folder.zip")).unwrap();
        let err = import_file(&src.path().join("folder.zip"), mods.path()).unwrap_err();
        assert!(err.0.contains("folder"), "{err}");
        assert!(
            fs::read_dir(mods.path()).unwrap().next().is_none(),
            "a refused file leaves nothing in the folder"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_link_is_refused() {
        let src = tempfile::tempdir().unwrap();
        let mods = tempfile::tempdir().unwrap();
        let real = write(src.path(), "real.zip", &good());
        let link = src.path().join("link.zip");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        assert!(import_file(&link, mods.path())
            .unwrap_err()
            .0
            .contains("link"));
    }

    #[test]
    fn an_empty_modpack_is_refused() {
        let src = tempfile::tempdir().unwrap();
        let mods = tempfile::tempdir().unwrap();
        let p = write(
            src.path(),
            "p.modpack",
            &simple_zip(&[("readme.txt", b"x")]),
        );
        assert!(import_file(&p, mods.path()).is_err());
    }

    #[test]
    fn numbering() {
        assert_eq!(numbered("A.zip", 1), "A.zip");
        assert_eq!(numbered("A.b.zip", 2), "A.b (2).zip");
    }
}
