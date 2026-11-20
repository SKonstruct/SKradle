//! What the Mods page shows and does, as plain Rust: the mods folder, the list the page draws (read from the core's
//! scan), adding files, removing and switching mods, and the screenshot fixtures. `bridge/mods_model.rs` only moves
//! this between threads and Qt properties.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Serialize;
use skradle_core::mods::import::import_file;
use skradle_core::mods::tags::{tags_of, Tag};
use skradle_core::mods::{self, Container, InstalledMod};

use crate::discovery::local_path_from_url_text;

/// The folder the player's mods live in, the one applied at launch: `SKRADLE_MODS_DIR` when set (tests), else
/// `<data dir>/SKradle/mods`.
pub fn mods_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("SKRADLE_MODS_DIR").filter(|d| !d.is_empty()) {
        return Some(PathBuf::from(dir));
    }
    dirs::data_dir().map(|d| d.join("SKradle").join("mods"))
}

/// `file:///...` for a local path, percent-encoded, with the drive letter form on Windows.
pub fn file_url(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    let mut out = String::from("file://");
    if !text.starts_with('/') {
        out.push('/');
    }
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || b"/-_.~:".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// One mod as the page draws it.
#[derive(Debug, Serialize, PartialEq)]
pub struct ItemView {
    pub file: String,
    pub name: String,
    pub version: String,
    pub author: String,
    pub description: String,
    /// `zip`, `jar` or `modpack`.
    pub container: &'static str,
    pub tags: Vec<&'static str>,
    /// Mods inside (1 unless it is a modpack).
    pub parts: usize,
    /// Files it would write.
    pub files: usize,
    pub size: u64,
    pub enabled: bool,
    /// Why it cannot be used; `None` for a mod that scanned.
    pub error: Option<String>,
    pub warnings: Vec<String>,
    /// A class mod built for another game version than the install.
    pub incompatible: bool,
    /// Lower-case text the search box matches.
    pub search: String,
}

fn container_id(c: Container) -> &'static str {
    match c {
        Container::Zip => "zip",
        Container::Jar => "jar",
        Container::Modpack => "modpack",
    }
}

/// The view of one scanned entry. `game_version` is the install's version, when known.
pub fn item_view(dir: &Path, m: &InstalledMod, game_version: Option<&str>) -> ItemView {
    let size = std::fs::symlink_metadata(dir.join(&m.file_name)).map_or(0, |md| md.len());
    let container = Container::from_file_name(&m.file_name).map_or("zip", container_id);
    let mut v = ItemView {
        file: m.file_name.clone(),
        name: m.file_name.clone(),
        version: String::new(),
        author: String::new(),
        description: String::new(),
        container,
        tags: Vec::new(),
        parts: 0,
        files: 0,
        size,
        enabled: m.enabled,
        error: None,
        warnings: Vec::new(),
        incompatible: false,
        search: String::new(),
    };
    match &m.parsed {
        Err(e) => {
            v.tags = vec![Tag::Broken.id()];
            v.error = Some(e.to_string());
        }
        Ok(scanned) => {
            let info = scanned.pack_info().unwrap_or(&scanned.info);
            v.name = scanned.display_name().to_owned();
            v.version = info.version.clone().unwrap_or_default();
            v.author = info.author.clone().unwrap_or_default();
            v.description = info.description.clone().unwrap_or_default();
            v.tags = tags_of(scanned).into_iter().map(Tag::id).collect();
            v.parts = scanned.parts.len();
            v.files = scanned
                .parts
                .iter()
                .map(|p| p.applicable_files().count())
                .sum();
            v.warnings = scanned.warnings.clone();
            for p in &scanned.parts {
                v.warnings.extend(p.warnings.iter().cloned());
            }
            v.incompatible = scanned
                .parts
                .iter()
                .any(|p| !p.compatible_with(game_version));
        }
    }
    v.search = format!(
        "{} {} {} {} {}",
        v.name,
        v.file,
        v.author,
        v.description,
        v.tags.join(" ")
    )
    .to_lowercase();
    v
}

/// Tag id and how many mods have it, in a fixed order.
#[derive(Debug, Serialize, PartialEq)]
pub struct TagCount {
    pub id: &'static str,
    pub count: usize,
}

