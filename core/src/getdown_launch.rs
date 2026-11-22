//! What starting the game needs from `getdown.txt` and `extra.txt`: the classpath, the JVM arguments, the main class per
//! app id, and the substitutions and platform qualifiers the game's updater applies. [`crate::getdown_txt`] keeps the
//! few settings the launcher shows; this module reproduces how the updater turns the file into a command line, so a
//! launch that bypasses the updater (see [`crate::launch::direct`]) starts the game the same way.
//!
//! Everything here was read from the game's own updater (`getdown-pro.jar`, Getdown 1.8.10: `Config.parsePairs`,
//! `Config.checkQualifiers`, `Application.initArgs`, `Application.processArg`, `PathBuilder`) with `javap`, and the
//! real `getdown.txt`:
//!
//! * A line is `key = value`, split at the FIRST `=`. A `#` starts a comment anywhere on the line, unless the file
//!   sets `strict_comments = true`, in which case only a `#` in column 0 does. A line without `=` is a key with an
//!   empty value. Repeated keys are lists (`code`, `jvmarg`), in file order.
//! * A platform qualifier is at the START OF THE VALUE: `code = [mac os x] code/a.jar`. Text between `[` and the
//!   first `]` is the qualifier; the line is dropped when it does not match. A value that starts with `[` but has no
//!   `]` is dropped. (The existing [`crate::getdown_txt::GetdownConfig`] still skips lines that START with `[`.)
//! * Qualifiers are a comma list; the line applies when any entry matches. An entry is `os` or `os-arch`
//!   (lowercase, trimmed) and matches when the lowercase Java `os.name` contains `os` and `os.arch` contains `arch`.
//!   A list starting with `!` is a negation of one entry; a negated list with a comma is invalid and the line is
//!   dropped.
//! * App ids: `class` is the default main class and `<id>.class` overrides it; `jvmarg` lines always apply and
//!   `<id>.jvmarg` lines are added; `<id>.apparg` and `<id>.classpath` (extra classpath directories) apply to that
//!   id only. The real file has the ids `client` and `crucible-editor`.
//! * `%APPDIR%`, `%VERSION%` and `%ENV.NAME%` are replaced in every JVM and app argument (an unset variable becomes
//!   `MISSING-NAME`). `extra.txt` is read with the same line parser, so `-Dx = 1` becomes `-Dx=1`, and its lines
//!   are processed like `jvmarg` lines and come after them.
//!
//! Not covered: `optimum_jvmarg`, `env.txt`, the `app.*` system properties the updater forwards, auxiliary groups
//! and the code cache (`use_code_cache`, off in the game's file). None is used by the game's `getdown.txt`.

use std::collections::BTreeSet;
use std::fmt;
use std::path::Path;

use crate::game::Os;

/// The platform qualifiers are evaluated against: Java's `os.name` and `os.arch`, lowercase.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    os_name: String,
    os_arch: String,
}

impl Target {
    /// From the values Java reports (`Mac OS X`, `aarch64`); they are lowercased.
    pub fn new(os_name: &str, os_arch: &str) -> Self {
        Self {
            os_name: os_name.to_lowercase(),
            os_arch: os_arch.to_lowercase(),
        }
    }

    /// The usual Java names for an OS and a CPU architecture as `std::env::consts::ARCH` spells it. Java reports
    /// `amd64` for 64-bit x86 on Windows and Linux and `x86_64` on macOS.
    pub fn for_os(os: Os, arch: &str) -> Self {
        let os_name = match os {
            Os::Windows => "windows",
            Os::MacOs => "mac os x",
            Os::Linux => "linux",
        };
        let os_arch = match (arch, os) {
            ("x86_64", Os::MacOs) => "x86_64",
            ("x86_64", _) => "amd64",
            ("x86", _) => "x86",
            (other, _) => other,
        };
        Self::new(os_name, os_arch)
    }

    pub fn current() -> Self {
        Self::for_os(Os::current(), std::env::consts::ARCH)
    }

    /// True when a qualifier list such as `linux,mac os x` or `!windows` selects this target.
    pub fn matches(&self, qualifiers: &str) -> bool {
        if let Some(negated) = qualifiers.strip_prefix('!') {
            return !qualifiers.contains(',') && !self.matches_one(negated);
        }
        java_split(qualifiers, ',')
            .iter()
            .any(|entry| self.matches_one(entry))
    }

    fn matches_one(&self, entry: &str) -> bool {
        let entry = entry.trim().to_lowercase();
        let mut parts = entry.split('-');
        let os = parts.next().unwrap_or("");
        let arch = parts.next().unwrap_or("");
        self.os_name.contains(os) && self.os_arch.contains(arch)
    }
}

