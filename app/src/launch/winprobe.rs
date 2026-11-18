#![cfg_attr(unix, allow(dead_code))]
//! How the running game is found and ended on Windows. The code that runs the commands is in `supervisor.rs`
//! (`cfg(windows)` only); everything that can be wrong with a string lives here, builds on every platform and is
//! tested everywhere.
//!
//! The game's updater starts the game and exits, and Windows has no process group to follow without unsafe code, so
//! the game is found by its command line: it holds the game folder's path (the updater passes `-Dapp_dir`-style
//! absolute paths and the classpath). The folder is handed to PowerShell through an environment variable and
//! compared as a plain substring, so no character in it (quotes, smart quotes, `*`, `[`, `$`, backticks) can be
//! read as PowerShell syntax or as a wildcard.

use std::ffi::OsString;
use std::path::Path;

/// The environment variable that carries the game folder to the script.
pub const ROOT_VARIABLE: &str = "SKRADLE_GAME_ROOT";

/// `CREATE_NO_WINDOW`: stops a console window from flashing for every poll.
#[cfg_attr(not(windows), allow(dead_code))]
pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// The script has no interpolated value at all: the folder is read from [`ROOT_VARIABLE`].
pub const SCRIPT: &str = "$root = $env:SKRADLE_GAME_ROOT; \
if (-not $root) { exit 0 }; \
Get-CimInstance Win32_Process | \
Where-Object { $_.CommandLine -and $_.ProcessId -ne $PID -and $_.Name -match '^javaw?\\.exe$' \
-and $_.CommandLine.IndexOf($root, [System.StringComparison]::OrdinalIgnoreCase) -ge 0 } | \
ForEach-Object { $_.ProcessId }";

/// The arguments of the PowerShell call that lists the game's process ids.
pub fn probe_args() -> Vec<OsString> {
    ["-NoProfile", "-NonInteractive", "-Command", SCRIPT]
        .map(OsString::from)
        .to_vec()
}

/// The folder as the game's command line spells it: backslashes, no trailing separator (the updater expands `.`
/// with `File.getAbsolutePath`, which uses backslashes).
pub fn root_text(root: &Path) -> String {
    let text = root.display().to_string().replace('/', "\\");
    let trimmed = text.trim_end_matches('\\');
    // `C:\` alone must keep its separator.
    if trimmed.len() == 2 && trimmed.ends_with(':') {
        text
    } else {
        trimmed.to_owned()
    }
}

/// The arguments that end a process and its children.
pub fn taskkill_args(pid: u32) -> Vec<OsString> {
    ["/PID", &pid.to_string(), "/T", "/F"]
        .map(OsString::from)
        .to_vec()
}

/// The process ids PowerShell printed, one per line; anything else is ignored.
pub fn parse_pids(output: &str) -> Vec<u32> {
    output
        .lines()
        .filter_map(|line| line.trim().parse().ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_script_never_contains_the_folder() {
        // Nothing is interpolated: whatever the folder is, the script text is the same constant.
        assert!(SCRIPT.contains("$env:SKRADLE_GAME_ROOT"));
        assert!(!SCRIPT.contains("-like"));
        assert!(SCRIPT.contains("IndexOf($root, [System.StringComparison]::OrdinalIgnoreCase)"));
        let args = probe_args();
        assert_eq!(args.len(), 4);
        assert_eq!(args[3], OsString::from(SCRIPT));
    }

    #[test]
    fn hostile_folder_names_stay_out_of_the_command() {
        for name in [
            "C:\\Games\\[SK]\\Spiral Knights",
            "C:\\Games\\\u{2018}; calc; \u{2019}",
            "C:\\Games\\a'b*c?d$e`f",
        ] {
            let args: Vec<String> = probe_args()
                .iter()
                .map(|a| a.to_string_lossy().into_owned())
                .collect();
            assert!(args.iter().all(|a| !a.contains(name)), "{name}");
            assert_eq!(root_text(Path::new(name)), name);
        }
    }

    #[test]
    fn the_folder_is_spelled_the_way_the_game_does() {
        assert_eq!(
            root_text(Path::new("C:/Games/Spiral Knights")),
            "C:\\Games\\Spiral Knights"
        );
        assert_eq!(
            root_text(Path::new("C:\\Games\\Spiral Knights\\")),
            "C:\\Games\\Spiral Knights"
        );
        assert_eq!(root_text(Path::new("C:/")), "C:\\");
    }

    #[test]
    fn taskkill_gets_a_numeric_id_only() {
        assert_eq!(
            taskkill_args(4242),
            ["/PID", "4242", "/T", "/F"].map(OsString::from).to_vec()
        );
    }

    #[test]
    fn process_ids_are_read_from_lines() {
        assert_eq!(
            parse_pids("1234\r\n 5678 \r\nnot a number\r\n\r\n"),
            [1234, 5678]
        );
        assert!(parse_pids("").is_empty());
        assert!(parse_pids("-5\nabc").is_empty());
    }
}
