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
        return Some((Phase::Launching, Step::Preparing));
    }
    None
}

/// How much of the start of the file identifies it: a new run rewrites the log from the top, so the first bytes
/// change even when the new log has already grown past the old length.
const FINGERPRINT: usize = 64;
/// A line longer than this without a newline is dropped (the log is text; this is a runaway).
const MAX_PARTIAL: usize = 64 * 1024;

/// Follows a log file from where it was when the launch began.
pub struct LogTail {
    path: PathBuf,
    offset: u64,
    /// The first bytes of the file as last seen, to notice that the file was started over.
    head: Vec<u8>,
    /// The incomplete last line, kept as bytes: a character split across two reads is decoded once it is whole.
    partial: Vec<u8>,
}

fn read_head(file: &mut std::fs::File, len: usize) -> Option<Vec<u8>> {
    let mut head = vec![0u8; len];
    file.seek(SeekFrom::Start(0)).ok()?;
    file.read_exact(&mut head).ok()?;
    Some(head)
}

impl LogTail {
    /// Starts at the end of the file as it is now (a missing file counts as empty).
    pub fn at_end(path: &Path) -> Self {
        let mut tail = Self {
            path: path.to_path_buf(),
            offset: 0,
            head: Vec::new(),
            partial: Vec::new(),
        };
        if let Ok(mut file) = std::fs::File::open(path) {
            let len = file.metadata().map_or(0, |m| m.len());
            tail.offset = len;
            tail.head =
                read_head(&mut file, len.min(FINGERPRINT as u64) as usize).unwrap_or_default();
        }
        tail
    }

    fn start_over(&mut self) {
        self.offset = 0;
        self.head.clear();
        self.partial.clear();
    }

    /// The complete lines written since the last call. A log that was truncated or rewritten is read again from the
    /// start.
    pub fn read_new(&mut self) -> Vec<String> {
        let Ok(mut file) = std::fs::File::open(&self.path) else {
            return Vec::new();
        };
        let len = file.metadata().map_or(0, |m| m.len());
        let rewritten = len < self.offset
            || (!self.head.is_empty()
                && read_head(&mut file, self.head.len()).is_none_or(|now| now != self.head));
        if rewritten {
            self.start_over();
        }
        if len == self.offset || file.seek(SeekFrom::Start(self.offset)).is_err() {
            return Vec::new();
        }
        let mut bytes = Vec::new();
        // At most 1 MB per call; the rest comes on the next one.
        if file.by_ref().take(1 << 20).read_to_end(&mut bytes).is_err() {
            return Vec::new();
        }
        self.offset += bytes.len() as u64;
        if self.head.len() < FINGERPRINT {
            let want = self.offset.min(FINGERPRINT as u64) as usize;
            if let Some(head) = read_head(&mut file, want) {
                self.head = head;
            }
        }
        self.partial.extend_from_slice(&bytes);
        let mut lines = Vec::new();
        while let Some(end) = self.partial.iter().position(|b| *b == b'\n') {
            let line: Vec<u8> = self.partial.drain(..=end).collect();
            let text = String::from_utf8_lossy(&line[..line.len() - 1]);
            let text = text.trim_end_matches('\r');
            if !text.is_empty() {
                lines.push(text.to_owned());
            }
        }
        if self.partial.len() > MAX_PARTIAL {
            self.partial.clear();
        }
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
            Some((Phase::Launching, Step::Preparing))
        );
        assert_eq!(
            classify(&line("Running /game/java_vm/bin/java -cp x")),
            Some((Phase::Launching, Step::Preparing))
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

    #[test]
    fn a_rewritten_log_that_grew_past_the_old_length_is_read_from_the_top() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("launcher.log");
        std::fs::write(&path, "2026/11/17 11:00:00:000 INFO Short old run\n").unwrap();
        let mut tail = LogTail::at_end(&path);
        // The new run overwrites the file and writes MORE than the old one before the first poll.
        std::fs::write(
            &path,
            "2026/11/17 12:00:00:000 INFO Application requires update.\n2026/11/17 12:00:01:000 INFO Downloading resource a\n",
        )
        .unwrap();
        let lines = tail.read_new();
        assert_eq!(lines.len(), 2, "{lines:?}");
        assert!(lines[0].contains("Application requires update."));
    }

    #[test]
    fn a_character_split_across_two_reads_is_decoded_whole() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("launcher.log");
        std::fs::write(&path, "").unwrap();
        let mut tail = LogTail::at_end(&path);
        let text = "caf\u{e9}\n".as_bytes();
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        f.write_all(&text[..4]).unwrap(); // "caf" and the first byte of the accent
        assert!(tail.read_new().is_empty());
        f.write_all(&text[4..]).unwrap();
        assert_eq!(tail.read_new(), ["caf\u{e9}"]);
    }

    #[test]
    fn a_runaway_line_without_a_newline_is_capped() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("launcher.log");
        std::fs::write(&path, "").unwrap();
        let mut tail = LogTail::at_end(&path);
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        f.write_all(&vec![b'x'; MAX_PARTIAL + 10]).unwrap();
        assert!(tail.read_new().is_empty());
        assert!(tail.partial.len() <= MAX_PARTIAL);
        f.write_all(b"\nnext\n").unwrap();
        assert_eq!(tail.read_new(), ["next"]);
    }
}
