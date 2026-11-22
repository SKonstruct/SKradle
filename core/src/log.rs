//! SKradle's own log: one file, `skradle.log`, with a previous run kept beside it. See `docs/logging.md`.
//!
//! Small and in-house on purpose: four levels, UTC ISO 8601 timestamps, a size cap with rotation, and text scrubbed
//! of secrets and home folders before it is written. Logging never panics and never returns an error; if the file
//! cannot be opened or written, lines are dropped. Calls made before [`init`] are dropped too.

use std::collections::VecDeque;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use regex::Regex;

/// The most a log file may hold before it is rotated.
pub const MAX_BYTES: u64 = 2 * 1024 * 1024;
/// How many files are kept: `skradle.log`, `skradle.1.log`, `skradle.2.log`.
pub const KEEP: usize = 3;
/// The folder name under the data directory (`<data dir>/SKradle/logs`).
pub const FOLDER_NAME: &str = "logs";
const FILE_STEM: &str = "skradle";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Error,
    Warn,
    Info,
    Debug,
}

impl Level {
    fn label(self) -> &'static str {
        match self {
            Level::Error => "ERROR",
            Level::Warn => "WARN ",
            Level::Info => "INFO ",
            Level::Debug => "DEBUG",
        }
    }

    /// `error`, `warn`, `info` or `debug` (any case).
    pub fn parse(text: &str) -> Option<Level> {
        match text.trim().to_ascii_lowercase().as_str() {
            "error" => Some(Level::Error),
            "warn" | "warning" => Some(Level::Warn),
            "info" => Some(Level::Info),
            "debug" | "trace" => Some(Level::Debug),
            _ => None,
        }
    }
}

/// The path of log number `index` in `dir` (0 is the current one).
pub fn file_path(dir: &Path, index: usize) -> PathBuf {
    if index == 0 {
        dir.join(format!("{FILE_STEM}.log"))
    } else {
        dir.join(format!("{FILE_STEM}.{index}.log"))
    }
}

/// Moves `skradle.log` to `skradle.1.log`, that one to `.2`, and so on; the oldest is dropped. Errors are ignored:
/// a log that cannot rotate is still better than none.
fn rotate_files(dir: &Path) {
    let _ = fs::remove_file(file_path(dir, KEEP - 1));
    for index in (0..KEEP - 1).rev() {
        let from = file_path(dir, index);
        if from.is_file() {
            let _ = fs::rename(&from, file_path(dir, index + 1));
        }
    }
}

/// The writer behind the global log; also usable on its own (tests).
pub struct Logger {
    dir: PathBuf,
    file: Option<File>,
    size: u64,
    max: u64,
    level: Level,
    home: Option<String>,
}

impl Logger {
    /// Opens a new `skradle.log` in `dir` (created if needed). The previous run's file becomes `skradle.1.log`.
    pub fn open(dir: &Path, max_bytes: u64, level: Level, home: Option<&Path>) -> Logger {
        let _ = fs::create_dir_all(dir);
        let current = file_path(dir, 0);
        if fs::metadata(&current).is_ok_and(|m| m.len() > 0) {
            rotate_files(dir);
        }
        let mut logger = Logger {
            dir: dir.to_path_buf(),
            file: None,
            size: 0,
            max: max_bytes.max(1024),
            level,
            home: home.map(|h| h.to_string_lossy().into_owned()),
        };
        logger.reopen();
        logger
    }

    fn reopen(&mut self) {
        self.file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(file_path(&self.dir, 0))
            .ok();
        self.size = self
            .file
            .as_ref()
            .and_then(|f| f.metadata().ok())
            .map_or(0, |m| m.len());
    }

    /// Writes one entry stamped `now` (already formatted). Text is scrubbed first; a newline inside the message
    /// continues on an indented line.
    pub fn write(&mut self, now: &str, level: Level, target: &str, message: &str) {
        if level > self.level {
            return;
        }
        let clean = scrub(message, self.home.as_deref());
        let mut text = format!("{now} {} [{target}] ", level.label());
        let mut lines = clean.lines();
        text.push_str(lines.next().unwrap_or(""));
        for line in lines {
            text.push_str("\n    ");
            text.push_str(line);
        }
        text.push('\n');
        if self.file.is_some() && self.size + text.len() as u64 > self.max {
            self.file = None;
            rotate_files(&self.dir);
            self.reopen();
        }
        if let Some(file) = self.file.as_mut() {
            if file.write_all(text.as_bytes()).is_ok() {
                self.size += text.len() as u64;
            }
        }
    }

    pub fn path(&self) -> PathBuf {
        file_path(&self.dir, 0)
    }
}

