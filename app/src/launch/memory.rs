//! The game's heap limit lives in the game folder's `extra.txt` (`-Xmx...`), which the updater reads when it starts
//! the game. The launcher only reads it for display and, when the player set a limit, rewrites the `-Xmx` line (and
//! lowers `-Xms` if it would exceed the new limit, which the JVM refuses to start with).

use skradle_core::extra_txt;

/// `4096M`, `2g`, `1048576k` or plain bytes, as the JVM accepts them, in megabytes.
pub fn parse_size_mb(size: &str) -> Option<u32> {
    let size = size.trim();
    let (digits, unit) = size.split_at(
        size.find(|c: char| !c.is_ascii_digit())
            .unwrap_or(size.len()),
    );
    let n: u64 = digits.parse().ok()?;
    let mb = match unit.to_ascii_lowercase().as_str() {
        "m" => n,
        "g" => n.checked_mul(1024)?,
        "k" => n / 1024,
        "" => n / (1024 * 1024),
        _ => return None,
    };
    u32::try_from(mb).ok()
}

fn flag_mb(text: &str, flag: &str) -> Option<u32> {
    text.lines()
        .filter_map(|l| l.trim().strip_prefix(flag))
        .filter_map(parse_size_mb)
        .next_back()
}

/// The heap limit `extra.txt` sets, if any.
pub fn xmx_mb(extra_txt: &str) -> Option<u32> {
    flag_mb(extra_txt, "-Xmx")
}

/// `extra.txt` with the heap limit set to `mb` and `-Xms` capped to it.
pub fn with_memory(existing: &str, mb: u32) -> String {
    let xms = flag_mb(existing, "-Xms").map(|ms| ms.min(mb));
    let mut managed = Vec::new();
    if let Some(xms) = xms {
        managed.push(format!("-Xms{xms}M"));
    }
    managed.push(format!("-Xmx{mb}M"));
    extra_txt::apply(existing, &managed, |line| {
        let line = line.trim();
        line.starts_with("-Xmx") || line.starts_with("-Xms")
    })
}

/// Writes the limit into `dir/extra.txt` (atomically; nothing is written when it already says that).
pub fn write_limit(dir: &std::path::Path, mb: u32) -> std::io::Result<()> {
    let path = dir.join("extra.txt");
    let existing = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e),
    };
    let updated = with_memory(&existing, mb);
    if updated == existing {
        return Ok(());
    }
    let tmp = path.with_extension("txt.skradle-tmp");
    std::fs::write(&tmp, &updated)?;
    std::fs::rename(&tmp, &path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes() {
        assert_eq!(parse_size_mb("4096M"), Some(4096));
        assert_eq!(parse_size_mb("2g"), Some(2048));
        assert_eq!(parse_size_mb("1048576k"), Some(1024));
        assert_eq!(parse_size_mb("536870912"), Some(512));
        assert_eq!(parse_size_mb("lots"), None);
        assert_eq!(parse_size_mb("5x"), None);
    }

    #[test]
    fn reads_the_limit_from_extra_txt() {
        assert_eq!(xmx_mb("-XX:+UseG1GC\n-Xms2048M\n-Xmx4096M\n"), Some(4096));
        assert_eq!(xmx_mb("-Xmx1g\n-Xmx2g\n"), Some(2048));
        assert_eq!(xmx_mb("-Dx=1\n"), None);
    }

    #[test]
    fn sets_the_limit_and_keeps_other_lines() {
        let out = with_memory(
            "-XX:+UseG1GC\n-Xms2048M\n-Xmx4096M\n-Dskgfx.fg=true\n",
            8192,
        );
        assert_eq!(out, "-XX:+UseG1GC\n-Dskgfx.fg=true\n-Xms2048M\n-Xmx8192M\n");
        assert_eq!(with_memory(&out, 8192), out);
    }

    #[test]
    fn lowers_xms_so_the_jvm_still_starts() {
        let out = with_memory("-Xms2048M\n-Xmx4096M\n", 1024);
        assert_eq!(out, "-Xms1024M\n-Xmx1024M\n");
    }

    #[test]
    fn adds_the_limit_when_there_was_none() {
        assert_eq!(with_memory("", 2048), "-Xmx2048M\n");
    }

    #[test]
    fn writes_the_file_in_the_game_folder() {
        let dir = tempfile::tempdir().unwrap();
        write_limit(dir.path(), 3072).unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.path().join("extra.txt")).unwrap(),
            "-Xmx3072M\n"
        );
        write_limit(dir.path(), 3072).unwrap();
        assert!(!dir.path().join("extra.skradle-tmp").exists());
    }
}
