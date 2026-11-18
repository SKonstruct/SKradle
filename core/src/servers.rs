//! The servers the game can connect to besides the official one, and which one is selected.
//!
//! A server is only connection settings (host, port, public key and two URLs): the game files stay the stock
//! client's, and at launch the connection keys in the `deployment.properties` inside `code/config.jar` are pointed
//! at the chosen server (Official's values when none is selected, so a previous server never lingers). This module
//! holds the data, the validation and the property text; rewriting the jar itself lives with the jar helpers.

use std::collections::BTreeMap;
use std::io;
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

use crate::getdown_txt::GetdownConfig;
use crate::jar::{Jar, JarError, Limits};
use crate::net::sha256_hex;

/// The official service. These are facts about the game's own configuration.
pub mod official {
    pub const HOST: &str = "game.spiralknights.com";
    pub const PORT: u16 = 47624;
    pub const CLIENT_URL: &str = "http://gamemedia2.spiralknights.com";
    pub const CONTENT_URL: &str = "http://content.spiralknights.com/";
    /// The service's RSA public key as `modulus#exponent` in hex.
    pub const PUBLIC_KEY: &str = "a5ed0dc3892b9472cfb668e236064e989e95945dad18f3d7e7d8e474d6e03de38bc044c3429b9ca649d0881d601c0eb8ffebc3756f0503f73a8ca1760943ea0e8921ad6f8102026586db3133844bbadbcfcfc666d23982d7684511fbf6cd8bb1d02a14270d0854098d16fe88f99c05825b0fe1b6fd497709106f2c418796aaf7aab7c92f26fcd9fbb3c43df48075fed8dd931273a7b0a333c8de5967797874c1944aed65b47f0792b273a529ac22a2dce08dad04eeebeeff67c7bc99b97682bff488038b28e24f4b5eea77ed966caede52f2c1ecf2b403110a9765daa81ddf718129a040823bead3a0bdca70ef6d08f483757a6d3b6e01fbbcb32006b7872bcd#10001";
}

/// The key under which the connection settings live in the game's properties file.
const KEYS: [&str; 6] = [
    "server_host",
    "server_ports",
    "datagram_ports",
    "key.public",
    "client_root_url",
    "content_url",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Server {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    /// `modulus#exponent` in hex. Blank uses Official's.
    #[serde(default, rename = "publicKey")]
    pub public_key: String,
    /// Optional root URL of the client files. Blank uses Official's.
    #[serde(default, rename = "clientUrl")]
    pub client_url: String,
    /// Optional content URL. Blank uses Official's.
    #[serde(default, rename = "contentUrl")]
    pub content_url: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Problem {
    Name,
    Host,
    Port,
    Key,
    Url,
    /// Another server (or Official) already has this name.
    Duplicate,
}

/// What is wrong with a server as typed, or `None` if it is fine. Fields are expected trimmed; `others` are the
/// other saved servers for the duplicate-name check.
pub fn validate(server: &Server, others: &[Server]) -> Option<Problem> {
    let name_len = server.name.chars().count();
    if name_len == 0 || name_len > 40 {
        return Some(Problem::Name);
    }
    if server.name.eq_ignore_ascii_case("official")
        || others
            .iter()
            .any(|o| o.id != server.id && o.name.to_lowercase() == server.name.to_lowercase())
    {
        return Some(Problem::Duplicate);
    }
    // The host ends up in a Java properties file and a socket address: no whitespace, no '/'.
    if server.host.is_empty()
        || server.host.len() > 253
        || !server.host.bytes().all(|b| {
            b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b':' | b'[' | b']' | b'-')
        })
    {
        return Some(Problem::Host);
    }
    if server.port == 0 {
        return Some(Problem::Port);
    }
    if !server.public_key.is_empty() && !is_public_key(&server.public_key) {
        return Some(Problem::Key);
    }
    if !is_http_url_or_blank(&server.client_url) || !is_http_url_or_blank(&server.content_url) {
        return Some(Problem::Url);
    }
    None
}

fn is_public_key(key: &str) -> bool {
    key.split_once('#').is_some_and(|(modulus, exponent)| {
        let hex = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_hexdigit());
        hex(modulus) && hex(exponent)
    })
}

