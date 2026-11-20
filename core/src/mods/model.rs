//! The typed model of a mod and how to read one from a file.
//!
//! Formats (all are plain zip archives; the container only decides how the archive is read):
//! - `.zip` and `.jar`: one mod. The archive root is the game's `rsrc/` folder: `item/x/model.dat` replaces
//!   `rsrc/item/x/model.dat`. A `mod.json` at the root (or, failing that, the first one in a folder) describes it.
//!   Its `type` decides where the files go: a **resource** mod (the default) writes loose files into `rsrc/`, a
//!   **class** mod merges its files into `code/config.jar`. Locale changes in `mod.json` are merged into
//!   `code/projectx-config.jar` for both. `mod.json` and `mod.png` never reach the game.
//! - `.modpack`: an archive of mods. Every `.zip` or `.jar` at its root is a mod of its own, read like the above; a
//!   modpack inside a modpack is ignored.
//!
//! `.jar` has no layout of its own that is known to differ from `.zip`; it is read as one (unverified against a real
//! `.jar` mod, none was available).
//!
//! Reading a mod only scans the archive (names, sizes, `mod.json`); file contents are read when the mod is applied,
//! so listing a folder of mods is cheap. Every archive goes through [`crate::jar::Archive`], so a hostile one is
//! rejected whole, with the reason, before anything is written anywhere.

use std::fmt;
use std::fs::File;
use std::io;
use std::path::Path;

use crate::jar::{Archive, JarError, Limits};

use super::image::{read_image_from, ImageProblem, ModImage};
use super::metadata::{parse_mod_json, ModInfo, MAX_MOD_JSON_BYTES};
use super::protected::is_protected;

/// Largest mod file read (the biggest known mod is 84 MB).
pub const MAX_MOD_FILE_BYTES: u64 = 1024 * 1024 * 1024;

/// What a mod file is, by its extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Container {
    Zip,
    Jar,
    Modpack,
}

impl Container {
    /// `.zip`, `.jar` or `.modpack` (any case); anything else is not a mod.
    pub fn from_file_name(name: &str) -> Option<Container> {
        let lower = name.to_ascii_lowercase();
        if lower.ends_with(".zip") {
            Some(Container::Zip)
        } else if lower.ends_with(".jar") {
            Some(Container::Jar)
        } else if lower.ends_with(".modpack") {
            Some(Container::Modpack)
        } else {
            None
        }
    }
}

/// Where a mod's files go.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Loose files written into `rsrc/`.
    Resource,
    /// Files merged into `code/config.jar`.
    Class,
}

/// One file of a mod that will be written (or, if `protected`, left out).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartFile {
    /// The path inside the mod, which is also the path under `rsrc/` or inside `code/config.jar`.
    pub path: String,
    /// Position in the archive.
    pub index: usize,
    pub size: u64,
    /// A game file a mod may not replace; see [`super::protected`].
    pub protected: bool,
}

/// Where a part's archive is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PartSource {
    /// The mod file itself.
    Archive,
    /// An archive inside a modpack: the entry's position and name.
    Inner { index: usize, path: String },
}

/// One installable unit: the whole of a `.zip`/`.jar` mod, or one mod inside a `.modpack`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Part {
    /// A name to show: `mod.json`'s name, else the file name.
    pub name: String,
    pub source: PartSource,
    pub info: ModInfo,
    pub kind: Kind,
    pub files: Vec<PartFile>,
    pub warnings: Vec<String>,
}

impl Part {
    /// Files that will be written.
    pub fn applicable_files(&self) -> impl Iterator<Item = &PartFile> {
        self.files.iter().filter(|f| !f.protected)
    }

    /// False when this is a class mod built for another game version than `game_version`. Mods that name no
    /// version, and installs whose version is unknown, are compatible.
    pub fn compatible_with(&self, game_version: Option<&str>) -> bool {
        if self.kind != Kind::Class {
            return true;
        }
        match (self.info.px_version.as_deref(), game_version) {
            (Some(want), Some(have)) if !want.is_empty() && !have.is_empty() => {
                want.eq_ignore_ascii_case(have)
            }
            _ => true,
        }
    }
}

