//! `ReleaseNotes` (QML singleton `ReleaseNotes`): the changelog, bundled into the binary at build time and parsed
//! once, for the offline release-notes viewer. `include_str!` makes cargo rebuild when `CHANGELOG.md` changes.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        /// The versions as JSON: `[{title, date, intro: [html], sections: [{name, items: [{html, bullet, level}]}]}]`,
        /// newest first, text as escaped Qt rich text (`<b> <i> <tt> <a href>`). "[]" when the file has none.
        #[qproperty(QString, json, READ, CONSTANT)]
        type ReleaseNotes = super::ReleaseNotesRust;
    }
}

use cxx_qt_lib::QString;

/// The repository's changelog, embedded at build time.
const CHANGELOG: &str = include_str!("../../../CHANGELOG.md");

/// The viewer's data.
pub fn notes_json() -> String {
    serde_json::to_string(&skradle_core::changelog::parse(CHANGELOG))
        .unwrap_or_else(|_| "[]".into())
}

pub struct ReleaseNotesRust {
    json: QString,
}

impl Default for ReleaseNotesRust {
    fn default() -> Self {
        Self {
            json: QString::from(notes_json().as_str()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bundled_changelog_reaches_the_viewer_as_json() {
        let json: serde_json::Value = serde_json::from_str(&notes_json()).unwrap();
        let versions = json.as_array().unwrap();
        assert!(!versions.is_empty());
        assert_eq!(versions[0]["title"], "Unreleased");
        assert!(!versions[0]["sections"].as_array().unwrap().is_empty());
        // Nothing in the embedded text can carry a script or a non-https link.
        assert!(!json.to_string().contains("<script"));
        assert!(!json.to_string().contains("href=\\\"http:"));
    }
}
