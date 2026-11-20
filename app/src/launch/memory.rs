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
    skradle_core::jar::write_atomic(&path, updated.as_bytes())
}

/// The machine's memory in megabytes, or `None` when it cannot be read. Runs a short system command on macOS and
/// Windows (a few milliseconds), so call it off the UI thread.
pub fn total_ram_mb() -> Option<u32> {
    #[cfg(target_os = "linux")]
    {
        parse_meminfo(&std::fs::read_to_string("/proc/meminfo").ok()?)
    }
    #[cfg(target_os = "macos")]
    {
        let out = std::process::Command::new("/usr/sbin/sysctl")
            .args(["-n", "hw.memsize"])
            .output()
            .ok()?;
        parse_bytes(&String::from_utf8_lossy(&out.stdout))
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        let out = std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "(Get-CimInstance Win32_ComputerSystem).TotalPhysicalMemory",
            ])
            .creation_flags(super::winprobe::CREATE_NO_WINDOW)
            .output()
            .ok()?;
        parse_bytes(&String::from_utf8_lossy(&out.stdout))
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    {
        None
    }
}

/// `MemTotal:  16384256 kB` from `/proc/meminfo`, in megabytes.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn parse_meminfo(text: &str) -> Option<u32> {
    let line = text.lines().find(|l| l.starts_with("MemTotal:"))?;
    let kb: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
    u32::try_from(kb / 1024).ok().filter(|mb| *mb > 0)
}

/// A byte count printed by a system command, in megabytes.
#[cfg_attr(target_os = "linux", allow(dead_code))]
pub fn parse_bytes(text: &str) -> Option<u32> {
    let bytes: u64 = text.trim().parse().ok()?;
    u32::try_from(bytes / (1024 * 1024))
        .ok()
        .filter(|mb| *mb > 0)
}

/// What the memory slider offers: from [`crate::settings::MEMORY_MIN`] up to three quarters of the machine's memory
/// (the rest is left to the system), rounded down to a step of 256 MB, but never below 2048 so there is always a
/// range, and never below what is already chosen. An unknown total offers up to 8192.
pub fn slider_range(total_mb: Option<u32>, current_mb: u32) -> (u32, u32) {
    let by_machine = total_mb.map_or(8192, |t| (t / 4 * 3) / 256 * 256);
    let max = by_machine
        .max(2048)
        .max(current_mb.div_ceil(256) * 256)
        .min(crate::settings::MEMORY_MAX);
    (crate::settings::MEMORY_MIN, max)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_machines_memory_from_the_system_outputs() {
        assert_eq!(
            parse_meminfo("MemTotal:       16384256 kB\nMemFree: 1 kB\n"),
            Some(16000)
        );
        assert_eq!(parse_meminfo("MemFree: 1 kB\n"), None);
        assert_eq!(parse_bytes("17179869184\n"), Some(16384));
        assert_eq!(parse_bytes("lots"), None);
        assert_eq!(parse_bytes("0"), None);
    }

    #[test]
    fn the_slider_range_follows_the_machine_and_never_hides_the_current_value() {
        use crate::settings::{MEMORY_MAX, MEMORY_MIN};
        assert_eq!(slider_range(Some(16384), 0), (MEMORY_MIN, 12288));
        assert_eq!(slider_range(Some(8192), 0), (MEMORY_MIN, 6144));
        assert_eq!(
            slider_range(Some(3000), 0),
            (MEMORY_MIN, 2048),
            "always a range"
        );
        assert_eq!(slider_range(None, 0), (MEMORY_MIN, 8192));
        assert_eq!(
            slider_range(Some(8192), 10000),
            (MEMORY_MIN, 10240),
            "a larger saved value stays reachable"
        );
        assert_eq!(slider_range(Some(u32::MAX), 0).1, MEMORY_MAX);
    }

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
        assert!(!dir.path().join("extra.txt.skradle-tmp").exists());
    }
}
