//! The editors pack: one jar with Silverweave, the Model Viewer, the Particle Editor and the Interface Tester.
//!
//! The `Editors` workflow builds it every week from the newest upstream sources (spiralview and Clyde, both BSD-2-Clause)
//! and publishes `skradle-editors.jar` plus a small manifest, `editors.json`, as the rolling `editors` release of this
//! project. This module reads the manifest, says whether the copy on disk is the newest, and downloads the jar, which is
//! kept only when its SHA-256 matches the manifest's. The pack never contains anything of the game itself (the build
//! refuses to publish a jar with a class taken from the game); at run time the game's jars sit behind it on the class
//! path (see [`crate::editors`]).
//!
//! On disk (`<dir>`, the app's data folder `editors/`): `skradle-editors.jar` and `editors.json`, the manifest of the
//! jar that is there. The jar is written first, the manifest after it, so a download that stops half way leaves a pack
//! that [`installed`] does not believe in.

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::net::{Client, DownloadOptions, NetError, Progress};

/// Where the rolling release lives. Only this exact prefix is ever downloaded from.
pub const RELEASE_BASE: &str = "https://github.com/SKonstruct/SKradle/releases/download/editors/";
pub const MANIFEST_NAME: &str = "editors.json";
pub const JAR_NAME: &str = "skradle-editors.jar";
const MAX_MANIFEST_BYTES: u64 = 64 * 1024;
/// The jar is about 24 MB; anything near this is wrong.
const MAX_JAR_BYTES: u64 = 150 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JarInfo {
    pub name: String,
    pub sha256: String,
    pub size: u64,
}

/// What the workflow wrote about one build of the pack.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub schema: u32,
    /// `<spiralview commit>-<clyde version>-<game code hash>`: differs whenever any input changed.
    pub build: String,
    pub jar: JarInfo,
    /// The oldest Java major version the jar runs on.
    pub java_min: u32,
}

#[derive(Debug)]
pub enum PackError {
    Net(NetError),
    /// The manifest is not what this version of SKradle understands, or says something unsafe.
    Manifest(String),
    Io(io::Error),
}

impl fmt::Display for PackError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Net(e) => write!(f, "{e}"),
            Self::Manifest(why) => write!(f, "the editors' manifest is not usable: {why}"),
            Self::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for PackError {}

impl From<NetError> for PackError {
    fn from(e: NetError) -> Self {
        Self::Net(e)
    }
}

impl From<io::Error> for PackError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

/// Reads and checks a manifest. Everything in it is untrusted input: it names a file to write and a size cap.
pub fn parse_manifest(text: &str) -> Result<Manifest, PackError> {
    let bad = |why: &str| PackError::Manifest(why.to_owned());
    let manifest: Manifest = serde_json::from_str(text).map_err(|e| bad(&e.to_string()))?;
    if manifest.schema != 1 {
        return Err(bad("a newer format than this SKradle reads"));
    }
    let build_ok = !manifest.build.is_empty()
        && manifest.build.len() <= 96
        && manifest
            .build
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
    if !build_ok {
        return Err(bad("the build name"));
    }
    if manifest.jar.name != JAR_NAME {
        return Err(bad("the jar name"));
    }
    let sha = &manifest.jar.sha256;
    if sha.len() != 64 || !sha.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(bad("the jar checksum"));
    }
    if manifest.jar.size == 0 || manifest.jar.size > MAX_JAR_BYTES {
        return Err(bad("the jar size"));
    }
    if !(8..=100).contains(&manifest.java_min) {
        return Err(bad("the Java version"));
    }
    Ok(manifest)
}

/// The pack on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installed {
    pub manifest: Manifest,
    pub jar: PathBuf,
}

/// The pack in `dir`, if a complete one is there: a readable manifest and a jar of exactly the size it names.
pub fn installed(dir: &Path) -> Option<Installed> {
    let text = std::fs::read_to_string(dir.join(MANIFEST_NAME)).ok()?;
    let manifest = parse_manifest(&text).ok()?;
    let jar = dir.join(JAR_NAME);
    let size = std::fs::metadata(&jar).ok()?.len();
    (size == manifest.jar.size).then_some(Installed { manifest, jar })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    NotInstalled,
    UpToDate,
    /// A newer build is published.
    UpdateAvailable,
}

/// How the copy on disk compares with the published manifest.
pub fn status(installed: Option<&Installed>, remote: &Manifest) -> Status {
    match installed {
        None => Status::NotInstalled,
        Some(i) if i.manifest.build == remote.build => Status::UpToDate,
        Some(_) => Status::UpdateAvailable,
    }
}