/// A mod file, scanned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mod {
    pub file_name: String,
    pub container: Container,
    /// The mod's own `mod.json` (for a modpack, the pack's).
    pub info: ModInfo,
    /// One part for `.zip`/`.jar`, one per contained mod for `.modpack`.
    pub parts: Vec<Part>,
    pub warnings: Vec<String>,
}

#[derive(Debug)]
pub enum ModError {
    /// Not a regular file (a folder, a link, missing).
    NotAFile,
    /// Not `.zip`, `.jar` or `.modpack`.
    UnsupportedName,
    TooLarge,
    Archive(JarError),
    Io(io::Error),
    /// The file is not what it was when it was scanned.
    Changed(String),
}

impl fmt::Display for ModError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ModError::NotAFile => f.write_str("not a regular file"),
            ModError::UnsupportedName => f.write_str("not a .zip, .jar or .modpack file"),
            ModError::TooLarge => f.write_str("the file is too large"),
            ModError::Archive(e) => write!(f, "{e}"),
            ModError::Io(e) => write!(f, "{e}"),
            ModError::Changed(path) => {
                write!(f, "the mod changed while it was being applied ({path})")
            }
        }
    }
}

impl std::error::Error for ModError {}

impl From<JarError> for ModError {
    fn from(e: JarError) -> Self {
        ModError::Archive(e)
    }
}

impl From<io::Error> for ModError {
    fn from(e: io::Error) -> Self {
        ModError::Io(e)
    }
}

fn is_metadata_file(path: &str) -> bool {
    let base = path.rsplit('/').next().unwrap_or(path);
    base == "mod.json" || base == "mod.png"
}

/// The `mod.json` entry: the one at the root, else the first inside a folder.
pub(super) fn find_mod_json<R: io::Read + io::Seek>(archive: &Archive<R>) -> Option<usize> {
    archive
        .find("mod.json")
        .filter(|e| !e.is_dir)
        .or_else(|| {
            archive
                .entries()
                .iter()
                .find(|e| !e.is_dir && e.path.ends_with("/mod.json"))
        })
        .map(|e| e.index)
}

/// Reads `mod.json` from an opened archive. A missing, unreadable or invalid one gives the default metadata and a
/// note: the mod is still applied as a resource mod.
fn read_info<R: io::Read + io::Seek>(archive: &mut Archive<R>, notes: &mut Vec<String>) -> ModInfo {
    let Some(index) = find_mod_json(archive) else {
        return ModInfo::default();
    };
    let size = archive
        .entries()
        .iter()
        .find(|e| e.index == index)
        .map_or(0, |e| e.size);
    if size > MAX_MOD_JSON_BYTES {
        notes.push("mod.json is too large and was ignored".to_owned());
        return ModInfo::default();
    }
    let bytes = match archive.read(index) {
        Ok(b) => b,
        Err(e) => {
            notes.push(format!("mod.json could not be read: {e}"));
            return ModInfo::default();
        }
    };
    match parse_mod_json(&bytes) {
        Ok((info, warnings)) => {
            notes.extend(warnings);
            info
        }
        Err(e) => {
            notes.push(format!(
                "{e}; the mod is treated as a resource mod without metadata"
            ));
            ModInfo::default()
        }
    }
}

fn scan_part<R: io::Read + io::Seek>(
    archive: &mut Archive<R>,
    fallback_name: &str,
    source: PartSource,
) -> Part {
    let mut warnings = Vec::new();
    let info = read_info(archive, &mut warnings);
    let kind = if info.is_class() {
        Kind::Class
    } else {
        Kind::Resource
    };
    let files: Vec<PartFile> = archive
        .entries()
        .iter()
        .filter(|e| {
            !e.is_dir && !is_metadata_file(&e.path) && info.icon.as_deref() != Some(&e.path)
        })
        .map(|e| PartFile {
            path: e.path.clone(),
            index: e.index,
            size: e.size,
            protected: is_protected(&e.path),
        })
        .collect();
    if kind == Kind::Resource
        && !files.is_empty()
        && files.iter().all(|f| f.path.starts_with("rsrc/"))
    {
        warnings.push(
            "every file is under rsrc/, so it will be written to rsrc/rsrc/, which the game is not known to read"
                .to_owned(),
        );
    }
    let protected = files.iter().filter(|f| f.protected).count();
    if protected > 0 {
        warnings.push(format!(
            "{protected} protected game file(s) will be left out"
        ));
    }
    Part {
        name: info
            .name
            .clone()
            .unwrap_or_else(|| fallback_name.to_owned()),
        source,
        info,
        kind,
        files,
        warnings,
    }
}

