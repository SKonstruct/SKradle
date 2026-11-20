//! The mods folder: which mods are in it and which are switched on.
//!
//! Every `.zip`, `.jar` and `.modpack` directly inside the folder is a mod; files whose name starts with a dot
//! (the sync manifest, the state file, partial downloads) and everything else are ignored. A mod is **enabled** unless
//! its file name is listed under `"disabled"` in `.skradle-mods.json` in the same folder:
//!
//! ```json
//! {"disabled": ["Some Mod.zip"]}
//! ```
//!
//! so a mod the player drops into the folder is on by default. The mods apply in file-name order (case-insensitive,
//! then exact), and a later mod wins when two change the same file. An unreadable state file is an error, not "all
//! enabled": switching a mod back on by accident is worse than asking the player. Links are never followed: a link
//! with a mod name is listed, but fails to open (see [`Mod::open`]).

use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::model::{Container, Mod, ModError};
use crate::jar::write_atomic;

/// The file that records which mods are switched off.
pub const STATE_FILE: &str = ".skradle-mods.json";

/// One file in the mods folder.
#[derive(Debug)]
pub struct InstalledMod {
    pub file_name: String,
    pub enabled: bool,
    /// The scanned mod, or why it cannot be used.
    pub parsed: Result<Mod, ModError>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct State {
    #[serde(default)]
    disabled: BTreeSet<String>,
}

fn read_state(dir: &Path) -> io::Result<State> {
    let bytes = match crate::bounded::read_regular(&dir.join(STATE_FILE), 1024 * 1024) {
        Ok(b) => b,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(State::default()),
        Err(e) if e.kind() == io::ErrorKind::InvalidData => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("the mods state file cannot be used: {e}"),
            ))
        }
        Err(e) => return Err(e),
    };
    serde_json::from_slice(&bytes).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("the mods state file is damaged: {e}"),
        )
    })
}

/// A plain file name: one component, not hidden, with a mod extension.
pub(crate) fn is_mod_file_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('.')
        && !name.contains(['/', '\\', '\0'])
        && Container::from_file_name(name).is_some()
}

/// The names of the mod files in `dir`, in apply order. A missing folder has none.
pub fn mod_file_names(dir: &Path) -> io::Result<Vec<String>> {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };
    let mut names: Vec<String> = entries
        .filter_map(Result::ok)
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| is_mod_file_name(n))
        .collect();
    names.sort_by(|a, b| {
        a.to_lowercase()
            .cmp(&b.to_lowercase())
            .then_with(|| a.cmp(b))
    });
    Ok(names)
}

/// Scans every mod in `dir`.
pub fn list(dir: &Path) -> io::Result<Vec<InstalledMod>> {
    let state = read_state(dir)?;
    Ok(mod_file_names(dir)?
        .into_iter()
        .map(|file_name| InstalledMod {
            enabled: !state.disabled.contains(&file_name),
            parsed: Mod::open(&dir.join(&file_name)),
            file_name,
        })
        .collect())
}

/// Switches a mod on or off. The file does not have to exist (a disabled entry for a missing mod is harmless).
pub fn set_enabled(dir: &Path, file_name: &str, enabled: bool) -> io::Result<()> {
    if !is_mod_file_name(file_name) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "not a mod file name",
        ));
    }
    let mut state = read_state(dir)?;
    let changed = if enabled {
        state.disabled.remove(file_name)
    } else {
        state.disabled.insert(file_name.to_owned())
    };
    if !changed {
        return Ok(());
    }
    fs::create_dir_all(dir)?;
    let mut json = serde_json::to_vec_pretty(&state).map_err(io::Error::other)?;
    json.push(b'\n');
    write_atomic(&dir.join(STATE_FILE), &json)
}