/// `String.split` in Java drops trailing empty strings but keeps one empty string for empty input.
fn java_split(text: &str, separator: char) -> Vec<&str> {
    let mut parts: Vec<&str> = text.split(separator).collect();
    while parts.len() > 1 && parts.last().is_some_and(|p| p.is_empty()) {
        parts.pop();
    }
    parts
}

/// Parses `key = value` lines the way the updater does, dropping lines whose qualifier does not match `target`.
/// A line without `=` gives the whole line as key and an empty value.
pub fn parse_pairs(text: &str, strict_comments: bool, target: &Target) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    for line in text.lines() {
        let line = match line.find('#') {
            Some(0) => "",
            Some(at) if !strict_comments => &line[..at],
            _ => line,
        };
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let (key, mut value) = match line.split_once('=') {
            Some((key, value)) => (key.trim(), value.trim()),
            None => (line, ""),
        };
        if value.starts_with('[') {
            let Some(end) = value.find(']') else { continue };
            if !target.matches(&value[1..end]) {
                continue;
            }
            value = value[end + 1..].trim();
        }
        pairs.push((key.to_owned(), value.to_owned()));
    }
    pairs
}

/// The JVM arguments of an `extra.txt`: each line as the updater passes it (`key=value` rejoined when it has a
/// value). Pass `strict_comments` from the game's `getdown.txt`.
pub fn extra_jvm_args(text: &str, strict_comments: bool, target: &Target) -> Vec<String> {
    parse_pairs(text, strict_comments, target)
        .into_iter()
        .map(|(key, value)| {
            if value.is_empty() {
                key
            } else {
                format!("{key}={value}")
            }
        })
        .collect()
}

/// Replaces `%APPDIR%`, `%VERSION%` and `%ENV.NAME%` like the updater. `env` looks up an environment variable.
pub fn process_arg(
    arg: &str,
    app_dir: &Path,
    version: &str,
    env: &dyn Fn(&str) -> Option<String>,
) -> String {
    let arg = arg
        .replace("%APPDIR%", &app_dir.to_string_lossy())
        .replace("%VERSION%", version);
    let mut out = String::with_capacity(arg.len());
    let mut rest = arg.as_str();
    while let Some(start) = rest.find("%ENV.") {
        let after = &rest[start + 5..];
        let Some(end) = after.find('%') else { break };
        out.push_str(&rest[..start]);
        let name = &after[..end];
        match env(name) {
            Some(value) => out.push_str(&value),
            None => {
                out.push_str("MISSING-");
                out.push_str(name);
            }
        }
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out
}

/// What can go wrong turning the config into a launch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigError {
    /// Neither `class` nor `<id>.class` is set.
    MissingClass { app_id: String },
    /// An editor that comes from the downloadable editors pack was asked for without it.
    MissingEditorPack,
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingClass { app_id } => {
                write!(f, "getdown.txt has no main class for app id '{app_id}'")
            }
            Self::MissingEditorPack => write!(f, "the editors have not been downloaded yet"),
        }
    }
}

impl std::error::Error for ConfigError {}

/// One app id's launch settings, with qualifiers already applied. Arguments are as written (not yet substituted).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppLaunch {
    pub app_id: String,
    pub main_class: String,
    /// Classpath jars, relative to the game folder, in file order.
    pub code: Vec<String>,
    /// Extra classpath directories (`<id>.classpath`), as written.
    pub classpath_dirs: Vec<String>,
    /// `jvmarg` lines, then the app id's own.
    pub jvm_args: Vec<String>,
    pub app_args: Vec<String>,
}

impl AppLaunch {
    /// The `-classpath` value: jars then extra directories, joined with the platform's separator.
    pub fn classpath(&self, os: Os) -> String {
        let separator = if os == Os::Windows { ";" } else { ":" };
        self.code
            .iter()
            .chain(&self.classpath_dirs)
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join(separator)
    }
}

/// A `getdown.txt` read for one target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchConfig {
    pairs: Vec<(String, String)>,
    strict_comments: bool,
}

impl LaunchConfig {
    pub fn parse(text: &str, target: &Target) -> Self {
        let strict_comments = parse_pairs(text, false, target)
            .iter()
            .any(|(k, v)| k == "strict_comments" && v == "true");
        Self {
            pairs: parse_pairs(text, strict_comments, target),
            strict_comments,
        }
    }

    /// Whether the file asked for column-0-only comments; `extra.txt` is read the same way.
    pub fn strict_comments(&self) -> bool {
        self.strict_comments
    }