fn is_http_url_or_blank(url: &str) -> bool {
    if url.is_empty() {
        return true;
    }
    let Some(rest) = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
    else {
        return false;
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    !authority.is_empty()
        && !authority.contains(['@', ' ', '\\'])
        && !url.chars().any(|c| c.is_control() || c.is_whitespace())
}

/// The game wants `scheme://host[:port]`, not the full client path a person pastes in. A missing or unknown scheme
/// becomes `http`. Returns an empty string for an empty input.
pub fn to_client_root(url: &str) -> String {
    let url = url.trim();
    if url.is_empty() {
        return String::new();
    }
    let (scheme, rest) = match url.split_once("://") {
        Some((scheme, rest)) if !scheme.is_empty() => (scheme.to_ascii_lowercase(), rest),
        _ => ("http".to_owned(), url),
    };
    let scheme = if scheme == "https" { "https" } else { "http" };
    format!("{scheme}://{}", rest.split('/').next().unwrap_or(""))
}

/// The connection settings to write for `server`, or for Official when `None`. Blank fields fall back to Official's.
pub fn desired_properties(server: Option<&Server>) -> BTreeMap<&'static str, String> {
    let port = server.map_or(official::PORT, |s| s.port).to_string();
    let non_blank = |s: &str| (!s.is_empty()).then(|| s.to_owned());
    BTreeMap::from([
        (
            "server_host",
            server.map_or_else(|| official::HOST.to_owned(), |s| s.host.clone()),
        ),
        ("server_ports", port.clone()),
        ("datagram_ports", port),
        (
            "key.public",
            server
                .and_then(|s| non_blank(&s.public_key))
                .unwrap_or_else(|| official::PUBLIC_KEY.to_owned()),
        ),
        (
            "client_root_url",
            server
                .and_then(|s| non_blank(&s.client_url))
                .map_or_else(|| official::CLIENT_URL.to_owned(), |u| to_client_root(&u)),
        ),
        (
            "content_url",
            server
                .and_then(|s| non_blank(&s.content_url))
                .unwrap_or_else(|| official::CONTENT_URL.to_owned()),
        ),
    ])
}

/// Replaces the connection keys in a Java properties text with `wanted`, adding missing ones at the end, and leaves
/// every other line (other keys, comments, blank lines, line endings) exactly as it was. Returns the new text, or
/// `None` when nothing needs to change.
pub fn apply_properties(text: &str, wanted: &BTreeMap<&'static str, String>) -> Option<String> {
    let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let mut out = String::with_capacity(text.len() + 128);
    let mut seen: Vec<&str> = Vec::new();
    let mut changed = false;
    for raw in logical_lines(text) {
        let line = raw.trim_end_matches(['\r', '\n']);
        let ending = &raw[line.len()..];
        // A key the caller gave no value for is not managed: its lines stay as they are.
        match property_key(line).filter(|k| KEYS.contains(k) && wanted.contains_key(k)) {
            Some(key) => {
                let key = *KEYS
                    .iter()
                    .find(|k| **k == key)
                    .expect("filtered to known keys");
                if seen.contains(&key) {
                    changed = true; // a duplicate definition of a key we manage is dropped
                    continue;
                }
                seen.push(key);
                let value = &wanted[key]; // present: filtered above
                let replacement = format!("{key}={}", escape_value(value));
                if line != replacement {
                    changed = true;
                }
                out.push_str(&replacement);
                out.push_str(if ending.is_empty() { newline } else { ending });
            }
            None => out.push_str(raw),
        }
    }
    for key in KEYS {
        if let (false, Some(value)) = (seen.contains(&key), wanted.get(key)) {
            if !out.is_empty() && !out.ends_with('\n') {
                out.push_str(newline);
            }
            out.push_str(&format!("{key}={}{newline}", escape_value(value)));
            changed = true;
        }
    }
    changed.then_some(out)
}

/// The text split into logical lines: a physical line that ends in an odd number of backslashes continues on the next
/// one (comments and blank lines never continue). Each piece keeps its line endings.
fn logical_lines(text: &str) -> Vec<&str> {
    let mut pieces = Vec::new();
    let (mut start, mut end) = (0, 0);
    let mut continuing = false;
    for raw in text.split_inclusive('\n') {
        let body = raw.trim_end_matches(['\r', '\n']);
        let alone = !continuing && {
            let first = body.trim_start_matches([' ', '\t', '\u{c}']);
            first.starts_with(['#', '!']) || first.trim().is_empty()
        };
        end += raw.len();
        let slashes = body.chars().rev().take_while(|&c| c == '\\').count();
        continuing = !alone && slashes % 2 == 1;
        if !continuing {
            pieces.push(&text[start..end]);
            start = end;
        }
    }
    if start < text.len() {
        pieces.push(&text[start..]); // a continuation that runs into the end of the file
    }
    pieces
}

