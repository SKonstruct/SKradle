//! What SKradle knows locally about a newer game version, so a launch never waits on the network.
//!
//! A launch does not ask the game's server anything. After a standalone install was started directly, a background
//! check fetches the server's `getdown.txt` (short timeouts) and records the newest version in a small marker file in
//! SKradle's data folder; the next launch reads that marker and compares it with the installed version. Steam keeps a
//! Steam install up to date itself, so it is never checked.

use std::path::{Path, PathBuf};
use std::time::Duration;

use skradle_core::getdown_txt::GetdownConfig;
use skradle_core::install::LATEST_GETDOWN_TXT_URL;
use skradle_core::{log, net, servers};

const FILE: &str = "game-update-pending.txt";

fn marker(dir: &Path) -> PathBuf {
    dir.join(FILE)
}

/// The newest game version an earlier background check found, if any.
pub fn read(dir: &Path) -> Option<String> {
    let text = std::fs::read_to_string(marker(dir)).ok()?;
    let version = text.trim();
    (!version.is_empty()).then(|| version.to_owned())
}

/// True when the recorded version differs from the installed one (`local_getdown_txt`).
pub fn is_pending(local_getdown_txt: &str, recorded: Option<&str>) -> bool {
    recorded.is_some_and(|v| servers::update_pending(local_getdown_txt, &format!("version = {v}\n")))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Recorded {
    /// The server has a newer version than the installed one (the marker was written).
    Pending(String),
    /// The installed version is the newest (the marker was removed).
    Current,
    /// The answer named no version: nothing changed.
    Unknown,
}

/// Records what the server's `getdown.txt` says against the installed one.
pub fn record(dir: &Path, local_getdown_txt: &str, remote_getdown_txt: &str) -> Recorded {
    let Some(remote) = GetdownConfig::parse(remote_getdown_txt).version else {
        return Recorded::Unknown;
    };
    if servers::update_pending(local_getdown_txt, remote_getdown_txt) {
        let _ = std::fs::create_dir_all(dir);
        let _ = std::fs::write(marker(dir), format!("{remote}\n"));
        Recorded::Pending(remote)
    } else {
        let _ = std::fs::remove_file(marker(dir));
        Recorded::Current
    }
}

/// Checks the server on a background thread; the launch never waits for it. The result is only logged and recorded.
pub fn check_in_background(dir: PathBuf, local_getdown_txt: String) {
    let _ = std::thread::Builder::new()
        .name("skradle-update-check".into())
        .spawn(move || {
            let client = net::Client::with_timeouts(Duration::from_secs(4), Duration::from_secs(8));
            match client.get_text(LATEST_GETDOWN_TXT_URL, 256 * 1024) {
                Ok(remote) => match record(&dir, &local_getdown_txt, &remote) {
                    Recorded::Pending(v) => log::info(
                        "update",
                        format!("background check: game version {v} is available; the next launch updates the game"),
                    ),
                    Recorded::Current => log::info("update", "background check: the game is up to date"),
                    Recorded::Unknown => log::warn("update", "background check: the answer named no version"),
                },
                Err(e) => log::info("update", format!("background check failed: {e}")),
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_newer_remote_is_recorded_and_read_back_then_cleared_when_current() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(read(dir.path()), None);
        assert_eq!(
            record(dir.path(), "version = 1\n", "version = 2\n"),
            Recorded::Pending("2".into())
        );
        assert_eq!(read(dir.path()).as_deref(), Some("2"));
        assert!(is_pending("version = 1\n", read(dir.path()).as_deref()));
        // After the game updated, the marker no longer counts.
        assert!(!is_pending("version = 2\n", read(dir.path()).as_deref()));
        assert_eq!(record(dir.path(), "version = 2\n", "version = 2\n"), Recorded::Current);
        assert_eq!(read(dir.path()), None);
    }

    #[test]
    fn an_answer_without_a_version_changes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        record(dir.path(), "version = 1\n", "version = 2\n");
        assert_eq!(record(dir.path(), "version = 1\n", "garbage"), Recorded::Unknown);
        assert_eq!(read(dir.path()).as_deref(), Some("2"));
    }

    #[test]
    fn nothing_known_is_not_pending() {
        assert!(!is_pending("version = 1\n", None));
        assert!(!is_pending("nothing", Some("2")));
    }
}