impl Mod {
    /// Scans the mod file at `path`. The file name decides the container.
    pub fn open(path: &Path) -> Result<Mod, ModError> {
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or(ModError::UnsupportedName)?
            .to_owned();
        let container = Container::from_file_name(&file_name).ok_or(ModError::UnsupportedName)?;
        let mut archive = Archive::open(open_regular(path)?, &Limits::MOD)?;
        let mut warnings = Vec::new();
        match container {
            Container::Zip | Container::Jar => {
                let part = scan_part(&mut archive, &file_name, PartSource::Archive);
                Ok(Mod {
                    file_name,
                    container,
                    info: part.info.clone(),
                    parts: vec![part],
                    warnings,
                })
            }
            Container::Modpack => {
                let info = read_info(&mut archive, &mut warnings);
                let inner: Vec<(usize, String)> = archive
                    .entries()
                    .iter()
                    .filter(|e| {
                        !e.is_dir
                            && !e.path.contains('/')
                            && matches!(
                                Container::from_file_name(&e.path),
                                Some(Container::Zip | Container::Jar)
                            )
                    })
                    .map(|e| (e.index, e.path.clone()))
                    .collect();
                let mut parts = Vec::new();
                for (index, name) in inner {
                    let scanned = archive
                        .read(index)
                        .map_err(ModError::from)
                        .and_then(|bytes| {
                            Archive::from_bytes(bytes, &Limits::MOD).map_err(ModError::from)
                        });
                    match scanned {
                        Ok(mut inner_archive) => parts.push(scan_part(
                            &mut inner_archive,
                            &name,
                            PartSource::Inner {
                                index,
                                path: name.clone(),
                            },
                        )),
                        Err(e) => warnings.push(format!("{name} was skipped: {e}")),
                    }
                }
                if parts.is_empty() {
                    warnings.push("the modpack contains no readable mods".to_owned());
                }
                Ok(Mod {
                    file_name,
                    container,
                    info,
                    parts,
                    warnings,
                })
            }
        }
    }

    /// The mod's picture: the `icon` (or `image`) path of `mod.json`, else `mod.png` at the archive root or beside
    /// `mod.json`. `path` is the mod file this was scanned from. `Ok(None)` when the mod has none; an `Err` says why
    /// a picture it has cannot be used (too large, not a PNG or JPEG, ...), and callers show no picture then.
    pub fn read_image(&self, path: &Path) -> Result<Option<ModImage>, ImageProblem> {
        let mut archive = Archive::open(
            open_regular(path).map_err(|_| ImageProblem::Unreadable)?,
            &Limits::MOD,
        )
        .map_err(|_| ImageProblem::Unreadable)?;
        read_image_from(&mut archive, &self.info)
    }

    /// A name to show.
    pub fn display_name(&self) -> &str {
        self.info.name.as_deref().unwrap_or(&self.file_name)
    }

    /// The pack's own metadata when this is a modpack. Its locale changes apply once, after those of the mods it
    /// holds (so the pack wins), not once per part.
    pub fn pack_info(&self) -> Option<&ModInfo> {
        (self.container == Container::Modpack).then_some(&self.info)
    }

    /// Reads every file of `part` that will be written (protected ones excluded) from the archive at `path` and
    /// passes it to `f`. The archive is checked again as it is opened, and a file that is no longer what was scanned
    /// is an error.
    pub fn each_file<E: From<ModError>>(
        &self,
        path: &Path,
        part: &Part,
        f: impl FnMut(&PartFile, Vec<u8>) -> Result<(), E>,
    ) -> Result<(), E> {
        self.each_file_where(path, part, |_| true, f)
    }