/// The key of a `key=value`, `key: value` or `key value` line, or `None` for comments and blanks.
fn property_key(line: &str) -> Option<&str> {
    let line = line.trim_start();
    if line.is_empty() || line.starts_with(['#', '!']) {
        return None;
    }
    let end = line.find(['=', ':', ' ', '\t']).unwrap_or(line.len());
    Some(&line[..end])
}

/// Java properties escaping for a value: backslashes and line breaks; a leading space is protected.
fn escape_value(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for (i, c) in value.chars().enumerate() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ' ' if i == 0 => out.push_str("\\ "),
            c => out.push(c),
        }
    }
    out
}

/// True when Official has a newer game than the installed one. The updater would then replace `config.jar` after it
/// was edited and the game would silently connect to Official, so launching a private server must wait for the
/// update. A missing version on either side counts as no update (an offline private server still launches).
pub fn update_pending(local_getdown_txt: &str, remote_getdown_txt: &str) -> bool {
    let local = GetdownConfig::parse(local_getdown_txt).version;
    let remote = GetdownConfig::parse(remote_getdown_txt).version;
    matches!((local, remote), (Some(l), Some(r)) if l != r)
}

/// The stored list and the selection.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ServerList {
    pub servers: Vec<Server>,
    /// The id of the selected server; `None` is Official.
    pub selected: Option<String>,
}

#[derive(Serialize, Deserialize, Default)]
struct StoredList {
    #[serde(default)]
    servers: Vec<serde_json::Value>,
    #[serde(default)]
    selected: Option<String>,
}

impl ServerList {
    /// Parses stored JSON. A malformed or invalid entry is skipped instead of failing, so one bad record never hides the rest;
    /// unreadable JSON gives an empty list.
    pub fn parse(json: &str) -> Self {
        let stored: StoredList = serde_json::from_str(json).unwrap_or_default();
        let mut servers: Vec<Server> = Vec::new();
        for value in stored.servers {
            let Ok(server) = serde_json::from_value::<Server>(value) else {
                continue;
            };
            // The same rules a record has to meet to be saved: a hand-edited file must not get a host or an address
            // into the game's configuration that the launcher would have refused.
            if server.id.is_empty() || validate(&server, &servers).is_some() {
                continue;
            }
            if !servers.iter().any(|s| s.id == server.id) {
                servers.push(server);
            }
        }
        Self {
            servers,
            selected: stored.selected,
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::json!({ "servers": self.servers, "selected": self.selected }).to_string()
    }

    /// The selected server; `None` for Official, also when the selected one was deleted.
    pub fn selected_server(&self) -> Option<&Server> {
        let id = self.selected.as_deref()?;
        self.servers.iter().find(|s| s.id == id)
    }

    /// Adds the server or replaces the one with the same id. Returns what is wrong with it instead if it is invalid.
    pub fn save(&mut self, server: Server) -> Result<(), Problem> {
        if let Some(problem) = validate(&server, &self.servers) {
            return Err(problem);
        }
        match self.servers.iter_mut().find(|s| s.id == server.id) {
            Some(existing) => *existing = server,
            None => self.servers.push(server),
        }
        Ok(())
    }

    pub fn delete(&mut self, id: &str) {
        self.servers.retain(|s| s.id != id);
        if self.selected.as_deref() == Some(id) {
            self.selected = None;
        }
    }

    pub fn select(&mut self, id: Option<&str>) {
        self.selected = id
            .filter(|id| self.servers.iter().any(|s| s.id == *id))
            .map(str::to_owned);
    }

    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .map(|text| Self::parse(&text))
            .unwrap_or_default()
    }

    /// Writes through a temporary file (synced, then renamed) so a crash never leaves a half-written list.
    pub fn store(&self, path: &Path) -> io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        crate::jar::write_atomic(path, self.to_json().as_bytes())
    }
}

/// A fresh server id: 32 hex characters, unique within and across runs.
pub fn new_id() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    sha256_hex(format!("{nanos}-{}-{n}", std::process::id()).as_bytes())[..32].to_owned()
}