    /// All values of a key, in file order.
    pub fn values(&self, key: &str) -> Vec<&str> {
        self.pairs
            .iter()
            .filter(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
            .collect()
    }

    pub fn first(&self, key: &str) -> Option<&str> {
        self.values(key).into_iter().next()
    }

    /// Every app id the file defines (a `<id>.class`, `.jvmarg`, `.apparg` or `.classpath` key), sorted.
    pub fn app_ids(&self) -> Vec<String> {
        let ids: BTreeSet<&str> = self
            .pairs
            .iter()
            .filter_map(|(k, _)| {
                let (id, suffix) = k.rsplit_once('.')?;
                (!id.is_empty() && matches!(suffix, "class" | "jvmarg" | "apparg" | "classpath"))
                    .then_some(id)
            })
            .collect();
        ids.into_iter().map(str::to_owned).collect()
    }

    /// The launch settings for an app id (`client`, `crucible-editor`). An empty id means the file's defaults.
    pub fn app(&self, app_id: &str) -> Result<AppLaunch, ConfigError> {
        let prefix = if app_id.is_empty() {
            String::new()
        } else {
            format!("{app_id}.")
        };
        let own = |suffix: &str| -> Vec<String> {
            self.values(&format!("{prefix}{suffix}"))
                .into_iter()
                .map(str::to_owned)
                .collect()
        };
        let main_class = self
            .first(&format!("{prefix}class"))
            .or_else(|| self.first("class"))
            .ok_or_else(|| ConfigError::MissingClass {
                app_id: app_id.to_owned(),
            })?
            .to_owned();
        let mut jvm_args: Vec<String> = self
            .values("jvmarg")
            .into_iter()
            .map(str::to_owned)
            .collect();
        if !prefix.is_empty() {
            jvm_args.extend(own("jvmarg"));
        }
        Ok(AppLaunch {
            app_id: app_id.to_owned(),
            main_class,
            code: self.values("code").into_iter().map(str::to_owned).collect(),
            classpath_dirs: own("classpath"),
            jvm_args,
            app_args: own("apparg"),
        })
    }
}

/// A small anonymised copy of the structure of the game's `getdown.txt`: the same keys, qualifiers and app ids, with
/// invented hosts and a few of the code entries.
#[cfg(test)]
pub(crate) const FIXTURE: &str = include_str!("../tests/fixtures/getdown.txt");

#[cfg(test)]
mod tests {
    use super::*;

    fn mac() -> Target {
        Target::new("Mac OS X", "aarch64")
    }
    fn win() -> Target {
        Target::new("Windows 11", "amd64")
    }
    fn linux() -> Target {
        Target::new("Linux", "amd64")
    }

    #[test]
    fn qualifier_lists_and_negation() {
        assert!(mac().matches("mac os x"));
        assert!(!mac().matches("linux"));
        assert!(win().matches("windows"));
        assert!(linux().matches("windows, linux"));
        assert!(linux().matches("linux,"));
        assert!(!win().matches("!windows"));
        assert!(linux().matches("!windows"));
        assert!(!linux().matches("!windows,mac os x"));
    }

    #[test]
    fn qualifiers_can_name_an_architecture() {
        assert!(mac().matches("mac os x-aarch64"));
        assert!(!mac().matches("mac os x-amd64"));
        assert!(linux().matches("LINUX-AMD64"));
    }

    #[test]
    fn target_names_follow_java() {
        assert_eq!(
            Target::for_os(Os::Linux, "x86_64"),
            Target::new("linux", "amd64")
        );
        assert_eq!(
            Target::for_os(Os::MacOs, "x86_64"),
            Target::new("Mac OS X", "x86_64")
        );
        assert_eq!(
            Target::for_os(Os::MacOs, "aarch64"),
            Target::new("mac os x", "aarch64")
        );
    }

    #[test]
    fn pairs_split_at_the_first_equals_and_apply_qualifiers() {
        let text =
            "a = 1\nurl = http://h/x?y=z\ncode = [linux] l.jar\ncode = [mac os x] m.jar\nbare\n";
        let pairs = parse_pairs(text, false, &mac());
        assert_eq!(
            pairs,
            [
                ("a".into(), "1".into()),
                ("url".into(), "http://h/x?y=z".into()),
                ("code".into(), "m.jar".into()),
                ("bare".into(), "".into()),
            ]
        );
    }

    #[test]
    fn comments_depend_on_strictness() {
        let text = "a = 1 # note\n# whole\n  # indented\nb = 2\n";
        let loose = parse_pairs(text, false, &mac());
        assert_eq!(loose, [("a".into(), "1".into()), ("b".into(), "2".into())]);
        let strict = parse_pairs(text, true, &mac());
        assert_eq!(strict[0], ("a".into(), "1 # note".into()));
        assert!(strict.iter().any(|(k, _)| k == "# indented"));
    }

    #[test]
    fn broken_qualifiers_drop_the_line() {
        let pairs = parse_pairs("a = [linux oops\nb = [] x\nc = [!a,b] y\n", false, &linux());
        assert_eq!(pairs, [("b".into(), "x".into())]);
    }

