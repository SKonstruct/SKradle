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

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::model::{Container, Kind, Mod, ModError};
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
    /// The order the player chose, by file name, first applied first; a mod later in it wins where two change the same
    /// file. Mods not named here (new ones) come after the named ones, by name, so a mod just added takes effect.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    order: Vec<String>,
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

/// The names of the mod files in `dir`, in apply order: the order the player chose ([`move_mod`]), then any others by
/// name. The last mod wins where two change the same file. A missing folder has none.
pub fn mod_file_names(dir: &Path) -> io::Result<Vec<String>> {
    let names = sorted_names(dir)?;
    if names.is_empty() {
        return Ok(names);
    }
    Ok(in_chosen_order(names, &read_state(dir)?.order))
}

/// `names` with the ones `order` names first, in that order; the rest follow in their own order.
fn in_chosen_order(mut names: Vec<String>, order: &[String]) -> Vec<String> {
    let mut out = Vec::with_capacity(names.len());
    for wanted in order {
        if let Some(i) = names.iter().position(|n| n == wanted) {
            out.push(names.remove(i));
        }
    }
    out.extend(names);
    out
}

/// Moves a mod to position `to` (0 is applied first) in the apply order and remembers the whole order.
pub fn move_mod(dir: &Path, file_name: &str, to: usize) -> io::Result<()> {
    if !is_mod_file_name(file_name) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "not a mod file name",
        ));
    }
    let mut names = mod_file_names(dir)?;
    let Some(from) = names.iter().position(|n| n == file_name) else {
        return Err(io::Error::new(io::ErrorKind::NotFound, "no such mod"));
    };
    let name = names.remove(from);
    names.insert(to.min(names.len()), name);
    let mut state = read_state(dir)?;
    if state.order == names {
        return Ok(());
    }
    state.order = names;
    let mut json = serde_json::to_vec_pretty(&state).map_err(io::Error::other)?;
    json.push(b'\n');
    write_atomic(&dir.join(STATE_FILE), &json)
}

/// Two mods that change the same files: `winner` comes later in the apply order, so its version of those files is the
/// one the game gets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    pub loser: String,
    pub winner: String,
    /// How many files both change.
    pub count: usize,
    /// A few of them, for a tooltip.
    pub examples: Vec<String>,
}

const MAX_EXAMPLES: usize = 3;

/// The files two enabled mods both change, for every such pair, from the scans in apply order. A mod does not conflict
/// with itself, and a file in `rsrc/` and one of the same name inside `config.jar` are different files.
pub fn conflicts(mods: &[InstalledMod]) -> Vec<Conflict> {
    let mut writers: HashMap<(bool, &str), Vec<usize>> = HashMap::new();
    for (i, installed) in mods.iter().enumerate() {
        let (true, Ok(scanned)) = (installed.enabled, &installed.parsed) else {
            continue;
        };
        for part in &scanned.parts {
            for file in part.applicable_files() {
                let list = writers
                    .entry((part.kind == Kind::Class, file.path.as_str()))
                    .or_default();
                if list.last() != Some(&i) {
                    list.push(i);
                }
            }
        }
    }
    let mut pairs: BTreeMap<(usize, usize), (usize, Vec<String>)> = BTreeMap::new();
    let mut shared: Vec<(&(bool, &str), &Vec<usize>)> =
        writers.iter().filter(|(_, w)| w.len() > 1).collect();
    shared.sort_by(|a, b| a.0.cmp(b.0));
    for ((_, path), who) in shared {
        for (k, &a) in who.iter().enumerate() {
            for &b in &who[k + 1..] {
                let entry = pairs.entry((a, b)).or_default();
                entry.0 += 1;
                if entry.1.len() < MAX_EXAMPLES {
                    entry.1.push((*path).to_owned());
                }
            }
        }
    }
    pairs
        .into_iter()
        .map(|((a, b), (count, examples))| Conflict {
            loser: mods[a].file_name.clone(),
            winner: mods[b].file_name.clone(),
            count,
            examples,
        })
        .collect()
}

