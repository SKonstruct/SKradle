//! The part of `config/server.yml` the launcher decides: who can reach the game and the web listener, and on
//! which ports. Every other line, comment and key in the file is left exactly as the player has it.
//!
//! **What the engine's source says** (read from the engine's settings and web code, v0.40.x; nothing was run):
//!
//! * The game listener and the shared HTTP listener (WebUI under `/admin/`, News images under `/news/`) both bind
//!   `server-ip`. They cannot be bound differently, so the administration pages cannot be moved to loopback while
//!   players on the LAN keep the game and News.
//! * `webui-public-url` is not a bind address. It only sets which `Host` and `Origin` the admin routes accept: with
//!   `0.0.0.0` ("automatic") the connection's own local IP or, for loopback peers, `localhost`; with a concrete
//!   origin, exactly that authority. A request that fails the check gets `403` from every `/` and `/admin/` route.
//!   `/news/` has its own handler and is not checked. The `Host` header is chosen by the client, so this is
//!   protection against a browser being tricked into talking to the server, not access control.
//! * Access control is the WebUI login: every `/admin/api/` route except the first-run `setup` and `login` needs a
//!   session, for loopback peers too, and logins are limited to ten a minute for the whole server.
//!
//! **What the launcher writes** ([`HostConfig::apply`]): `webui-enabled: true`, `webui-tls-mode: http`,
//! `webui-public-url: http://127.0.0.1:<web port>`, `webui/content_url-port: <web port>`, `server-port`, and
//! `server-ip` (`127.0.0.1`, or `0.0.0.0` when other players may join). With other players allowed this is the
//! safest configuration the engine offers: LAN browsers fail the Host check, LAN players still reach `/news/`, and a
//! hand-made request that spoofs the Host header still meets the login. It remains plain HTTP guarded only by
//! the password, so only enable LAN hosting on a network you trust. Without LAN hosting everything is loopback only.
//! The web port is a concrete number kept in the file: port `0` would change the players' content URL on every start.
//! A legacy `news-port` line is dropped, because the engine refuses a file where it and
//! `webui/content_url-port` disagree.

use std::fmt;
use std::io;

use super::fsx::write_atomic;
use super::layout::ServerLayout;

pub const DEFAULT_GAME_PORT: u16 = 47624;
pub const DEFAULT_WEB_PORT: u16 = 8080;
pub const LOOPBACK: &str = "127.0.0.1";