/// Whether `host` is on the local network: a private or link-local address, a `.local` name or a bare hostname
/// (which only resolves locally). Some platforms ask the player for permission before an app talks to these.
pub fn is_local_network_host(host: &str) -> bool {
    let h = host.trim().trim_matches(['[', ']']).to_lowercase();
    if h.is_empty() || h == "localhost" {
        return false;
    }
    // An address is classified as one (a zone such as `%en0` is not part of it).
    let bare = h.split('%').next().unwrap_or(&h);
    if let Ok(ip) = bare.parse::<IpAddr>() {
        return match ip {
            IpAddr::V4(v4) => v4.is_private() || v4.is_link_local(),
            // Unique local fc00::/7 and link-local fe80::/10.
            IpAddr::V6(v6) => {
                let first = v6.segments()[0];
                first & 0xfe00 == 0xfc00 || first & 0xffc0 == 0xfe80
            }
        };
    }
    // A name, perhaps with a port (`host:47624`): only the name counts.
    let name = h.rsplit_once(':').map_or(h.as_str(), |(name, _)| name);
    name.ends_with(".local") || !name.contains('.')
}

/// The jar entry that holds the connection settings.
const PROPERTIES_ENTRY: &str = "deployment.properties";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionOutcome {
    /// The game has no `code/config.jar` yet (not installed), so there is nothing to point.
    NotInstalled,
    /// The jar already had exactly these settings.
    Unchanged,
    /// The jar was rewritten.
    Updated,
}

#[derive(Debug)]
pub enum ConnectionError {
    Jar(JarError),
    /// `config.jar` has no `deployment.properties`.
    MissingProperties,
    /// A value cannot be written in the properties file's Latin-1 encoding.
    NotLatin1,
}

impl std::fmt::Display for ConnectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConnectionError::Jar(e) => write!(f, "{e}"),
            ConnectionError::MissingProperties => write!(f, "config.jar has no {PROPERTIES_ENTRY}"),
            ConnectionError::NotLatin1 => write!(
                f,
                "a server setting contains characters the game cannot read"
            ),
        }
    }
}

impl std::error::Error for ConnectionError {}

impl From<JarError> for ConnectionError {
    fn from(e: JarError) -> Self {
        ConnectionError::Jar(e)
    }
}

/// The path of the jar the connection settings live in.
pub fn config_jar_path(game_root: &Path) -> PathBuf {
    game_root.join("code").join("config.jar")
}

/// Points the game at `server` (Official when `None`) by rewriting the connection keys of the
/// `deployment.properties` inside `code/config.jar`. Every launch should write the full set, so a previous server's
/// settings never linger and a game update that restored the jar is healed. Only that one entry changes: class and
/// locale mods merged into the jar are untouched, and the jar is replaced atomically.
///
/// Order matters: applying mods rebuilds `config.jar` from the game's original, which discards these settings, so
/// call this AFTER `mods::apply`. Before launching a private server also check [`update_pending`]: the game's
/// updater would replace `config.jar` again after this edit and the game would connect to Official.
pub fn apply_connection(
    game_root: &Path,
    server: Option<&Server>,
) -> Result<ConnectionOutcome, ConnectionError> {
    let jar_path = config_jar_path(game_root);
    if !jar_path.is_file() {
        return Ok(ConnectionOutcome::NotInstalled);
    }
    let mut jar = Jar::read_path(&jar_path, &Limits::CODE_JAR)?;
    let current = jar
        .get(PROPERTIES_ENTRY)
        .ok_or(ConnectionError::MissingProperties)?;
    let text = latin1_decode(current);
    let Some(updated) = apply_properties(&text, &desired_properties(server)) else {
        return Ok(ConnectionOutcome::Unchanged);
    };
    jar.set(
        PROPERTIES_ENTRY,
        latin1_encode(&updated).ok_or(ConnectionError::NotLatin1)?,
    )?;
    jar.write_atomic(&jar_path)?;
    Ok(ConnectionOutcome::Updated)
}

/// Java properties files are ISO-8859-1: every byte is one character, so unknown text survives a round trip.
fn latin1_decode(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| char::from(b)).collect()
}