fn secret_pattern() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r#"(?i)(password|passwd|pwd|token|secret|session[_-]?key|authorization|api[_-]?key|access[_-]?key|cookie|encrypted)(["']?\s*[:=]\s*["']?)(?:bearer\s+)?[^\s"'&,;]+"#,
        )
        .expect("a valid pattern")
    })
}

fn query_pattern() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"(https?://[^\s?#"']+)[?#][^\s"']*"#).expect("a valid pattern"))
}

fn userinfo_pattern() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"(?i)(https?://)[^/@\s"']+@"#).expect("a valid pattern"))
}

fn long_secret_pattern() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b[0-9A-Za-z+/_-]{40,}={0,2}").expect("a valid pattern"))
}

/// Removes what must not reach a log: the home folder becomes `~`, `password=...`-style values and long keys are
/// replaced, and a URL loses its query string and any `user:password@`.
pub fn scrub(text: &str, home: Option<&str>) -> String {
    let mut out = text.to_owned();
    if let Some(home) = home.filter(|h| h.len() > 1) {
        let trimmed = home.trim_end_matches(['/', '\\']);
        for variant in [trimmed.to_owned(), trimmed.replace('\\', "/"), trimmed.replace('/', "\\")] {
            if !variant.is_empty() {
                out = out.replace(&variant, "~");
            }
        }
    }
    out = userinfo_pattern().replace_all(&out, "${1}<user>@").into_owned();
    out = query_pattern().replace_all(&out, "${1}?<query>").into_owned();
    out = secret_pattern().replace_all(&out, "${1}${2}<redacted>").into_owned();
    out = long_secret_pattern().replace_all(&out, "<redacted>").into_owned();
    out
}

/// `2026-11-22T14:03:09.123Z` for a Unix time in milliseconds.
pub fn iso8601(unix_millis: u64) -> String {
    let secs = unix_millis / 1000;
    let millis = unix_millis % 1000;
    let days = (secs / 86_400) as i64;
    let rest = secs % 86_400;
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { year + 1 } else { year };
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{millis:03}Z",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    )
}

fn now_text() -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64);
    iso8601(millis)
}

/// What the first lines of a session say about the program and the machine.
#[derive(Debug, Clone, Default)]
pub struct Session {
    pub version: String,
    pub os: String,
    pub arch: String,
    pub qt: String,
    /// `steam`, `standalone` or `custom` for the selected install; `None` when there is none.
    pub install_kind: Option<String>,
    /// The Java a launch would use, for example `25.0.3 (the game's own)`; `None` when none was found.
    pub java: Option<String>,
}

/// The session header, one line each.
pub fn session_header(session: &Session) -> Vec<String> {
    let or = |value: &Option<String>, none: &str| value.clone().unwrap_or_else(|| none.to_owned());
    vec![
        format!("SKradle {} started", session.version),
        format!("system: {} {}", session.os, session.arch),
        format!("Qt: {}", session.qt),
        format!("install kind: {}", or(&session.install_kind, "no game found")),
        format!("java: {}", or(&session.java, "none found")),
    ]
}

static GLOBAL: OnceLock<Mutex<Logger>> = OnceLock::new();

fn lock() -> Option<MutexGuard<'static, Logger>> {
    GLOBAL
        .get()
        .map(|m| m.lock().unwrap_or_else(|e| e.into_inner()))
}

/// The folder logs go to by default: `<data dir>/SKradle/logs` (`SKRADLE_LOG_DIR` overrides it).
pub fn default_dir(data_dir: Option<PathBuf>) -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("SKRADLE_LOG_DIR").filter(|v| !v.is_empty()) {
        return Some(PathBuf::from(dir));
    }
    data_dir.map(|d| d.join("SKradle").join(FOLDER_NAME))
}

/// Starts the global log in `dir`. Only the first call has an effect. `SKRADLE_LOG=debug` (or `info`, `warn`,
/// `error`) picks the level; the default is `info`. Returns the folder.
pub fn init(dir: &Path, home: Option<&Path>) -> PathBuf {
    let level = std::env::var("SKRADLE_LOG")
        .ok()
        .and_then(|v| Level::parse(&v))
        .unwrap_or(Level::Info);
    let _ = GLOBAL.set(Mutex::new(Logger::open(dir, MAX_BYTES, level, home)));
    dir.to_path_buf()
}

/// The folder of the global log, once started.
pub fn folder() -> Option<PathBuf> {
    lock().and_then(|l| l.path().parent().map(Path::to_path_buf))
}

/// The current log file, once started.
pub fn current_file() -> Option<PathBuf> {
    lock().map(|l| l.path())
}

pub fn log(level: Level, target: &str, message: &str) {
    if let Some(mut logger) = lock() {
        logger.write(&now_text(), level, target, message);
    }
}