/// Asks GitHub for the published manifest.
pub fn fetch_manifest(client: &Client) -> Result<Manifest, PackError> {
    let text = client.get_text(
        &format!("{RELEASE_BASE}{MANIFEST_NAME}"),
        MAX_MANIFEST_BYTES,
    )?;
    parse_manifest(&text)
}

/// Downloads the jar named by `manifest` into `dir`, keeps it only when its SHA-256 matches, then records the manifest.
pub fn download(
    client: &Client,
    manifest: &Manifest,
    dir: &Path,
    progress: &mut dyn FnMut(Progress),
) -> Result<Installed, PackError> {
    std::fs::create_dir_all(dir)?;
    let jar = dir.join(JAR_NAME);
    let options = DownloadOptions::new(manifest.jar.size).with_sha256(manifest.jar.sha256.clone());
    client.download(
        &format!("{RELEASE_BASE}{JAR_NAME}"),
        &jar,
        &options,
        progress,
    )?;
    save_manifest(dir, manifest)?;
    Ok(Installed {
        manifest: manifest.clone(),
        jar,
    })
}

/// Writes `manifest` as the one for the jar in `dir`, all or nothing.
pub fn save_manifest(dir: &Path, manifest: &Manifest) -> Result<(), PackError> {
    let text =
        serde_json::to_string_pretty(manifest).map_err(|e| PackError::Manifest(e.to_string()))?;
    let part = dir.join(format!("{MANIFEST_NAME}.part"));
    std::fs::write(&part, text)?;
    std::fs::rename(&part, dir.join(MANIFEST_NAME))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn good() -> String {
        format!(
            r#"{{"schema":1,"build":"c488ba2-1.2.1-9bd80dc","jar":{{"name":"{JAR_NAME}","sha256":"{}","size":1234}},"java_min":25,"spiralview":{{"commit":"c488"}},"future":true}}"#,
            "ab".repeat(32)
        )
    }

    #[test]
    fn a_good_manifest_is_read_and_unknown_fields_are_ignored() {
        let m = parse_manifest(&good()).unwrap();
        assert_eq!(m.build, "c488ba2-1.2.1-9bd80dc");
        assert_eq!((m.jar.size, m.java_min), (1234, 25));
    }

    #[test]
    fn a_manifest_that_names_something_unsafe_or_unknown_is_refused() {
        let cases = [
            good().replace(r#""schema":1"#, r#""schema":2"#),
            good().replace("c488ba2-1.2.1-9bd80dc", "../../evil"),
            good().replace("c488ba2-1.2.1-9bd80dc", ""),
            good().replace(JAR_NAME, "../skradle-editors.jar"),
            good().replace(&"ab".repeat(32), "zz"),
            good().replace(r#""size":1234"#, r#""size":0"#),
            good().replace(r#""size":1234"#, r#""size":999999999999"#),
            good().replace(r#""java_min":25"#, r#""java_min":3"#),
            "not json".to_owned(),
            "{}".to_owned(),
        ];
        for text in cases {
            assert!(
                matches!(parse_manifest(&text), Err(PackError::Manifest(_))),
                "{text}"
            );
        }
    }

    #[test]
    fn the_pack_on_disk_counts_only_when_manifest_and_jar_agree() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(installed(dir.path()), None);

        let manifest = parse_manifest(&good()).unwrap();
        save_manifest(dir.path(), &manifest).unwrap();
        assert_eq!(installed(dir.path()), None, "no jar yet");

        std::fs::write(dir.path().join(JAR_NAME), vec![0u8; 10]).unwrap();
        assert_eq!(installed(dir.path()), None, "a jar of the wrong size");

        std::fs::write(dir.path().join(JAR_NAME), vec![0u8; 1234]).unwrap();
        let found = installed(dir.path()).unwrap();
        assert_eq!(found.manifest, manifest);
        assert_eq!(found.jar, dir.path().join(JAR_NAME));

        std::fs::write(dir.path().join(MANIFEST_NAME), "{ broken").unwrap();
        assert_eq!(installed(dir.path()), None, "a damaged manifest");
        assert!(!dir.path().join("editors.json.part").exists());
    }

    #[test]
    fn newer_builds_are_noticed_and_the_same_build_is_not_downloaded_again() {
        let remote = parse_manifest(&good()).unwrap();
        assert_eq!(status(None, &remote), Status::NotInstalled);
        let same = Installed {
            manifest: remote.clone(),
            jar: PathBuf::from("x"),
        };
        assert_eq!(status(Some(&same), &remote), Status::UpToDate);
        let mut older = same.clone();
        older.manifest.build = "0000000-1.2.0-1111111".into();
        assert_eq!(status(Some(&older), &remote), Status::UpdateAvailable);
    }
}