fn latin1_encode(text: &str) -> Option<Vec<u8>> {
    text.chars()
        .map(|c| u8::try_from(u32::from(c)).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server(id: &str, name: &str) -> Server {
        Server {
            id: id.into(),
            name: name.into(),
            host: "play.example.org".into(),
            port: 47624,
            public_key: String::new(),
            client_url: String::new(),
            content_url: String::new(),
        }
    }

    #[test]
    fn validation_catches_each_problem() {
        let ok = server("a", "My server");
        assert_eq!(validate(&ok, &[]), None);
        let mut s = ok.clone();
        s.name = String::new();
        assert_eq!(validate(&s, &[]), Some(Problem::Name));
        s.name = "x".repeat(41);
        assert_eq!(validate(&s, &[]), Some(Problem::Name));
        s.name = "OFFICIAL".into();
        assert_eq!(validate(&s, &[]), Some(Problem::Duplicate));
        let other = server("b", "my SERVER");
        assert_eq!(
            validate(&ok, std::slice::from_ref(&other)),
            Some(Problem::Duplicate)
        );
        assert_eq!(
            validate(
                &Server {
                    id: "b".into(),
                    ..ok.clone()
                },
                &[other]
            ),
            None,
            "same id is the same server"
        );
        for host in ["", "bad host", "a/b", "evil.example/x", "h\nost", "a;b"] {
            assert_eq!(
                validate(
                    &Server {
                        host: host.into(),
                        ..ok.clone()
                    },
                    &[]
                ),
                Some(Problem::Host),
                "{host:?}"
            );
        }
        assert_eq!(
            validate(
                &Server {
                    host: "[::1]".into(),
                    ..ok.clone()
                },
                &[]
            ),
            None
        );
        assert_eq!(
            validate(
                &Server {
                    port: 0,
                    ..ok.clone()
                },
                &[]
            ),
            Some(Problem::Port)
        );
        assert_eq!(
            validate(
                &Server {
                    public_key: "abc".into(),
                    ..ok.clone()
                },
                &[]
            ),
            Some(Problem::Key)
        );
        assert_eq!(
            validate(
                &Server {
                    public_key: "zz#10001".into(),
                    ..ok.clone()
                },
                &[]
            ),
            Some(Problem::Key)
        );
        assert_eq!(
            validate(
                &Server {
                    public_key: "abc123#10001".into(),
                    ..ok.clone()
                },
                &[]
            ),
            None
        );
        for url in [
            "ftp://x.org",
            "x.org",
            "http://",
            "http://u@evil/x",
            "https://a b.org",
            "http://x.org/\n",
        ] {
            assert_eq!(
                validate(
                    &Server {
                        client_url: url.into(),
                        ..ok.clone()
                    },
                    &[]
                ),
                Some(Problem::Url),
                "{url:?}"
            );
        }
        assert_eq!(
            validate(
                &Server {
                    content_url: "https://c.example.org/x".into(),
                    ..ok
                },
                &[]
            ),
            None
        );
    }

    #[test]
    fn client_roots_keep_only_scheme_and_authority() {
        assert_eq!(
            to_client_root("  https://Files.Example.org:8080/client/v2  "),
            "https://Files.Example.org:8080"
        );
        assert_eq!(to_client_root("HTTP://x.org/a"), "http://x.org");
        assert_eq!(to_client_root("x.org/a/b"), "http://x.org");
        assert_eq!(to_client_root("ftp://x.org/a"), "http://x.org");
        assert_eq!(to_client_root(""), "");
    }

    #[test]
    fn official_values_are_written_when_no_server_is_selected() {
        let d = desired_properties(None);
        assert_eq!(d["server_host"], official::HOST);
        assert_eq!(d["server_ports"], "47624");
        assert_eq!(d["datagram_ports"], "47624");
        assert_eq!(d["key.public"], official::PUBLIC_KEY);
        assert_eq!(d["client_root_url"], official::CLIENT_URL);
        assert_eq!(d["content_url"], official::CONTENT_URL);
        assert!(official::PUBLIC_KEY.ends_with("#10001") && is_public_key(official::PUBLIC_KEY));
    }

    #[test]
    fn a_servers_blank_fields_fall_back_to_officials() {
        let mut s = server("a", "x");
        s.port = 5000;
        s.client_url = "https://files.example.org/some/path".into();
        let d = desired_properties(Some(&s));
        assert_eq!(d["server_host"], "play.example.org");
        assert_eq!(d["server_ports"], "5000");
        assert_eq!(d["key.public"], official::PUBLIC_KEY);
        assert_eq!(d["client_root_url"], "https://files.example.org");
        assert_eq!(d["content_url"], official::CONTENT_URL);
    }

    #[test]
    fn properties_are_rewritten_in_place_preserving_everything_else() {
        let text = "# game config\nserver_host=old.example\nsome.other=value with spaces\n\nserver_ports = 1\r\nkey.public:abc\ncontent_url=http://content.spiralknights.com/\n";
        let wanted = desired_properties(None);
        let out = apply_properties(text, &wanted).unwrap();
        assert!(out.starts_with("# game config\nserver_host=game.spiralknights.com\n"));
        assert!(out.contains("\nsome.other=value with spaces\n\n"));
        assert!(
            out.contains("server_ports=47624\r\n"),
            "the line's own ending is kept"
        );
        assert!(
            out.contains("datagram_ports=47624"),
            "missing keys are appended"
        );
        assert!(out.contains("client_root_url=http://gamemedia2.spiralknights.com"));
        assert_eq!(
            apply_properties(&out, &wanted),
            None,
            "applying again changes nothing"
        );
    }

    #[test]
    fn properties_edge_cases() {
        let wanted = desired_properties(None);
        let out = apply_properties("", &wanted).unwrap();
        assert_eq!(out.lines().count(), 6);
        let dup = apply_properties("server_host=a\nserver_host=b\n", &wanted).unwrap();
        assert_eq!(
            dup.matches("server_host=").count(),
            1,
            "duplicate definitions collapse to one"
        );
        let no_final_newline = apply_properties("a=b", &wanted).unwrap();
        assert!(no_final_newline.starts_with("a=b\n"));
        let escaped = BTreeMap::from([("server_host", "a\\b c".to_owned())]);
        assert_eq!(escape_value(&escaped["server_host"]), "a\\\\b c");
        assert_eq!(escape_value(" lead"), "\\ lead");
    }

    #[test]
    fn a_pending_update_needs_both_versions_and_a_difference() {
        assert!(update_pending("version = 1\n", "version = 2\n"));
        assert!(!update_pending("version = 2\n", "version = 2\n"));
        assert!(
            !update_pending("version = 2\n", "nothing"),
            "unreadable remote: no update"
        );
        assert!(
            !update_pending("nothing", "version = 2\n"),
            "unreadable local: no update"
        );
    }

    #[test]
    fn the_list_skips_bad_records_and_round_trips() {
        let json = r#"{"servers":[
            {"id":"a","name":"A","host":"h.example","port":1},
            {"id":"","name":"no id","host":"h","port":1},
            {"id":"b","name":"B","host":"h","port":0},
            {"id":"a","name":"dup id","host":"h","port":2},
            "garbage",
            {"id":"c","name":"C","host":"c.example","port":3,"publicKey":"ab#10001"}
        ],"selected":"c"}"#;
        let list = ServerList::parse(json);
        let ids: Vec<_> = list.servers.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, ["a", "c"]);
        assert_eq!(list.selected_server().unwrap().name, "C");
        assert_eq!(ServerList::parse(&list.to_json()), list);
        assert_eq!(ServerList::parse("not json"), ServerList::default());
        assert_eq!(ServerList::parse(""), ServerList::default());
    }

    #[test]
    fn apply_properties_with_fewer_keys_leaves_the_others_alone_instead_of_panicking() {
        let mut wanted = BTreeMap::new();
        wanted.insert("server_host", "h.example".to_owned());
        let out = apply_properties("server_host=old\nport=1\nkey.public=abc\n", &wanted).unwrap();
        assert_eq!(out, "server_host=h.example\nport=1\nkey.public=abc\n");
        assert_eq!(apply_properties("server_host=h.example\n", &wanted), None);
        assert_eq!(
            apply_properties("", &BTreeMap::new()),
            None,
            "nothing wanted, nothing to change"
        );
    }

    #[test]
    fn the_list_drops_records_the_launcher_would_refuse_to_save() {
        let json = r#"{"servers":[
            {"id":"ok","name":"Fine","host":"h.example","port":1},
            {"id":"h1","name":"Spaced","host":"evil host","port":1},
            {"id":"h2","name":"Newline","host":"a\nserver=evil","port":1},
            {"id":"k","name":"Key","host":"h","port":1,"publicKey":"not a key"},
            {"id":"u","name":"Url","host":"h","port":1,"contentUrl":"file:///etc/passwd"},
            {"id":"u2","name":"Url2","host":"h","port":1,"clientUrl":"http://user@evil/"},
            {"id":"n","name":"Official","host":"h","port":1},
            {"id":"d","name":"fine","host":"h","port":1}
        ]}"#;
        let list = ServerList::parse(json);
        let ids: Vec<_> = list.servers.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, ["ok"]);
    }

    #[test]
    fn saving_selecting_and_deleting() {
        let mut list = ServerList::default();
        assert!(list.save(server("a", "A")).is_ok());
        assert_eq!(list.save(server("b", "a")), Err(Problem::Duplicate));
        assert!(
            list.save(Server {
                port: 99,
                ..server("a", "A")
            })
            .is_ok(),
            "same id replaces"
        );
        assert_eq!(list.servers.len(), 1);
        assert_eq!(list.servers[0].port, 99);
        list.select(Some("a"));
        assert!(list.selected_server().is_some());
        list.select(Some("missing"));
        assert!(
            list.selected_server().is_none(),
            "selecting an unknown id means Official"
        );
        list.select(Some("a"));
        list.delete("a");
        assert!(list.servers.is_empty() && list.selected.is_none());
    }

    #[test]
    fn the_list_is_stored_atomically() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub/servers.json");
        let mut list = ServerList::default();
        list.save(server("a", "A")).unwrap();
        list.store(&path).unwrap();
        assert_eq!(ServerList::load(&path), list);
        assert!(!crate::jar::temp_path(&path).exists());
        assert_eq!(
            ServerList::load(&dir.path().join("missing.json")),
            ServerList::default()
        );
    }

    fn game_with_config_jar(properties: &[u8]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let mut jar = Jar::default();
        jar.set(PROPERTIES_ENTRY, properties.to_vec()).unwrap();
        jar.set("com/example/Mod.class", vec![0xCA, 0xFE, 0xBA, 0xBE])
            .unwrap();
        jar.set("config/items.dat", vec![1, 2, 3]).unwrap();
        std::fs::create_dir_all(dir.path().join("code")).unwrap();
        jar.write_atomic(&config_jar_path(dir.path())).unwrap();
        dir
    }

    fn read_entry(game: &Path, name: &str) -> Vec<u8> {
        Jar::read_path(&config_jar_path(game), &Limits::CODE_JAR)
            .unwrap()
            .get(name)
            .unwrap()
            .to_vec()
    }

    #[test]
    fn the_connection_is_written_into_config_jar_and_only_there() {
        let game = game_with_config_jar(
            b"# deployment
server_host=old.example
other=keep me
",
        );
        let mut s = server("a", "A");
        s.port = 5000;
        assert_eq!(
            apply_connection(game.path(), Some(&s)).unwrap(),
            ConnectionOutcome::Updated
        );
        let props = String::from_utf8(read_entry(game.path(), PROPERTIES_ENTRY)).unwrap();
        assert!(props.contains("server_host=play.example.org\n"));
        assert!(props.contains("server_ports=5000"));
        assert!(props.contains("other=keep me"));
        assert_eq!(
            read_entry(game.path(), "com/example/Mod.class"),
            [0xCA, 0xFE, 0xBA, 0xBE],
            "other entries are untouched"
        );
        assert_eq!(read_entry(game.path(), "config/items.dat"), [1, 2, 3]);
        assert_eq!(
            apply_connection(game.path(), Some(&s)).unwrap(),
            ConnectionOutcome::Unchanged,
            "idempotent"
        );
    }

    #[test]
    fn switching_back_to_official_leaves_nothing_of_the_private_server() {
        let game = game_with_config_jar(
            b"server_host=x
",
        );
        apply_connection(game.path(), Some(&server("a", "A"))).unwrap();
        assert_eq!(
            apply_connection(game.path(), None).unwrap(),
            ConnectionOutcome::Updated
        );
        let props = String::from_utf8(read_entry(game.path(), PROPERTIES_ENTRY)).unwrap();
        assert!(props.contains(&format!("server_host={}", official::HOST)));
        assert!(!props.contains("play.example.org"));
    }

    #[test]
    fn non_ascii_bytes_in_the_properties_survive_the_rewrite() {
        let game = game_with_config_jar(b"title=caf\xe9\nserver_host=old\n");
        apply_connection(game.path(), None).unwrap();
        let props = read_entry(game.path(), PROPERTIES_ENTRY);
        assert!(
            props.windows(5).any(|w| w == b"caf\xe9\n"),
            "the Latin-1 byte is kept as one byte"
        );
    }

    #[test]
    fn a_missing_game_or_properties_entry_is_reported() {
        let empty = tempfile::tempdir().unwrap();
        assert_eq!(
            apply_connection(empty.path(), None).unwrap(),
            ConnectionOutcome::NotInstalled
        );
        let dir = tempfile::tempdir().unwrap();
        let mut jar = Jar::default();
        jar.set("x.txt", vec![1]).unwrap();
        std::fs::create_dir_all(dir.path().join("code")).unwrap();
        jar.write_atomic(&config_jar_path(dir.path())).unwrap();
        assert!(matches!(
            apply_connection(dir.path(), None),
            Err(ConnectionError::MissingProperties)
        ));
    }

    #[test]
    fn a_corrupt_jar_is_an_error_and_is_left_alone() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("code")).unwrap();
        std::fs::write(config_jar_path(dir.path()), b"not a zip").unwrap();
        assert!(matches!(
            apply_connection(dir.path(), None),
            Err(ConnectionError::Jar(_))
        ));
        assert_eq!(
            std::fs::read(config_jar_path(dir.path())).unwrap(),
            b"not a zip"
        );
    }

    #[test]
    fn ids_are_unique_hex() {
        let a = new_id();
        assert_eq!(a.len(), 32);
        assert!(a.bytes().all(|b| b.is_ascii_hexdigit()));
        assert_ne!(a, new_id());
    }

    #[test]
    fn local_network_hosts_are_recognised() {
        for yes in [
            "10.0.0.5",
            "172.16.0.1",
            "172.31.255.1",
            "192.168.1.20",
            "169.254.1.1",
            "fe80::1",
            "fd00::1",
            "mypc.local",
            "mypc",
            "[fe80::2]",
        ] {
            assert!(is_local_network_host(yes), "{yes}");
        }
        for no in [
            "8.8.8.8",
            "172.32.0.1",
            "play.example.org",
            "localhost",
            "",
            "2001:db8::1",
        ] {
            assert!(!is_local_network_host(no), "{no}");
        }
    }
}