/// Deletes a mod file and forgets its switch. A link is removed itself, never what it points to; a folder is
/// refused. A mod a modpack sync wrote comes back at the next sync while the pack still has it.
pub fn remove(dir: &Path, file_name: &str) -> io::Result<()> {
    if !is_mod_file_name(file_name) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "not a mod file name",
        ));
    }
    let path = dir.join(file_name);
    let meta = fs::symlink_metadata(&path)?;
    if meta.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "a folder is not a mod file",
        ));
    }
    fs::remove_file(&path)?;
    // A damaged state file must not turn a successful removal into an error.
    let _ = set_enabled(dir, file_name, true);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jar::testutil::simple_zip;

    fn mods_dir() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let zip = simple_zip(&[("a.png", b"1")]);
        for name in ["b.zip", "A.zip", "c.JAR", "d.modpack"] {
            fs::write(dir.path().join(name), &zip).unwrap();
        }
        fs::write(dir.path().join("notes.txt"), b"x").unwrap();
        fs::write(dir.path().join(".modpack-synced"), b"b.zip\n").unwrap();
        fs::write(dir.path().join(".hidden.zip"), &zip).unwrap();
        fs::write(dir.path().join("e.zip.part"), b"partial").unwrap();
        fs::create_dir(dir.path().join("sub")).unwrap();
        dir
    }

    #[test]
    fn lists_mod_files_in_apply_order() {
        let dir = mods_dir();
        let names = mod_file_names(dir.path()).unwrap();
        assert_eq!(names, ["A.zip", "b.zip", "c.JAR", "d.modpack"]);
        assert!(mod_file_names(&dir.path().join("missing"))
            .unwrap()
            .is_empty());
    }

    #[test]
    fn everything_is_enabled_until_switched_off() {
        let dir = mods_dir();
        assert!(list(dir.path()).unwrap().iter().all(|m| m.enabled));
        set_enabled(dir.path(), "b.zip", false).unwrap();
        let listed = list(dir.path()).unwrap();
        let state: Vec<_> = listed
            .iter()
            .map(|m| (m.file_name.as_str(), m.enabled))
            .collect();
        assert_eq!(
            state,
            [
                ("A.zip", true),
                ("b.zip", false),
                ("c.JAR", true),
                ("d.modpack", true)
            ]
        );
        let json = fs::read_to_string(dir.path().join(STATE_FILE)).unwrap();
        assert!(
            json.contains("\"disabled\"") && json.contains("b.zip"),
            "{json}"
        );
        set_enabled(dir.path(), "b.zip", true).unwrap();
        assert!(list(dir.path()).unwrap().iter().all(|m| m.enabled));
    }

    #[test]
    fn a_broken_mod_is_listed_with_its_error() {
        let dir = mods_dir();
        fs::write(dir.path().join("bad.zip"), b"not a zip").unwrap();
        let listed = list(dir.path()).unwrap();
        let bad = listed.iter().find(|m| m.file_name == "bad.zip").unwrap();
        assert!(bad.parsed.is_err());
        // d.modpack holds an ordinary zip, so it opens but contains no mods.
        let good = listed.iter().find(|m| m.file_name == "A.zip").unwrap();
        assert!(good.parsed.is_ok());
    }

    #[test]
    fn a_damaged_state_file_is_an_error_not_all_enabled() {
        let dir = mods_dir();
        fs::write(dir.path().join(STATE_FILE), b"{ nope").unwrap();
        assert!(list(dir.path()).is_err());
        assert!(set_enabled(dir.path(), "b.zip", false).is_err());
        fs::write(dir.path().join(STATE_FILE), b"{}").unwrap();
        assert!(
            list(dir.path()).is_ok(),
            "a state file without the key is fine"
        );
    }

    #[test]
    fn set_enabled_refuses_names_that_are_not_mod_files() {
        let dir = mods_dir();
        for bad in [
            "../x.zip",
            "a/b.zip",
            "x.txt",
            ".hidden.zip",
            "",
            "a\\b.zip",
        ] {
            assert!(set_enabled(dir.path(), bad, false).is_err(), "{bad:?}");
        }
        assert!(!dir.path().join(STATE_FILE).exists());
    }

    #[test]
    fn remove_deletes_the_file_and_its_switch() {
        let dir = mods_dir();
        set_enabled(dir.path(), "b.zip", false).unwrap();
        remove(dir.path(), "b.zip").unwrap();
        assert!(!dir.path().join("b.zip").exists());
        let json = fs::read_to_string(dir.path().join(STATE_FILE)).unwrap();
        assert!(!json.contains("b.zip"), "{json}");
        assert!(remove(dir.path(), "b.zip").is_err(), "already gone");
        for bad in ["../x.zip", "notes.txt", ".hidden.zip", "sub"] {
            assert!(remove(dir.path(), bad).is_err(), "{bad:?}");
        }
        assert!(dir.path().join("notes.txt").exists());
        assert!(dir.path().join("sub").exists());
    }
}
