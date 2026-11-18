//! The updater's `launcher.log` in the game folder: the only place it reports progress outside its own window.
//! The log carries phase messages ("Application requires update.", "Downloading resource ...") but no byte
//! counts, so it can say what the updater is doing, not how far it is.

use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use super::{Phase, Step};

/// What a log line tells about the updater's phase.
pub fn classify(line: &str) -> Option<(Phase, Step)> {
    // Lines look like `2026/11/17 12:05:29:022 INFO Application requires update.`
    let message = ["INFO ", "WARNING ", "SEVERE "]
        .iter()
        .find_map(|level| line.split_once(level).map(|(_, rest)| rest))
        .unwrap_or(line)
        .trim();
    if message.starts_with("Application requires update") || message.starts_with("Downloading") {
        return Some((Phase::Updating, Step::Downloading));
    }
    // The updater is about to start, or has started, the game.
    if message.starts_with("Launching without update")
        || message.starts_with("Install completed")
        || message.starts_with("Nothing to install")
        || (message.starts_with("Running ") && !message.contains("headless"))
    {
        return Some((Phase::Launching, Step::Checking));
    }
    None
}

/// Follows a log file from where it was when the launch began.
pub struct LogTail {
    path: PathBuf,
    offset: u64,
    partial: String,
}

impl LogTail {
    /// Starts at the end of the file as it is now (a missing file counts as empty).
    pub fn at_end(path: &Path) -> Self {
        let offset = std::fs::metadata(path).map_or(0, |m| m.len());
        Self {
            path: path.to_path_buf(),
            offset,
            partial: String::new(),
        }
    }

    /// The complete lines written since the last call. A log that was truncated is read again from the start.
    pub fn read_new(&mut self) -> Vec<String> {
        let Ok(mut file) = std::fs::File::open(&self.path) else {
            return Vec::new();
        };
        let len = file.metadata().map_or(0, |m| m.len());
        if len < self.offset {
            self.offset = 0;
            self.partial.clear();
        }
        if len == self.offset || file.seek(SeekFrom::Start(self.offset)).is_err() {
            return Vec::new();
        }
        let mut bytes = Vec::new();
        // At most 1 MB per call; the rest comes on the next one.
        if file.take(1 << 20).read_to_end(&mut bytes).is_err() {
            return Vec::new();
        }
        self.offset += bytes.len() as u64;
        self.partial.push_str(&String::from_utf8_lossy(&bytes));
        let mut lines: Vec<String> = self
            .partial
            .split('\n')
            .map(|l| l.trim_end_matches('\r').to_owned())
            .collect();
        self.partial = lines.pop().unwrap_or_default();
        lines.retain(|l| !l.is_empty());
        lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn classifies_the_updater_phases() {
        let line = |m: &str| format!("2026/11/17 12:05:29:022 INFO {m}");
        assert_eq!(
            classify(&line("Application requires update.")),
            Some((Phase::Updating, Step::Downloading))
        );
        assert_eq!(
            classify(&line("Downloading resource code/a.jar")),
            Some((Phase::Updating, Step::Downloading))
        );
        assert_eq!(
            classify(&line("Nothing to install.")),
            Some((Phase::Launching, Step::Checking))
        );
        assert_eq!(
            classify(&line("Running /game/java_vm/bin/java -cp x")),
            Some((Phase::Launching, Step::Checking))
        );
        assert_eq!(
            classify(&line(
                "Running in headless JVM, will attempt to operate without UI."
            )),
            None
        );
        assert_eq!(classify(&line("Getdown starting [version=1.8.10]")), None);
        assert_eq!(
            classify("Downloading resource"),
            Some((Phase::Updating, Step::Downloading))
        );
    }

    #[test]
    fn tail_returns_only_new_complete_lines() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("launcher.log");
        std::fs::write(&path, "old line\n").unwrap();
        let mut tail = LogTail::at_end(&path);
        assert!(tail.read_new().is_empty());
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        f.write_all(b"one\r\ntwo\nthr").unwrap();
        assert_eq!(tail.read_new(), ["one", "two"]);
        f.write_all(b"ee\n").unwrap();
        assert_eq!(tail.read_new(), ["three"]);
    }

    #[test]
    fn tail_survives_truncation_and_a_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("launcher.log");
        let mut tail = LogTail::at_end(&path);
        assert!(tail.read_new().is_empty());
        std::fs::write(&path, "a long first line\n").unwrap();
        assert_eq!(tail.read_new(), ["a long first line"]);
        std::fs::write(&path, "new\n").unwrap();
        assert_eq!(tail.read_new(), ["new"]);
    }
}