#[derive(Debug)]
pub enum ConfigError {
    /// A port outside 1-65535, or the game and web ports are the same.
    BadPort(&'static str),
    /// The file has a line the launcher does not dare to edit around (indentation, a list, a repeated key, ...).
    Malformed {
        line: usize,
        why: &'static str,
    },
    Io(io::Error),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::BadPort(why) => write!(f, "{why}"),
            ConfigError::Malformed { line, why } => {
                write!(f, "server.yml line {line}: {why}")
            }
            ConfigError::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for ConfigError {}

impl From<io::Error> for ConfigError {
    fn from(e: io::Error) -> Self {
        ConfigError::Io(e)
    }
}

/// What the launcher decides about reachability.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HostConfig {
    /// Whether other players (LAN or VPN) may connect to the game and the News listener.
    pub lan: bool,
    pub game_port: u16,
    pub web_port: u16,
}

impl Default for HostConfig {
    fn default() -> Self {
        Self {
            lan: false,
            game_port: DEFAULT_GAME_PORT,
            web_port: DEFAULT_WEB_PORT,
        }
    }
}

impl HostConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.game_port == 0 {
            return Err(ConfigError::BadPort("the game port must be 1-65535"));
        }
        if self.web_port == 0 {
            return Err(ConfigError::BadPort("the web port must be 1-65535"));
        }
        if self.game_port == self.web_port {
            return Err(ConfigError::BadPort(
                "the game port and the web port must differ",
            ));
        }
        Ok(())
    }

    /// The `server-ip` value.
    pub fn bind_address(&self) -> &'static str {
        if self.lan {
            "0.0.0.0"
        } else {
            LOOPBACK
        }
    }

    /// The `webui-public-url` value: the one address the embedded view must use.
    pub fn public_url(&self) -> String {
        format!("http://{LOOPBACK}:{}", self.web_port)
    }

    /// The URL the app opens for administration. Always `127.0.0.1`, never `localhost`: it must match
    /// [`HostConfig::public_url`] exactly or the engine refuses the requests.
    pub fn web_ui_url(&self) -> String {
        format!("{}/admin/", self.public_url())
    }

    /// The content (News) URL a player at `host` puts in the server entry.
    pub fn content_url(&self, host: &str) -> String {
        let host = if host.contains(':') && !host.starts_with('[') {
            format!("[{host}]")
        } else {
            host.to_owned()
        };
        format!("http://{host}:{}/", self.web_port)
    }

    fn managed_values(&self) -> [(&'static str, String); 6] {
        [
            ("server-ip", self.bind_address().to_owned()),
            ("server-port", self.game_port.to_string()),
            ("webui-enabled", "true".to_owned()),
            ("webui-public-url", self.public_url()),
            ("webui/content_url-port", self.web_port.to_string()),
            ("webui-tls-mode", "http".to_owned()),
        ]
    }

    /// Returns `text` with the managed keys set, every other line byte for byte as it was. Missing keys are added
    /// at the end; a trailing comment on a replaced line is kept; `news-port` is dropped. Refuses a file with a
    /// line it cannot safely edit around (nothing is written then).
    pub fn apply(&self, text: &str) -> Result<String, ConfigError> {
        self.validate()?;
        let lines = scan(text)?;
        let managed = self.managed_values();
        let mut out = String::with_capacity(text.len() + 160);
        let mut seen = [false; 6];
        for (raw, line) in text.split_inclusive('\n').zip(&lines) {
            match line {
                Line::Other => out.push_str(raw),
                Line::Key { name, comment } => {
                    if *name == "news-port" {
                        continue;
                    }
                    match managed.iter().position(|(k, _)| k == name) {
                        Some(i) => {
                            seen[i] = true;
                            let ending = &raw[raw.trim_end_matches(['\r', '\n']).len()..];
                            out.push_str(&format!("{}: {}", managed[i].0, quote(&managed[i].1)));
                            out.push_str(comment);
                            out.push_str(if ending.is_empty() { "\n" } else { ending });
                        }
                        None => out.push_str(raw),
                    }
                }
            }
        }
        let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
        for (i, (key, value)) in managed.iter().enumerate() {
            if !seen[i] {
                if !out.is_empty() && !out.ends_with('\n') {
                    out.push_str(newline);
                }
                out.push_str(&format!("{key}: {}{newline}", quote(value)));
            }
        }
        Ok(out)
    }

    /// Rewrites `config/server.yml` (created if missing) atomically. The old file is untouched when this fails.
    pub fn write(&self, layout: &ServerLayout) -> Result<(), ConfigError> {
        let current = match std::fs::read_to_string(layout.server_yml()) {
            Ok(text) => text,
            Err(e) if e.kind() == io::ErrorKind::NotFound => String::new(),
            Err(e) => return Err(e.into()),
        };
        let updated = self.apply(&current)?;
        if updated != current {
            write_atomic(&layout.server_yml(), updated.as_bytes())?;
        }
        Ok(())
    }
}

/// What the file currently says about the managed keys; a missing key is `None`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Current {
    pub bind: Option<String>,
    pub game_port: Option<u16>,
    pub web_port: Option<u16>,
    pub webui_enabled: Option<bool>,
    pub public_url: Option<String>,
    pub tls_mode: Option<String>,
}

impl Current {
    /// Whether the file is in the form [`HostConfig::apply`] writes: WebUI on, plain HTTP, the admin origin pinned
    /// to `127.0.0.1` on the web port, and an explicit web port (the engine's shipped file has none of that).
    pub fn is_launcher_configured(&self) -> bool {
        self.webui_enabled == Some(true)
            && self.tls_mode.as_deref() == Some("http")
            && self.web_port.is_some()
            && self.public_url.as_deref() == Some(self.host_config().public_url().as_str())
    }