/// Only tags that exist among the mods.
pub fn tag_counts(items: &[ItemView]) -> Vec<TagCount> {
    let mut counts: BTreeMap<usize, (&'static str, usize)> = BTreeMap::new();
    let order = [
        Tag::Modpack,
        Tag::Code,
        Tag::Textures,
        Tag::Audio,
        Tag::Text,
        Tag::Resources,
        Tag::Broken,
    ];
    for item in items {
        for t in &item.tags {
            if let Some(i) = order.iter().position(|o| o.id() == *t) {
                counts.entry(i).or_insert((order[i].id(), 0)).1 += 1;
            }
        }
    }
    counts
        .into_values()
        .map(|(id, count)| TagCount { id, count })
        .collect()
}

/// The folder as the page shows it.
#[derive(Debug, Default, Serialize)]
pub struct Listing {
    pub items: Vec<ItemView>,
    pub tags: Vec<TagCount>,
    /// Why the folder could not be read (a damaged state file, no permission); empty items then.
    pub error: Option<String>,
}

/// Scans the folder. A missing folder is an empty list.
pub fn build_listing(dir: &Path, game_version: Option<&str>) -> Listing {
    match mods::list_mods(dir) {
        Ok(found) => {
            let items: Vec<ItemView> = found
                .iter()
                .map(|m| item_view(dir, m, game_version))
                .collect();
            Listing {
                tags: tag_counts(&items),
                items,
                error: None,
            }
        }
        Err(e) => Listing {
            error: Some(e.to_string()),
            ..Listing::default()
        },
    }
}

/// What an action did, for the page's toast and report.
#[derive(Debug, Default, Serialize, PartialEq)]
pub struct Outcome {
    /// `add`, `enable`, `disable` or `remove`.
    pub kind: &'static str,
    pub ok: usize,
    /// Files added under another name: (name asked for, name used).
    pub renamed: Vec<(String, String)>,
    /// File and why it was not handled.
    pub failed: Vec<(String, String)>,
}

/// Switches each named mod. The first failure of the state file stops (every other name would fail the same way).
pub fn set_enabled_all(dir: &Path, files: &[String], enabled: bool) -> Outcome {
    let mut out = Outcome {
        kind: if enabled { "enable" } else { "disable" },
        ..Outcome::default()
    };
    for f in files {
        match mods::set_enabled(dir, f, enabled) {
            Ok(()) => out.ok += 1,
            Err(e) => out.failed.push((f.clone(), e.to_string())),
        }
    }
    out
}

pub fn remove_all(dir: &Path, files: &[String]) -> Outcome {
    let mut out = Outcome {
        kind: "remove",
        ..Outcome::default()
    };
    for f in files {
        match mods::remove(dir, f) {
            Ok(()) => out.ok += 1,
            Err(e) => out.failed.push((f.clone(), e.to_string())),
        }
    }
    out
}

/// Adds every path (a `file://` URL or a plain path, as a drop or a dialog gives it) after validating it.
pub fn add_all(dir: &Path, sources: &[String]) -> Outcome {
    let mut out = Outcome {
        kind: "add",
        ..Outcome::default()
    };
    for s in sources {
        let path = local_path_from_url_text(s);
        let shown = path
            .file_name()
            .map_or_else(|| s.clone(), |n| n.to_string_lossy().into_owned());
        if path.as_os_str().is_empty() {
            out.failed
                .push((shown, "this is not a file on this computer".to_owned()));
            continue;
        }
        match import_file(&path, dir) {
            Ok(done) => {
                out.ok += 1;
                if done.renamed {
                    out.renamed.push((shown, done.file_name));
                }
            }
            Err(why) => out.failed.push((shown, why.to_string())),
        }
    }
    out
}

/// Screenshot and self-test runs: a folder with a few real mods in it, never the player's own.
pub fn write_fixtures(dir: &Path) -> std::io::Result<()> {
    use skradle_core::jar::Jar;
    fn zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut jar = Jar::default();
        for (name, data) in entries {
            let _ = jar.set(name, data.to_vec());
        }
        jar.to_bytes().unwrap_or_default()
    }
    std::fs::create_dir_all(dir)?;
    let write = |name: &str, bytes: Vec<u8>| std::fs::write(dir.join(name), bytes);
    write(
        "Sharp HUD.zip",
        zip(&[
            (
                "mod.json",
                br#"{"mod":{"name":"Sharp HUD","author":"Fixture Author","version":"1.4","description":"Draws the interface at full resolution."}}"#,
            ),
            ("ui/hud.png", b"png"),
        ]),
    )?;
    write(
        "Damage Numbers.jar",
        zip(&[
            (
                "mod.json",
                br#"{"mod":{"name":"Damage Numbers","author":"Fixture Author","version":"0.9","type":"class","description":"Larger, clearer hit numbers."}}"#,
            ),
            ("com/example/Numbers.class", b"class"),
        ]),
    )?;
    write(
        "Plain Speech.zip",
        zip(&[(
            "mod.json",
            br#"{"mod":{"name":"Plain Speech","version":"2.0","description":"Shorter wording in menus.","locale":{"messages.properties":{"menu.play":"Play"}}}}"#,
        )]),
    )?;
    write("Fanfare.zip", zip(&[("sound/fanfare.ogg", b"ogg")]))?;
    let inner_a = zip(&[("ui/a.png", b"1")]);
    let inner_b = zip(&[("sound/b.ogg", b"2")]);
    write(
        "Starter Pack.modpack",
        zip(&[
            ("mod.json", br#"{"mod":{"name":"Starter Pack","author":"Fixture Author","description":"A small collection."}}"#),
            ("one.zip", &inner_a),
            ("two.zip", &inner_b),
        ]),
    )?;
    write("Broken Mod.zip", b"this is not a zip file".to_vec())?;
    mods::set_enabled(dir, "Fanfare.zip", false)?;
    mods::set_enabled(dir, "Plain Speech.zip", false)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_urls_are_encoded() {
        assert_eq!(
            file_url(Path::new("/home/me/My Mods")),
            "file:///home/me/My%20Mods"
        );
        assert_eq!(
            file_url(Path::new("C:\\Users\\me\\mods")),
            "file:///C:/Users/me/mods"
        );
        // The URL reads back as the same path.
        assert_eq!(
            local_path_from_url_text(&file_url(Path::new("/a b/ü"))),
            PathBuf::from("/a b/ü")
        );
    }

    #[test]
    fn the_fixture_folder_lists_with_tags_and_errors() {
        let dir = tempfile::tempdir().unwrap();
        write_fixtures(dir.path()).unwrap();
        let l = build_listing(dir.path(), Some("20261110044614"));
        assert!(l.error.is_none());
        let by = |f: &str| l.items.iter().find(|i| i.file == f).unwrap();
        assert_eq!(by("Sharp HUD.zip").tags, ["textures"]);
        assert_eq!(by("Sharp HUD.zip").author, "Fixture Author");
        assert_eq!(by("Damage Numbers.jar").tags, ["code"]);
        assert_eq!(by("Plain Speech.zip").tags, ["text"]);
        assert!(!by("Plain Speech.zip").enabled);
        assert_eq!(by("Starter Pack.modpack").parts, 2);
        assert_eq!(
            by("Starter Pack.modpack").tags,
            ["modpack", "textures", "audio"]
        );
        let broken = by("Broken Mod.zip");
        assert_eq!(broken.tags, ["broken"]);
        assert!(broken.error.is_some());
        let counts: Vec<_> = l.tags.iter().map(|t| (t.id, t.count)).collect();
        assert_eq!(
            counts,
            [
                ("modpack", 1),
                ("code", 1),
                ("textures", 2),
                ("audio", 2),
                ("text", 1),
                ("broken", 1)
            ]
        );
    }

    #[test]
    fn tags_exist_only_for_what_is_installed() {
        let dir = tempfile::tempdir().unwrap();
        assert!(build_listing(dir.path(), None).tags.is_empty());
        assert!(build_listing(&dir.path().join("missing"), None)
            .items
            .is_empty());
    }

    #[test]
    fn a_class_mod_for_another_version_is_flagged() {
        let dir = tempfile::tempdir().unwrap();
        write_fixtures(dir.path()).unwrap();
        // Rewrite the class mod with a version.
        let mut jar = skradle_core::jar::Jar::default();
        jar.set(
            "mod.json",
            br#"{"type":"class","pxVersion":"111"}"#.to_vec(),
        )
        .unwrap();
        jar.set("A.class", b"x".to_vec()).unwrap();
        std::fs::write(
            dir.path().join("Damage Numbers.jar"),
            jar.to_bytes().unwrap(),
        )
        .unwrap();
        let l = build_listing(dir.path(), Some("222"));
        assert!(
            l.items
                .iter()
                .find(|i| i.file == "Damage Numbers.jar")
                .unwrap()
                .incompatible
        );
        let l = build_listing(dir.path(), None);
        assert!(
            !l.items
                .iter()
                .find(|i| i.file == "Damage Numbers.jar")
                .unwrap()
                .incompatible
        );
    }

    #[test]
    fn adding_removing_and_switching() {
        let src = tempfile::tempdir().unwrap();
        let dir = tempfile::tempdir().unwrap();
        write_fixtures(src.path()).unwrap();
        std::fs::write(src.path().join("notes.txt"), b"x").unwrap();
        let sources: Vec<String> = [
            "Sharp HUD.zip",
            "Broken Mod.zip",
            "notes.txt",
            "Sharp HUD.zip",
        ]
        .iter()
        .map(|n| file_url(&src.path().join(n)))
        .collect();
        let out = add_all(dir.path(), &sources);
        assert_eq!(out.ok, 2);
        assert_eq!(
            out.renamed,
            [("Sharp HUD.zip".into(), "Sharp HUD (2).zip".into())]
        );
        let failed: Vec<_> = out.failed.iter().map(|f| f.0.as_str()).collect();
        assert_eq!(failed, ["Broken Mod.zip", "notes.txt"]);

        let files = vec!["Sharp HUD.zip".to_owned(), "missing.zip".to_owned()];
        let off = set_enabled_all(dir.path(), &files, false);
        assert_eq!(off.ok, 2, "a disabled entry for a missing mod is harmless");
        let listing = build_listing(dir.path(), None);
        assert!(
            !listing
                .items
                .iter()
                .find(|i| i.file == "Sharp HUD.zip")
                .unwrap()
                .enabled
        );
        let gone = remove_all(dir.path(), &files);
        assert_eq!(gone.ok, 1);
        assert_eq!(gone.failed.len(), 1);
        assert_eq!(build_listing(dir.path(), None).items.len(), 1);
    }

    #[test]
    fn a_damaged_state_file_is_an_error_not_an_empty_list() {
        let dir = tempfile::tempdir().unwrap();
        write_fixtures(dir.path()).unwrap();
        std::fs::write(dir.path().join(".skradle-mods.json"), b"{ nope").unwrap();
        let l = build_listing(dir.path(), None);
        assert!(l.error.is_some() && l.items.is_empty());
    }
}