    #[test]
    fn extra_txt_lines_are_rejoined() {
        let args = extra_jvm_args(
            "-Xmx2g\n-Dx = 1\n-XX:+UseG1GC # fast\n-Dw = [windows] 1\n",
            false,
            &mac(),
        );
        assert_eq!(args, ["-Xmx2g", "-Dx=1", "-XX:+UseG1GC"]);
    }

    #[test]
    fn substitutions() {
        let env = |name: &str| (name == "HOME").then(|| "/home/k".to_owned());
        let out = process_arg(
            "-Da=%APPDIR%/n -Dv=%VERSION% -Dh=%ENV.HOME%/x -Dm=%ENV.NOPE%",
            Path::new("/games/sk"),
            "42",
            &env,
        );
        assert_eq!(out, "-Da=/games/sk/n -Dv=42 -Dh=/home/k/x -Dm=MISSING-NOPE");
        assert_eq!(
            process_arg("100%", Path::new("/g"), "1", &env),
            "100%",
            "a lone percent sign stays"
        );
    }

    #[test]
    fn reads_the_real_files_structure() {
        let config = LaunchConfig::parse(FIXTURE, &mac());
        assert_eq!(config.app_ids(), ["client", "crucible-editor"]);
        let client = config.app("client").unwrap();
        assert_eq!(
            client.main_class,
            "com.threerings.projectx.client.ProjectXApp"
        );
        assert!(client
            .code
            .contains(&"code/lwjgl-natives-macos-arm64.jar".to_owned()));
        assert!(!client
            .code
            .iter()
            .any(|c| c.contains("linux") || c.contains("windows")));
        assert!(client
            .jvm_args
            .contains(&"-Djava.library.path=%APPDIR%/native".to_owned()));
        assert!(client
            .jvm_args
            .contains(&"-Djdk.lang.Process.launchMechanism=FORK".to_owned()));

        let linux_client = LaunchConfig::parse(FIXTURE, &linux())
            .app("client")
            .unwrap();
        assert!(!linux_client
            .jvm_args
            .iter()
            .any(|a| a.contains("launchMechanism")));
        assert!(linux_client
            .code
            .contains(&"code/lwjgl-natives-linux.jar".to_owned()));
    }

    #[test]
    fn app_ids_override_the_class_and_share_the_code() {
        let config = LaunchConfig::parse(FIXTURE, &win());
        let editor = config.app("crucible-editor").unwrap();
        assert_eq!(
            editor.main_class,
            "com.threerings.projectx.tools.CrucibleSceneEditor"
        );
        assert_eq!(editor.code, config.app("client").unwrap().code);
        // Without an id the default class applies.
        assert_eq!(
            config.app("").unwrap().main_class,
            "com.threerings.projectx.client.ProjectXApp"
        );
        // An unknown id falls back to the default class.
        assert_eq!(
            config.app("nope").unwrap().main_class,
            "com.threerings.projectx.client.ProjectXApp"
        );
    }

    #[test]
    fn missing_class_is_an_error() {
        let config = LaunchConfig::parse("code = a.jar\n", &mac());
        assert_eq!(
            config.app("client"),
            Err(ConfigError::MissingClass {
                app_id: "client".into()
            })
        );
        assert!(config
            .app("client")
            .unwrap_err()
            .to_string()
            .contains("client"));
    }

    #[test]
    fn id_specific_jvm_args_apply_after_the_shared_ones() {
        let text = "class = A\njvmarg = -Dshared=1\nx.jvmarg = -Dx=1\nx.apparg = go\nx.classpath = extra\n";
        let config = LaunchConfig::parse(text, &mac());
        let x = config.app("x").unwrap();
        assert_eq!(x.jvm_args, ["-Dshared=1", "-Dx=1"]);
        assert_eq!(x.app_args, ["go"]);
        assert_eq!(x.classpath_dirs, ["extra"]);
        let none = config.app("").unwrap();
        assert_eq!(none.jvm_args, ["-Dshared=1"]);
        assert!(none.app_args.is_empty());
    }

    #[test]
    fn classpath_uses_the_platform_separator() {
        let config = LaunchConfig::parse("class = A\ncode = a.jar\ncode = b.jar\n", &mac());
        let app = config.app("").unwrap();
        assert_eq!(app.classpath(Os::Linux), "a.jar:b.jar");
        assert_eq!(app.classpath(Os::Windows), "a.jar;b.jar");
    }

    #[test]
    fn strict_comments_is_detected_from_the_file() {
        let config = LaunchConfig::parse("strict_comments = true\nclass = A # B\n", &mac());
        assert!(config.strict_comments());
        assert_eq!(config.first("class"), Some("A # B"));
        assert!(!LaunchConfig::parse("class = A # B\n", &mac()).strict_comments());
    }
}