    /// Like [`Mod::each_file`], but only reads the files `want` accepts.
    pub fn each_file_where<E: From<ModError>>(
        &self,
        path: &Path,
        part: &Part,
        want: impl Fn(&PartFile) -> bool,
        mut f: impl FnMut(&PartFile, Vec<u8>) -> Result<(), E>,
    ) -> Result<(), E> {
        // The archive is checked again as it is opened: a file swapped for a link or a huge one since the scan is
        // refused, not read.
        let mut outer = Archive::open(open_regular(path)?, &Limits::MOD).map_err(ModError::from)?;
        match &part.source {
            PartSource::Archive => each_in(&mut outer, part, &want, &mut f),
            PartSource::Inner {
                index,
                path: inner_path,
            } => {
                let ok = outer
                    .entries()
                    .iter()
                    .any(|e| e.index == *index && &e.path == inner_path);
                if !ok {
                    return Err(ModError::Changed(inner_path.clone()).into());
                }
                let bytes = outer.read(*index).map_err(ModError::from)?;
                let mut inner = Archive::from_bytes(bytes, &Limits::MOD).map_err(ModError::from)?;
                each_in(&mut inner, part, &want, &mut f)
            }
        }
    }
}

/// Opens a mod file after checking it is a plain file (not a link, not a folder) within the size limit.
fn open_regular(path: &Path) -> Result<File, ModError> {
    let meta = std::fs::symlink_metadata(path)?;
    if !meta.is_file() {
        return Err(ModError::NotAFile);
    }
    if meta.len() > MAX_MOD_FILE_BYTES {
        return Err(ModError::TooLarge);
    }
    Ok(File::open(path)?)
}