    /// The launcher's view: LAN when the bind is not loopback; ports default to the engine's defaults.
    pub fn host_config(&self) -> HostConfig {
        HostConfig {
            lan: self.bind.as_deref().is_some_and(|b| b != LOOPBACK),
            game_port: self.game_port.unwrap_or(DEFAULT_GAME_PORT),
            web_port: self.web_port.unwrap_or(DEFAULT_WEB_PORT),
        }
    }
}

/// Reads the managed keys from `text`. Values the launcher does not understand come back as `None`.
pub fn read(text: &str) -> Result<Current, ConfigError> {
    let lines = scan(text)?;
    let mut current = Current::default();
    for (raw, line) in text.split_inclusive('\n').zip(&lines) {
        let Line::Key { name, .. } = line else {
            continue;
        };
        let value = value_of(raw);
        match *name {
            "server-ip" => current.bind = Some(value),
            "server-port" => current.game_port = value.parse().ok(),
            "webui/content_url-port" | "news-port" => {
                current.web_port = value.parse().ok().or(current.web_port)
            }
            "webui-enabled" => current.webui_enabled = value.parse().ok(),
            "webui-public-url" => current.public_url = Some(value),
            "webui-tls-mode" => current.tls_mode = Some(value),
            _ => {}
        }
    }
    Ok(current)
}

/// Reads `config/server.yml`; a missing file is all defaults.
pub fn read_file(layout: &ServerLayout) -> Result<Current, ConfigError> {
    match std::fs::read_to_string(layout.server_yml()) {
        Ok(text) => read(&text),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Current::default()),
        Err(e) => Err(e.into()),
    }
}

enum Line<'a> {
    /// Blank, comment or document marker: passed through.
    Other,
    Key {
        name: &'a str,
        /// The trailing ` # comment` (with its leading spaces), or empty.
        comment: &'a str,
    },
}

/// Classifies every line. Only flat `key: value` files are edited: anything indented, a list item, a line without
/// a colon, a repeated key or a tab refuses the whole file.
fn scan(text: &str) -> Result<Vec<Line<'_>>, ConfigError> {
    let mut lines = Vec::new();
    let mut seen: Vec<&str> = Vec::new();
    for (index, raw) in text.split_inclusive('\n').enumerate() {
        let number = index + 1;
        let line = raw.trim_end_matches(['\r', '\n']);
        let bad = |why| ConfigError::Malformed { line: number, why };
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed == "---" || trimmed == "..." {
            if line.contains('\t') && !trimmed.starts_with('#') && !trimmed.is_empty() {
                return Err(bad("tab character"));
            }
            lines.push(Line::Other);
            continue;
        }
        if line.starts_with([' ', '\t']) {
            return Err(bad("indented line (nested settings are not edited)"));
        }
        if line.starts_with(['-', '[', '{', '&', '*', '!', '|', '>', '%', '@', '`', '?']) {
            return Err(bad("not a plain `key: value` line"));
        }
        let Some((name, rest)) = line.split_once(':') else {
            return Err(bad("no `key: value`"));
        };
        if name.is_empty() || name.contains(['#', '"', '\'', ' ', '\t']) {
            return Err(bad("unsupported key"));
        }
        if !rest.is_empty() && !rest.starts_with(' ') {
            return Err(bad("expected a space after the colon"));
        }
        if seen.contains(&name) {
            return Err(bad("repeated key"));
        }
        seen.push(name);
        let comment_at = comment_start(rest);
        lines.push(Line::Key {
            name,
            comment: &rest[comment_at..],
        });
    }
    Ok(lines)
}

