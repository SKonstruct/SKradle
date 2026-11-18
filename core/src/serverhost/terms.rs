//! The engine's licence and the player's recorded acceptance of it.
//!
//! Tortodrone is published under the PolyForm Noncommercial License 1.0.0; SKradle is MIT and contains none of
//! the engine. The launcher downloads the engine from the engine's own releases only after the player has
//! confirmed they understand the licence applies to them, and records that confirmation in the server folder
//! (`terms-accepted.json`). The licence and notice files from the release stay next to the jar.
//!
//! [`SUMMARY`] is the launcher's own wording, not a copy of the licence text; the licence itself is what governs.

use std::io;

use serde::{Deserialize, Serialize};

use super::fsx::write_atomic;
use super::layout::ServerLayout;

pub const LICENCE_NAME: &str = "PolyForm Noncommercial License 1.0.0";
pub const LICENCE_URL: &str = "https://polyformproject.org/licenses/noncommercial/1.0.0/";
pub const ENGINE_URL: &str = "https://github.com/N0rule/Tortodrone";

/// Bumped when what the player confirms changes, so an old acceptance does not cover new wording.
pub const TERMS_VERSION: u32 = 1;

/// What the player is asked to confirm, in the launcher's words.
pub const SUMMARY: &str = "The server engine is a separate program by its own author, not part of SKradle. \
It is source-available under the PolyForm Noncommercial License 1.0.0: you may use it for noncommercial \
purposes only, and you must keep its licence and notice files with it. SKradle downloads it from the author's \
GitHub releases and does not redistribute it. The game content it needs is derived from Spiral Knights and is \
not licensed for redistribution; keep it on this computer.";

/// A recorded acceptance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Acceptance {
    pub licence: String,
    pub url: String,
    #[serde(rename = "termsVersion")]
    pub terms_version: u32,
    /// Seconds since the Unix epoch.
    #[serde(rename = "acceptedAt")]
    pub accepted_at: u64,
}

/// The recorded acceptance, if there is a readable one for the current wording.
pub fn acceptance(layout: &ServerLayout) -> Option<Acceptance> {
    let text = std::fs::read_to_string(layout.terms_record()).ok()?;
    let record: Acceptance = serde_json::from_str(&text).ok()?;
    (record.terms_version == TERMS_VERSION && record.licence == LICENCE_NAME).then_some(record)
}

pub fn is_accepted(layout: &ServerLayout) -> bool {
    acceptance(layout).is_some()
}

/// Records that the player confirmed the terms, at `now` (seconds since the epoch).
pub fn accept(layout: &ServerLayout, now: u64) -> io::Result<Acceptance> {
    let record = Acceptance {
        licence: LICENCE_NAME.to_owned(),
        url: LICENCE_URL.to_owned(),
        terms_version: TERMS_VERSION,
        accepted_at: now,
    };
    let json = serde_json::to_string_pretty(&record).map_err(io::Error::other)?;
    write_atomic(&layout.terms_record(), json.as_bytes())?;
    Ok(record)
}

/// The current time as seconds since the epoch (0 if the clock is before it).
pub fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_is_accepted_until_recorded() {
        let dir = tempfile::tempdir().unwrap();
        let layout = ServerLayout::new(dir.path());
        assert!(!is_accepted(&layout));
        let record = accept(&layout, 1_700_000_000).unwrap();
        assert_eq!(record.accepted_at, 1_700_000_000);
        assert!(is_accepted(&layout));
        assert_eq!(acceptance(&layout), Some(record));
    }

    #[test]
    fn an_acceptance_of_other_wording_or_a_damaged_file_does_not_count() {
        let dir = tempfile::tempdir().unwrap();
        let layout = ServerLayout::new(dir.path());
        std::fs::write(
            layout.terms_record(),
            r#"{"licence":"PolyForm Noncommercial License 1.0.0","url":"u","termsVersion":0,"acceptedAt":1}"#,
        )
        .unwrap();
        assert!(!is_accepted(&layout));
        std::fs::write(layout.terms_record(), "not json").unwrap();
        assert!(!is_accepted(&layout));
        std::fs::write(
            layout.terms_record(),
            r#"{"licence":"Other","url":"u","termsVersion":1,"acceptedAt":1}"#,
        )
        .unwrap();
        assert!(!is_accepted(&layout));
    }

    #[test]
    fn the_summary_names_the_licence_and_the_noncommercial_limit() {
        assert!(SUMMARY.contains("Noncommercial"));
        assert!(SUMMARY.contains("not licensed for redistribution"));
    }
}