fn each_in<R: io::Read + io::Seek, E: From<ModError>>(
    archive: &mut Archive<R>,
    part: &Part,
    want: &impl Fn(&PartFile) -> bool,
    f: &mut impl FnMut(&PartFile, Vec<u8>) -> Result<(), E>,
) -> Result<(), E> {
    for file in part.applicable_files().filter(|f| want(f)) {
        let matches = archive
            .entries()
            .iter()
            .any(|e| e.index == file.index && e.path == file.path && e.size == file.size);
        if !matches {
            return Err(ModError::Changed(file.path.clone()).into());
        }
        let bytes = archive.read(file.index).map_err(ModError::from)?;
        f(file, bytes)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jar::testutil::{simple_zip, zip_with};

    fn write(dir: &Path, name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, bytes).unwrap();
        path
    }

    #[test]
    fn container_by_extension() {
        assert_eq!(Container::from_file_name("a.ZIP"), Some(Container::Zip));
        assert_eq!(Container::from_file_name("a.jar"), Some(Container::Jar));
        assert_eq!(
            Container::from_file_name("a.Modpack"),
            Some(Container::Modpack)
        );
        assert_eq!(Container::from_file_name("a.txt"), None);
        assert_eq!(Container::from_file_name(".modpack-synced"), None);
    }

    #[test]
    fn a_resource_mod_with_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let zip = simple_zip(&[
            (
                "mod.json",
                br#"{"mod": {"name": "Acheron Trail", "version": "1",}}"#,
            ),
            ("mod.png", b"png"),
            ("particle/a.dat", b"1"),
            ("config/item.dat", b"protected"),
            ("config/", b""),
        ]);
        let path = write(dir.path(), "Acheron_Trail.zip", &zip);
        let m = Mod::open(&path).unwrap();
        assert_eq!(m.display_name(), "Acheron Trail");
        assert_eq!(m.parts.len(), 1);
        let p = &m.parts[0];
        assert_eq!(p.kind, Kind::Resource);
        let paths: Vec<_> = p
            .files
            .iter()
            .map(|f| (f.path.as_str(), f.protected))
            .collect();
        assert_eq!(
            paths,
            [("particle/a.dat", false), ("config/item.dat", true)]
        );
        assert_eq!(p.applicable_files().count(), 1);
        assert!(p.warnings.iter().any(|w| w.contains("protected")));
    }

    #[test]
    fn mod_json_in_a_folder_is_found_and_never_applied() {
        let dir = tempfile::tempdir().unwrap();
        let zip = simple_zip(&[
            ("item/mod.json", br#"{"mod": {"name": "Luck Monster"}}"#),
            ("item/x.dat", b"1"),
        ]);
        let m = Mod::open(&write(dir.path(), "l.zip", &zip)).unwrap();
        assert_eq!(m.display_name(), "Luck Monster");
        let files: Vec<_> = m.parts[0].files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(files, ["item/x.dat"]);
    }

    #[test]
    fn a_mod_without_or_with_garbage_metadata_is_still_a_resource_mod() {
        let dir = tempfile::tempdir().unwrap();
        let none = write(dir.path(), "plain.zip", &simple_zip(&[("a/b.png", b"1")]));
        let m = Mod::open(&none).unwrap();
        assert_eq!(m.display_name(), "plain.zip");
        assert_eq!(m.parts[0].name, "plain.zip");
        assert!(m.parts[0].warnings.is_empty());
        let junk = write(
            dir.path(),
            "junk.zip",
            &simple_zip(&[("mod.json", b"{{{ not json"), ("a.png", b"1")]),
        );
        let m = Mod::open(&junk).unwrap();
        assert_eq!(m.parts[0].kind, Kind::Resource);
        assert_eq!(m.parts[0].files.len(), 1);
        assert!(m.parts[0].warnings.iter().any(|w| w.contains("mod.json")));
    }

    #[test]
    fn a_class_mod_and_its_game_version_check() {
        let dir = tempfile::tempdir().unwrap();
        let zip = simple_zip(&[
            (
                "mod.json",
                br#"{"mod": {"name": "C", "type": "class", "pxVersion": "20261110044614"}}"#,
            ),
            ("com/x/Y.class", b"cafebabe"),
        ]);
        let m = Mod::open(&write(dir.path(), "c.jar", &zip)).unwrap();
        let p = &m.parts[0];
        assert_eq!(m.container, Container::Jar);
        assert_eq!(p.kind, Kind::Class);
        assert!(p.compatible_with(Some("20261110044614")));
        assert!(!p.compatible_with(Some("20270101000000")));
        assert!(p.compatible_with(None), "unknown install version");
        let resource = Mod::open(&write(dir.path(), "r.zip", &simple_zip(&[("a", b"1")]))).unwrap();
        assert!(resource.parts[0].compatible_with(Some("1")));
    }

    #[test]
    fn rsrc_prefixed_mods_get_a_note() {
        let dir = tempfile::tempdir().unwrap();
        let zip = simple_zip(&[("rsrc/character/x.dat", b"1"), ("rsrc/", b"")]);
        let m = Mod::open(&write(dir.path(), "b.zip", &zip)).unwrap();
        assert!(m.parts[0].warnings.iter().any(|w| w.contains("rsrc/rsrc/")));
    }

    #[test]
    fn a_modpack_holds_several_mods() {
        let dir = tempfile::tempdir().unwrap();
        let class = simple_zip(&[
            ("mod.json", br#"{"type": "class", "name": "Inner Class"}"#),
            ("a/B.class", b"x"),
        ]);
        let res = simple_zip(&[
            (
                "mod.json",
                br#"{"name": "Inner Res", "locale": {"m.properties": {"k": "v"}}}"#,
            ),
            ("t/x.png", b"1"),
        ]);
        let pack = simple_zip(&[
            ("mod.json", br#"{"mod": {"name": "The Pack"}}"#),
            ("one.zip", &class),
            ("two.JAR", &res),
            ("broken.zip", b"not a zip"),
            ("readme.txt", b"hi"),
            ("nested/three.zip", &res),
            ("again.modpack", &res),
        ]);
        let m = Mod::open(&write(dir.path(), "pack.modpack", &pack)).unwrap();
        assert_eq!(m.container, Container::Modpack);
        assert_eq!(m.display_name(), "The Pack");
        let names: Vec<_> = m.parts.iter().map(|p| (p.name.as_str(), p.kind)).collect();
        assert_eq!(
            names,
            [("Inner Class", Kind::Class), ("Inner Res", Kind::Resource)]
        );
        assert!(
            m.warnings.iter().any(|w| w.contains("broken.zip")),
            "{:?}",
            m.warnings
        );
        assert_eq!(m.pack_info().is_some(), m.container == Container::Modpack);
        assert_eq!(m.parts[1].info.locale_changes(), 1);
    }

    #[test]
    fn each_file_streams_applicable_files_from_a_mod_and_from_a_modpack() {
        let dir = tempfile::tempdir().unwrap();
        let inner = simple_zip(&[
            ("a/1.png", b"one"),
            ("config/wall.dat", b"no"),
            ("b/2.png", b"two"),
        ]);
        let pack = simple_zip(&[("in.zip", &inner)]);
        let path = write(dir.path(), "p.modpack", &pack);
        let m = Mod::open(&path).unwrap();
        let mut seen = Vec::new();
        m.each_file::<ModError>(&path, &m.parts[0], |f, bytes| {
            seen.push((f.path.clone(), bytes));
            Ok(())
        })
        .unwrap();
        assert_eq!(
            seen,
            [
                ("a/1.png".to_owned(), b"one".to_vec()),
                ("b/2.png".to_owned(), b"two".to_vec())
            ]
        );
    }

    #[test]
    fn a_file_swapped_after_the_scan_is_detected() {
        let dir = tempfile::tempdir().unwrap();
        let path = write(dir.path(), "m.zip", &simple_zip(&[("a.png", b"1")]));
        let m = Mod::open(&path).unwrap();
        std::fs::write(&path, simple_zip(&[("other.png", b"1"), ("a.png", b"2")])).unwrap();
        let err = m
            .each_file::<ModError>(&path, &m.parts[0], |_, _| Ok(()))
            .unwrap_err();
        assert!(matches!(err, ModError::Changed(_)), "{err}");
    }

    #[test]
    fn hostile_archives_are_rejected_whole() {
        let dir = tempfile::tempdir().unwrap();
        let cases: Vec<(&str, Vec<u8>)> = vec![
            (
                "slip.zip",
                simple_zip(&[("ok.png", b"1"), ("../../evil", b"x")]),
            ),
            ("abs.zip", simple_zip(&[("/etc/x", b"x")])),
            (
                "link.zip",
                zip_with(&[("l", b"/etc/passwd", Some(0o120777))]),
            ),
            ("trunc.zip", simple_zip(&[("a", b"hello")])[..20].to_vec()),
            ("garbage.zip", b"MZ garbage".to_vec()),
            ("empty.zip", Vec::new()),
        ];
        for (name, bytes) in cases {
            let path = write(dir.path(), name, &bytes);
            assert!(Mod::open(&path).is_err(), "{name}");
        }
    }

    #[test]
    fn refuses_the_wrong_kind_of_file() {
        let dir = tempfile::tempdir().unwrap();
        let txt = write(dir.path(), "a.txt", b"x");
        assert!(matches!(Mod::open(&txt), Err(ModError::UnsupportedName)));
        let sub = dir.path().join("dir.zip");
        std::fs::create_dir(&sub).unwrap();
        assert!(matches!(Mod::open(&sub), Err(ModError::NotAFile)));
        assert!(Mod::open(&dir.path().join("missing.zip")).is_err());
        #[cfg(unix)]
        {
            let real = write(dir.path(), "real.zip", &simple_zip(&[("a", b"1")]));
            let link = dir.path().join("link.zip");
            std::os::unix::fs::symlink(&real, &link).unwrap();
            assert!(matches!(Mod::open(&link), Err(ModError::NotAFile)));
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_mod_swapped_for_a_link_after_the_scan_is_not_read() {
        let dir = tempfile::tempdir().unwrap();
        let path = write(dir.path(), "m.zip", &simple_zip(&[("a.txt", b"1")]));
        let scanned = Mod::open(&path).unwrap();
        let elsewhere = write(dir.path(), "elsewhere.zip", &simple_zip(&[("a.txt", b"1")]));
        std::fs::remove_file(&path).unwrap();
        std::os::unix::fs::symlink(&elsewhere, &path).unwrap();
        let result = scanned.each_file::<ModError>(&path, &scanned.parts[0], |_, _| Ok(()));
        assert!(matches!(result, Err(ModError::NotAFile)), "{result:?}");
    }
}