/// Where a trailing comment starts in the value part (` #` outside quotes), or the end.
fn comment_start(rest: &str) -> usize {
    let bytes = rest.as_bytes();
    let (mut single, mut double) = (false, false);
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'\'' if !double => single = !single,
            b'"' if !single => double = !double,
            b'#' if !single && !double && i > 0 && bytes[i - 1] == b' ' => {
                // include the spaces before the '#'
                let mut start = i;
                while start > 0 && bytes[start - 1] == b' ' {
                    start -= 1;
                }
                return start;
            }
            _ => {}
        }
    }
    rest.len()
}

/// The unquoted value of a `key: value` line without its comment.
fn value_of(raw: &str) -> String {
    let line = raw.trim_end_matches(['\r', '\n']);
    let rest = line.split_once(':').map_or("", |(_, r)| r);
    let value = rest[..comment_start(rest)].trim();
    let unquoted = value
        .strip_prefix('"')
        .and_then(|v| v.strip_suffix('"'))
        .or_else(|| value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')))
        .unwrap_or(value);
    unquoted.to_owned()
}

/// A plain scalar when that is unambiguous (letters, digits and `.-_/:`, no `: ` or ` #` possible), otherwise
/// double-quoted. The managed values are all of the first kind.
fn quote(value: &str) -> String {
    let plain = !value.is_empty()
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_' | b'/' | b':'));
    if plain {
        value.to_owned()
    } else {
        format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHIPPED: &str = "# Restart the server after editing.\nserver-ip: 0.0.0.0\nserver-port: 47624\nmax-clients: 16\nlog-mode: normal\n# Shown once in chat.\nwelcome-message: Welcome to Tortodrone!\n\nwebui-enabled: true\nwebui-public-url: 0.0.0.0\n# One shared HTTP port.\nwebui/content_url-port: 8080\nwebui-tls-mode: http\n";

    fn cfg(lan: bool) -> HostConfig {
        HostConfig {
            lan,
            game_port: 47624,
            web_port: 8080,
        }
    }

    #[test]
    fn rewrites_only_the_managed_lines_and_keeps_everything_else() {
        let out = cfg(false).apply(SHIPPED).unwrap();
        let expected = SHIPPED
            .replace("server-ip: 0.0.0.0", "server-ip: 127.0.0.1")
            .replace(
                "webui-public-url: 0.0.0.0",
                "webui-public-url: http://127.0.0.1:8080",
            );
        assert_eq!(out, expected);
        assert!(out.contains("# One shared HTTP port.\n"));
        assert_eq!(cfg(false).apply(&out).unwrap(), out, "idempotent");
    }

    #[test]
    fn lan_hosting_binds_every_interface_but_keeps_the_admin_origin_on_loopback() {
        let out = cfg(true).apply(SHIPPED).unwrap();
        assert!(out.contains("server-ip: 0.0.0.0\n"));
        assert!(out.contains("webui-public-url: http://127.0.0.1:8080\n"));
        let now = read(&out).unwrap();
        assert_eq!(now.host_config(), cfg(true));
    }

    #[test]
    fn missing_keys_are_added_and_unknown_keys_and_comments_survive() {
        let out = HostConfig {
            lan: false,
            game_port: 5000,
            web_port: 5001,
        }
        .apply("# mine\nfuture-setting: 3 # keep\nmax-clients: 4")
        .unwrap();
        assert!(out.starts_with("# mine\nfuture-setting: 3 # keep\nmax-clients: 4\n"));
        for line in [
            "server-ip: 127.0.0.1",
            "server-port: 5000",
            "webui-enabled: true",
            "webui-public-url: http://127.0.0.1:5001",
            "webui/content_url-port: 5001",
            "webui-tls-mode: http",
        ] {
            assert!(out.lines().any(|l| l == line), "{line}\n{out}");
        }
        assert_eq!(apply_twice(&out), out);
    }

    fn apply_twice(text: &str) -> String {
        HostConfig {
            lan: false,
            game_port: 5000,
            web_port: 5001,
        }
        .apply(text)
        .unwrap()
    }

    #[test]
    fn a_trailing_comment_and_crlf_line_endings_are_kept() {
        let text = "server-port: 1 # the game\r\nwebui-enabled: false\r\n";
        let out = cfg(false).apply(text).unwrap();
        assert!(out.starts_with("server-port: 47624 # the game\r\nwebui-enabled: true\r\n"));
        assert!(out.ends_with("webui-tls-mode: http\r\n"));
    }

    #[test]
    fn a_legacy_news_port_is_dropped() {
        let out = HostConfig {
            lan: false,
            game_port: 47624,
            web_port: 9000,
        }
        .apply("news-port: 8080\nmax-clients: 8\n")
        .unwrap();
        assert!(!out.contains("news-port"));
        assert!(out.contains("webui/content_url-port: 9000"));
        assert!(out.contains("max-clients: 8"));
    }

    #[test]
    fn a_corrupt_file_is_refused_and_not_touched() {
        for (text, line) in [
            ("server-port: 1\n  nested: 2\n", 2),
            ("a: 1\na: 2\n", 2),
            ("- item\n", 1),
            ("just words\n", 1),
            ("key:value\n", 1),
            ("a: 1\n\tb: 2\n", 2),
            ("\"quoted\": 1\n", 1),
        ] {
            match cfg(false).apply(text) {
                Err(ConfigError::Malformed { line: l, .. }) => assert_eq!(l, line, "{text:?}"),
                other => panic!("{text:?}: {other:?}"),
            }
            assert!(read(text).is_err());
        }
        let dir = tempfile::tempdir().unwrap();
        let layout = ServerLayout::new(dir.path());
        layout.ensure().unwrap();
        std::fs::write(layout.server_yml(), "a: 1\n  b\n").unwrap();
        assert!(cfg(false).write(&layout).is_err());
        assert_eq!(
            std::fs::read_to_string(layout.server_yml()).unwrap(),
            "a: 1\n  b\n"
        );
    }

    #[test]
    fn ports_are_validated() {
        for (game, web) in [(0, 8080), (47624, 0), (8080, 8080)] {
            let c = HostConfig {
                lan: false,
                game_port: game,
                web_port: web,
            };
            assert!(
                matches!(c.apply(""), Err(ConfigError::BadPort(_))),
                "{game} {web}"
            );
        }
    }

    #[test]
    fn the_engines_shipped_file_is_not_the_launchers_configuration() {
        let shipped = read(SHIPPED).unwrap();
        assert!(!shipped.is_launcher_configured());
        assert!(!Current::default().is_launcher_configured());
        let written = read(&cfg(true).apply(SHIPPED).unwrap()).unwrap();
        assert!(written.is_launcher_configured());
        // A hand edit that points the origin elsewhere is not.
        let edited = cfg(true)
            .apply(SHIPPED)
            .unwrap()
            .replace("http://127.0.0.1:8080", "http://192.168.1.5:8080");
        assert!(!read(&edited).unwrap().is_launcher_configured());
    }

    #[test]
    fn writes_the_file_atomically_and_reads_it_back() {
        let dir = tempfile::tempdir().unwrap();
        let layout = ServerLayout::new(dir.path());
        layout.ensure().unwrap();
        assert_eq!(read_file(&layout).unwrap(), Current::default());
        let wanted = HostConfig {
            lan: true,
            game_port: 40000,
            web_port: 40001,
        };
        wanted.write(&layout).unwrap();
        let now = read_file(&layout).unwrap();
        assert_eq!(now.host_config(), wanted);
        assert_eq!(now.webui_enabled, Some(true));
        assert_eq!(now.public_url.as_deref(), Some("http://127.0.0.1:40001"));
    }

    #[test]
    fn urls_use_the_loopback_address_the_engine_checks() {
        let c = cfg(true);
        assert_eq!(c.web_ui_url(), "http://127.0.0.1:8080/admin/");
        assert_eq!(c.content_url("192.168.1.20"), "http://192.168.1.20:8080/");
        assert_eq!(c.content_url("fd00::1"), "http://[fd00::1]:8080/");
    }
}