fn sorted_names(dir: &Path) -> io::Result<Vec<String>> {
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
    fn a_chosen_order_is_kept_and_new_mods_come_last() {
        let dir = mods_dir();
        move_mod(dir.path(), "d.modpack", 0).unwrap();
        move_mod(dir.path(), "A.zip", 3).unwrap();
        assert_eq!(
            mod_file_names(dir.path()).unwrap(),
            ["d.modpack", "b.zip", "c.JAR", "A.zip"]
        );
        // The order is written for people to read, and a mod added later takes effect (it comes last).
        let json = fs::read_to_string(dir.path().join(STATE_FILE)).unwrap();
        assert!(json.contains("\"order\": [\n    \"d.modpack\","), "{json}");
        fs::write(dir.path().join("0new.zip"), simple_zip(&[("a.png", b"1")])).unwrap();
        assert_eq!(
            mod_file_names(dir.path()).unwrap(),
            ["d.modpack", "b.zip", "c.JAR", "A.zip", "0new.zip"]
        );
        // Moving past the end puts it last; unknown or unsafe names are refused.
        move_mod(dir.path(), "d.modpack", 99).unwrap();
        assert_eq!(
            mod_file_names(dir.path()).unwrap().last().unwrap(),
            "d.modpack"
        );
        assert!(move_mod(dir.path(), "nope.zip", 0).is_err());
        assert!(move_mod(dir.path(), "../x.zip", 0).is_err());
        // The list follows the order too, so the engine applies in it.
        let listed: Vec<_> = list(dir.path())
            .unwrap()
            .into_iter()
            .map(|m| m.file_name)
            .collect();
        assert_eq!(listed, mod_file_names(dir.path()).unwrap());
    }

    #[test]
    fn mods_that_change_the_same_files_are_reported_with_the_later_one_winning() {
        let dir = tempfile::tempdir().unwrap();
        let both = simple_zip(&[
            ("texture/a.png", b"1"),
            ("texture/b.png", b"1"),
            ("only1.png", b"1"),
        ]);
        let other = simple_zip(&[
            ("texture/a.png", b"2"),
            ("texture/b.png", b"2"),
            ("only2.png", b"2"),
        ]);
        let third = simple_zip(&[("texture/a.png", b"3")]);
        fs::write(dir.path().join("1_first.zip"), &both).unwrap();
        fs::write(dir.path().join("2_second.zip"), &other).unwrap();
        fs::write(dir.path().join("3_third.zip"), &third).unwrap();
        let listed = list(dir.path()).unwrap();
        let found = conflicts(&listed);
        let pair = |l: &str, w: &str| {
            found
                .iter()
                .find(|c| c.loser == l && c.winner == w)
                .map(|c| c.count)
        };
        assert_eq!(pair("1_first.zip", "2_second.zip"), Some(2));
        assert_eq!(pair("1_first.zip", "3_third.zip"), Some(1));
        assert_eq!(pair("2_second.zip", "3_third.zip"), Some(1));
        assert!(found
            .iter()
            .all(|c| !c.examples.is_empty() && c.examples.len() <= 3));
        // Moving the first to the end makes it the winner of those files.
        move_mod(dir.path(), "1_first.zip", 2).unwrap();
        let again = conflicts(&list(dir.path()).unwrap());
        assert!(again
            .iter()
            .any(|c| c.loser == "2_second.zip" && c.winner == "1_first.zip"));
        // A switched-off mod conflicts with nothing.
        set_enabled(dir.path(), "3_third.zip", false).unwrap();
        let off = conflicts(&list(dir.path()).unwrap());
        assert!(off
            .iter()
            .all(|c| c.loser != "3_third.zip" && c.winner != "3_third.zip"));
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