pub fn error(target: &str, message: impl AsRef<str>) {
    log(Level::Error, target, message.as_ref());
}
pub fn warn(target: &str, message: impl AsRef<str>) {
    log(Level::Warn, target, message.as_ref());
}
pub fn info(target: &str, message: impl AsRef<str>) {
    log(Level::Info, target, message.as_ref());
}
pub fn debug(target: &str, message: impl AsRef<str>) {
    log(Level::Debug, target, message.as_ref());
}

/// The last `n` lines of the file at `path` (fewer when it is shorter; empty when it cannot be read). Continuation
/// lines of a multi-line entry count as lines.
pub fn tail_of(path: &Path, n: usize) -> Vec<String> {
    let Ok(file) = File::open(path) else {
        return Vec::new();
    };
    let mut lines: VecDeque<String> = VecDeque::with_capacity(n.min(4096) + 1);
    for line in BufReader::new(file).lines().map_while(Result::ok) {
        if n == 0 {
            break;
        }
        if lines.len() == n {
            lines.pop_front();
        }
        lines.push_back(line);
    }
    lines.into_iter().collect()
}

/// The last `n` lines of the current log.
pub fn tail(n: usize) -> Vec<String> {
    current_file().map_or_else(Vec::new, |p| tail_of(&p, n))
}