#[cfg(test)]
mod logical_line_tests {
    use super::*;

    fn wanted(pairs: &[(&'static str, &str)]) -> BTreeMap<&'static str, String> {
        pairs.iter().map(|(k, v)| (*k, (*v).to_owned())).collect()
    }

    #[test]
    fn a_managed_key_that_wraps_over_several_lines_is_replaced_whole() {
        let text = "a=1\nkey.public=aaaa\\\n  bbbb\\\n  cccc\nb=2\n";
        let out = apply_properties(text, &wanted(&[("key.public", "new")])).unwrap();
        assert_eq!(out, "a=1\nkey.public=new\nb=2\n");
    }

    #[test]
    fn a_comment_ending_in_a_backslash_does_not_swallow_the_next_line() {
        let text = "# old note \\\nkey.public=aaaa\n";
        let out = apply_properties(text, &wanted(&[("key.public", "new")])).unwrap();
        assert_eq!(out, "# old note \\\nkey.public=new\n");
    }

    #[test]
    fn logical_lines_keep_their_line_endings_and_a_dangling_continuation() {
        assert_eq!(
            logical_lines("a=1\r\nb=2\\\r\n3\r\n"),
            ["a=1\r\n", "b=2\\\r\n3\r\n"]
        );
        assert_eq!(logical_lines("a=1\\\n"), ["a=1\\\n"]);
        assert_eq!(logical_lines("a=1\\\\\nb=2"), ["a=1\\\\\n", "b=2"]);
    }

    #[test]
    fn local_network_hosts_are_told_by_address_not_by_text_prefix() {
        for local in [
            "192.168.1.5",
            "10.0.0.1",
            "172.16.0.1",
            "169.254.1.1",
            "fe80::1",
            "febf::1",
            "fe80::1%en0",
            "[fd00::1]",
            "fc00::1",
            "myserver",
            "myserver:47624",
            "box.local",
        ] {
            assert!(is_local_network_host(local), "{local}");
        }
        for remote in [
            "fd-game.example.com:47624",
            "fc.example.org",
            "fec0::1",
            "2001:db8::1",
            "8.8.8.8",
            "172.32.0.1",
            "localhost",
            "",
        ] {
            assert!(!is_local_network_host(remote), "{remote}");
        }
    }
}
