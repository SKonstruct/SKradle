//! The game's `extra.txt`: extra JVM arguments, one per line, read by the game's updater when it starts the game.
//! The file is not covered by the updater's checksums, so the launcher may edit it. It only touches the lines it
//! owns and leaves everything else (the player's own arguments, line endings) as it was.

use std::io;
use std::path::Path;

/// True for the lines SKradle's graphics module writes: the agent and its `-Dskgfx.*` settings.
pub fn is_skgfx_line(line: &str) -> bool {
    let line = line.trim();
    line.starts_with("-Dskgfx.")
        || (line.starts_with("-javaagent:") && line.contains("skgfx-agent"))
}

/// Replaces every line `owns` claims with `managed` (appended at the end) and keeps all other lines in order.
/// Calling it again with the same input gives the same text. The file's line ending style is kept.
pub fn apply(existing: &str, managed: &[String], owns: impl Fn(&str) -> bool) -> String {
    let newline = if existing.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let mut lines: Vec<&str> = existing.lines().filter(|line| !owns(line)).collect();
    while lines.last().is_some_and(|line| line.trim().is_empty()) {
        lines.pop();
    }
    let mut out = String::new();
    for line in lines
        .iter()
        .copied()
        .chain(managed.iter().map(String::as_str))
    {
        out.push_str(line);
        out.push_str(newline);
    }
    out
}

/// Reads `extra.txt` (a missing file counts as empty), applies `managed` and writes it back through a temporary
/// file so a crash cannot leave a half-written one. Removes the file when nothing is left in it.
pub fn update_file(path: &Path, managed: &[String], owns: impl Fn(&str) -> bool) -> io::Result<()> {
    let existing = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e),
    };
    let updated = apply(&existing, managed, owns);
    if updated == existing {
        return Ok(());
    }
    if updated.is_empty() {
        return match std::fs::remove_file(path) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        };
    }
    crate::jar::write_atomic(path, updated.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn managed() -> Vec<String> {
        vec![
            "-javaagent:/m/skgfx-agent.jar".into(),
            "-Dskgfx.scale=1.5".into(),
        ]
    }

    #[test]
    fn adds_managed_lines_and_keeps_the_players_own() {
        let out = apply("-Xmx4096M\n-Xms2048M\n", &managed(), is_skgfx_line);
        assert_eq!(
            out,
            "-Xmx4096M\n-Xms2048M\n-javaagent:/m/skgfx-agent.jar\n-Dskgfx.scale=1.5\n"
        );
    }

    #[test]
    fn replaces_old_managed_lines_and_is_idempotent() {
        let old = "-Xmx1g\n-javaagent:/old/skgfx-agent.jar\n-Dskgfx.scale=2\n-Dskgfx.sharpen=0.4\n";
        let once = apply(old, &managed(), is_skgfx_line);
        assert_eq!(
            once,
            "-Xmx1g\n-javaagent:/m/skgfx-agent.jar\n-Dskgfx.scale=1.5\n"
        );
        assert_eq!(apply(&once, &managed(), is_skgfx_line), once);
    }

    #[test]
    fn leaves_other_javaagent_lines_alone() {
        let out = apply("-javaagent:/x/profiler.jar\n", &[], is_skgfx_line);
        assert_eq!(out, "-javaagent:/x/profiler.jar\n");
    }

    #[test]
    fn keeps_crlf_line_endings() {
        let out = apply("-Xmx1g\r\n", &managed(), is_skgfx_line);
        assert_eq!(
            out,
            "-Xmx1g\r\n-javaagent:/m/skgfx-agent.jar\r\n-Dskgfx.scale=1.5\r\n"
        );
    }

    #[test]
    fn removing_everything_gives_an_empty_text() {
        assert_eq!(apply("-Dskgfx.scale=2\n", &[], is_skgfx_line), "");
    }

    #[test]
    fn file_roundtrip_and_cleanup() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("extra.txt");
        update_file(&path, &managed(), is_skgfx_line).unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "-javaagent:/m/skgfx-agent.jar\n-Dskgfx.scale=1.5\n"
        );
        update_file(&path, &[], is_skgfx_line).unwrap();
        assert!(!path.exists(), "an extra.txt with nothing left is removed");
        assert!(!dir.path().join("extra.txt.skradle-tmp").exists());
    }

    #[test]
    fn file_with_player_lines_survives_removal() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("extra.txt");
        std::fs::write(&path, "-Xmx2g\n-Dskgfx.scale=2\n").unwrap();
        update_file(&path, &[], is_skgfx_line).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "-Xmx2g\n");
    }
}
