//! Tags a mod earns from what it contains. Nothing is typed in by hand: they come from the parts and files the scan
//! already found, so the same mod always has the same tags.

use super::model::{Container, Kind, Mod, Part};

/// What a mod changes in the game.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Tag {
    /// A `.modpack`: several mods in one file.
    Modpack,
    /// A class mod (merged into `code/config.jar`).
    Code,
    /// Image files (textures, icons, interface art).
    Textures,
    /// Sound files.
    Audio,
    /// Locale changes: replaced game text.
    Text,
    /// Loose game files that are none of the above (models, animations, data).
    Resources,
    /// The mod could not be read.
    Broken,
}

impl Tag {
    /// A stable id for the interface (not a display word).
    pub fn id(self) -> &'static str {
        match self {
            Tag::Modpack => "modpack",
            Tag::Code => "code",
            Tag::Textures => "textures",
            Tag::Audio => "audio",
            Tag::Text => "text",
            Tag::Resources => "resources",
            Tag::Broken => "broken",
        }
    }
}

const IMAGE: &[&str] = &["png", "jpg", "jpeg", "tga", "dds", "bmp", "gif"];
const SOUND: &[&str] = &["ogg", "wav", "mp3", "mid", "midi"];

fn extension(path: &str) -> String {
    path.rsplit('/')
        .next()
        .and_then(|name| name.rsplit_once('.'))
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default()
}

fn part_tags(part: &Part, into: &mut Vec<Tag>) {
    let mut add = |t: Tag| {
        if !into.contains(&t) {
            into.push(t);
        }
    };
    if part.kind == Kind::Class {
        add(Tag::Code);
    }
    if part.info.locale_changes() > 0 {
        add(Tag::Text);
    }
    if part.kind == Kind::Resource {
        let mut other = false;
        for f in part.files.iter().filter(|f| !f.protected) {
            let ext = extension(&f.path);
            if IMAGE.contains(&ext.as_str()) {
                add(Tag::Textures);
            } else if SOUND.contains(&ext.as_str()) {
                add(Tag::Audio);
            } else {
                other = true;
            }
        }
        if other {
            add(Tag::Resources);
        }
    }
}

/// The tags of a scanned mod, in a fixed order.
pub fn tags_of(m: &Mod) -> Vec<Tag> {
    let mut tags = Vec::new();
    if m.container == Container::Modpack {
        tags.push(Tag::Modpack);
        if m.info.locale_changes() > 0 {
            tags.push(Tag::Text);
        }
    }
    for part in &m.parts {
        part_tags(part, &mut tags);
    }
    tags.sort();
    tags.dedup();
    tags
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jar::testutil::simple_zip;
    use std::fs;

    fn scan(name: &str, bytes: Vec<u8>) -> Mod {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(name);
        fs::write(&path, bytes).unwrap();
        Mod::open(&path).unwrap()
    }

    fn ids(m: &Mod) -> Vec<&'static str> {
        tags_of(m).into_iter().map(Tag::id).collect()
    }

    #[test]
    fn textures_audio_and_other_resources() {
        let m = scan(
            "a.zip",
            simple_zip(&[("ui/a.PNG", b"1"), ("snd/b.ogg", b"2")]),
        );
        assert_eq!(ids(&m), ["textures", "audio"]);
        let m = scan("b.zip", simple_zip(&[("item/x/model.dat", b"1")]));
        assert_eq!(ids(&m), ["resources"]);
        let m = scan(
            "c.zip",
            simple_zip(&[("a.png", b"1"), ("item/x/model.dat", b"1")]),
        );
        assert_eq!(ids(&m), ["textures", "resources"]);
    }

    #[test]
    fn class_and_locale_mods() {
        let json = br#"{"mod":{"type":"class","locale":{"messages.properties":{"k":"v"}}}}"#;
        let m = scan(
            "c.jar",
            simple_zip(&[("mod.json", json), ("a/B.class", b"x")]),
        );
        assert_eq!(ids(&m), ["code", "text"]);
    }

    #[test]
    fn a_modpack_has_its_own_tag_and_those_of_its_parts() {
        let inner = simple_zip(&[("a.png", b"1")]);
        let class = simple_zip(&[("mod.json", br#"{"type":"class"}"#), ("A.class", b"x")]);
        let pack = simple_zip(&[("one.zip", &inner), ("two.jar", &class)]);
        let m = scan("p.modpack", pack);
        assert_eq!(m.parts.len(), 2);
        assert_eq!(ids(&m), ["modpack", "code", "textures"]);
    }

    #[test]
    fn metadata_files_alone_earn_no_tag() {
        let m = scan(
            "e.zip",
            simple_zip(&[("mod.json", b"{}"), ("mod.png", b"x")]),
        );
        assert!(tags_of(&m).is_empty());
    }
}