/// Logs panics (location and message, with a backtrace when `RUST_BACKTRACE` asks for one) and then runs the hook
/// that was installed before, so the usual message and the crash reporter still run.
pub fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let location = info
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()))
            .unwrap_or_default();
        let message = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| (*s).to_owned())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "panic".to_owned());
        let thread = std::thread::current();
        error(
            "panic",
            format!(
                "thread '{}' panicked at {location}: {message}\n{}",
                thread.name().unwrap_or("?"),
                std::backtrace::Backtrace::capture()
            ),
        );
        previous(info);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(dir: &Path, index: usize) -> String {
        fs::read_to_string(file_path(dir, index)).unwrap_or_default()
    }

    #[test]
    fn timestamps_are_utc_iso_8601() {
        assert_eq!(iso8601(0), "1970-01-01T00:00:00.000Z");
        // 2024-02-29 12:34:56.789 (a leap day).
        assert_eq!(iso8601(1_709_210_096_789), "2024-02-29T12:34:56.789Z");
        assert_eq!(iso8601(1_798_000_000_000), "2026-12-23T04:26:40.000Z");
    }

    #[test]
    fn an_entry_has_time_level_target_and_message() {
        let dir = tempfile::tempdir().unwrap();
        let mut log = Logger::open(dir.path(), MAX_BYTES, Level::Info, None);
        log.write("2026-11-22T10:00:00.000Z", Level::Warn, "launch", "careful");
        log.write("2026-11-22T10:00:01.000Z", Level::Debug, "launch", "hidden");
        assert_eq!(
            read(dir.path(), 0),
            "2026-11-22T10:00:00.000Z WARN  [launch] careful\n"
        );
    }

    #[test]
    fn debug_entries_appear_when_the_level_allows() {
        let dir = tempfile::tempdir().unwrap();
        let mut log = Logger::open(dir.path(), MAX_BYTES, Level::Debug, None);
        log.write("t", Level::Debug, "x", "shown");
        assert!(read(dir.path(), 0).contains("DEBUG [x] shown"));
    }

    #[test]
    fn a_multi_line_message_stays_together() {
        let dir = tempfile::tempdir().unwrap();
        let mut log = Logger::open(dir.path(), MAX_BYTES, Level::Info, None);
        log.write("t", Level::Error, "panic", "first\nsecond\nthird");
        assert_eq!(read(dir.path(), 0), "t ERROR [panic] first\n    second\n    third\n");
    }

    #[test]
    fn the_previous_run_is_kept_as_the_first_old_file() {
        let dir = tempfile::tempdir().unwrap();
        {
            let mut log = Logger::open(dir.path(), MAX_BYTES, Level::Info, None);
            log.write("t", Level::Info, "x", "run one");
        }
        {
            let mut log = Logger::open(dir.path(), MAX_BYTES, Level::Info, None);
            log.write("t", Level::Info, "x", "run two");
        }
        {
            let mut log = Logger::open(dir.path(), MAX_BYTES, Level::Info, None);
            log.write("t", Level::Info, "x", "run three");
        }
        {
            let mut log = Logger::open(dir.path(), MAX_BYTES, Level::Info, None);
            log.write("t", Level::Info, "x", "run four");
        }
        assert!(read(dir.path(), 0).contains("run four"));
        assert!(read(dir.path(), 1).contains("run three"));
        assert!(read(dir.path(), 2).contains("run two"));
        // Only KEEP files exist: run one is gone.
        assert!(!file_path(dir.path(), 3).exists());
    }

    #[test]
    fn an_empty_previous_file_is_not_rotated() {
        let dir = tempfile::tempdir().unwrap();
        drop(Logger::open(dir.path(), MAX_BYTES, Level::Info, None));
        drop(Logger::open(dir.path(), MAX_BYTES, Level::Info, None));
        assert!(!file_path(dir.path(), 1).exists());
    }

    #[test]
    fn a_file_over_the_limit_rotates_and_stays_under_it() {
        let dir = tempfile::tempdir().unwrap();
        let mut log = Logger::open(dir.path(), 2048, Level::Info, None);
        let line = "word ".repeat(40);
        for i in 0..40 {
            log.write("t", Level::Info, "x", &format!("{i} {line}"));
        }
        let size = |i| fs::metadata(file_path(dir.path(), i)).map_or(0, |m| m.len());
        assert!(size(0) <= 2048 && size(0) > 0, "{}", size(0));
        assert!(size(1) > 0 && size(1) <= 2048);
        assert!(!file_path(dir.path(), KEEP).exists());
        // The newest entry is in the current file.
        assert!(read(dir.path(), 0).contains("39 word"));
    }

    #[test]
    fn the_home_folder_becomes_a_tilde() {
        assert_eq!(
            scrub("opened /Users/ann/Games/SK/x.jar", Some("/Users/ann")),
            "opened ~/Games/SK/x.jar"
        );
        assert_eq!(
            scrub(r"opened C:\Users\ann\SK and C:/Users/ann/SK", Some(r"C:\Users\ann")),
            r"opened ~\SK and ~/SK"
        );
    }

    #[test]
    fn secrets_and_query_strings_are_removed() {
        let out = scrub(
            "login password=hunter2 token: abc123 -DsessionKey=zzz https://u:p@host.example/a/b?key=1&x=2 done",
            None,
        );
        assert!(!out.contains("hunter2") && !out.contains("abc123") && !out.contains("zzz"), "{out}");
        assert!(!out.contains("key=1") && !out.contains("u:p@"), "{out}");
        assert!(out.contains("https://<user>@host.example/a/b?<query>"), "{out}");
        assert!(out.contains("password=<redacted>"), "{out}");
        assert!(out.ends_with("done"));
        let key = "A".repeat(48);
        assert_eq!(scrub(&format!("key {key} end"), None), "key <redacted> end");
        assert_eq!(scrub("Authorization: Bearer abc.def", None), "Authorization: <redacted>");
    }

    #[test]
    fn ordinary_text_is_left_alone() {
        let text = "Steam client: running, signed in (pid 4242); 2 variables set";
        assert_eq!(scrub(text, Some("/Users/ann")), text);
    }

    #[test]
    fn the_logger_scrubs_what_it_writes() {
        let dir = tempfile::tempdir().unwrap();
        let mut log = Logger::open(dir.path(), MAX_BYTES, Level::Info, Some(Path::new("/home/bo")));
        log.write("t", Level::Info, "x", "path /home/bo/sk password=abc");
        let text = read(dir.path(), 0);
        assert!(text.contains("path ~/sk password=<redacted>"), "{text}");
    }

    #[test]
    fn the_session_header_names_version_system_qt_install_and_java() {
        let header = session_header(&Session {
            version: "0.1.0".into(),
            os: "windows".into(),
            arch: "x86_64".into(),
            qt: "6.11.3".into(),
            install_kind: Some("steam".into()),
            java: Some("25.0.3 (the game's own)".into()),
        });
        assert_eq!(
            header,
            [
                "SKradle 0.1.0 started",
                "system: windows x86_64",
                "Qt: 6.11.3",
                "install kind: steam",
                "java: 25.0.3 (the game's own)"
            ]
        );
        let bare = session_header(&Session::default());
        assert!(bare[3].contains("no game found") && bare[4].contains("none found"));
    }

    #[test]
    fn the_tail_returns_the_last_lines() {
        let dir = tempfile::tempdir().unwrap();
        let mut log = Logger::open(dir.path(), MAX_BYTES, Level::Info, None);
        for i in 0..10 {
            log.write("t", Level::Info, "x", &format!("n{i}"));
        }
        let path = file_path(dir.path(), 0);
        let last = tail_of(&path, 3);
        assert_eq!(last.len(), 3);
        assert!(last[2].ends_with("n9") && last[0].ends_with("n7"));
        assert_eq!(tail_of(&path, 100).len(), 10);
        assert!(tail_of(&path, 0).is_empty());
        assert!(tail_of(&dir.path().join("missing"), 5).is_empty());
    }

    #[test]
    fn levels_parse() {
        assert_eq!(Level::parse(" DEBUG "), Some(Level::Debug));
        assert_eq!(Level::parse("warning"), Some(Level::Warn));
        assert_eq!(Level::parse("loud"), None);
    }
}
