//! Crash and error reports to Sentry. On by default, with an off switch in the settings; nothing is sent without a
//! DSN compiled in or configured, and nothing in this module can make the launcher fail: sending happens on a
//! background thread and every error is dropped.
//!
//! Reports are written by hand (a Sentry "envelope" with one event) instead of through the SDK, so the launcher
//! controls every byte that leaves: messages and backtraces are scrubbed of home folders, user names, IP addresses,
//! e-mail addresses and long tokens before anything is built. Crashes inside the C++ parts of Qt are not covered.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};

use regex::Regex;

use crate::net::{sha256_hex, Client};

const MAX_MESSAGE_CHARS: usize = 2000;
const MAX_BACKTRACE_CHARS: usize = 16_000;

/// A parsed Sentry DSN: `https://<public key>@<host>/<project id>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dsn {
    pub key: String,
    pub host: String,
    pub project_id: String,
}

impl Dsn {
    /// Accepts only `https` DSNs with a plain host (no port, path tricks or secret part).
    pub fn parse(text: &str) -> Option<Dsn> {
        let rest = text.trim().strip_prefix("https://")?;
        let (key, rest) = rest.split_once('@')?;
        let (host, project_id) = rest.split_once('/')?;
        let plain = |s: &str, extra: &[char]| {
            !s.is_empty()
                && s.chars()
                    .all(|c| c.is_ascii_alphanumeric() || extra.contains(&c))
        };
        (plain(key, &[])
            && plain(host, &['.', '-'])
            && plain(project_id, &[])
            && project_id.bytes().all(|b| b.is_ascii_digit()))
        .then(|| Dsn {
            key: key.to_owned(),
            host: host.to_owned(),
            project_id: project_id.to_owned(),
        })
    }

    pub fn envelope_url(&self) -> String {
        format!("https://{}/api/{}/envelope/", self.host, self.project_id)
    }

    pub fn auth_header(&self) -> String {
        format!(
            "Sentry sentry_version=7, sentry_key={}, sentry_client=skradle/{}",
            self.key,
            env!("CARGO_PKG_VERSION")
        )
    }
}

/// What to remove from text before it leaves the machine.
#[derive(Debug, Clone, Default)]
pub struct Scrubber {
    /// The home folder, replaced by `~` wherever it appears.
    pub home: Option<String>,
    /// The account name, replaced by `<user>` wherever it appears as a word.
    pub username: Option<String>,
}

impl Scrubber {
    pub fn from_environment() -> Self {
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .ok()
            .filter(|h| h.len() > 3);
        let username = std::env::var("USER")
            .or_else(|_| std::env::var("USERNAME"))
            .ok()
            .filter(|u| u.len() > 1);
        Self { home, username }
    }

    pub fn scrub(&self, text: &str) -> String {
        let mut out = text.to_owned();
        if let Some(home) = &self.home {
            out = out.replace(home.as_str(), "~");
            out = out.replace(&home.replace('\\', "/"), "~");
        }
        out = user_folder().replace_all(&out, "$1<user>$2").into_owned();
        if let Some(name) = &self.username {
            if let Ok(word) = Regex::new(&format!(r"\b{}\b", regex::escape(name))) {
                out = word.replace_all(&out, "<user>").into_owned();
            }
        }
        out = email().replace_all(&out, "<email>").into_owned();
        out = ipv4().replace_all(&out, "<ip>").into_owned();
        out = long_token().replace_all(&out, "<token>").into_owned();
        out
    }
}

fn user_folder() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)([/\\](?:users|home)[/\\])[^/\\\s:]+([/\\]?)").expect("valid regex")
    })
}

fn email() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}").expect("valid regex")
    })
}

fn ipv4() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b(?:\d{1,3}\.){3}\d{1,3}\b").expect("valid regex"))
}

/// Keys, tokens and hashes: 32 or more characters of hex or base64-looking text.
fn long_token() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"[A-Za-z0-9+/_=-]{32,}").expect("valid regex"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Fatal,
    Error,
}

#[derive(Debug, Clone)]
pub struct Report {
    pub level: Level,
    /// Short human text, for example the panic message.
    pub message: String,
    pub backtrace: Option<String>,
    pub tags: BTreeMap<String, String>,
}

/// The envelope (header line, item header line, event JSON) for one report. `event_id` is 32 hex characters.
pub fn build_envelope(
    report: &Report,
    dsn: &Dsn,
    event_id: &str,
    timestamp: u64,
    scrubber: &Scrubber,
    release: &str,
) -> String {
    let clip = |text: &str, max: usize| scrubber.scrub(&text.chars().take(max).collect::<String>());
    let mut tags: BTreeMap<&str, String> = BTreeMap::new();
    for (k, v) in &report.tags {
        tags.insert(k, clip(v, 200));
    }
    let mut event = serde_json::json!({
        "event_id": event_id,
        "timestamp": timestamp,
        "platform": "native",
        "level": match report.level { Level::Fatal => "fatal", Level::Error => "error" },
        "release": release,
        "message": { "formatted": clip(&report.message, MAX_MESSAGE_CHARS) },
        "tags": tags,
        "sdk": { "name": "skradle", "version": env!("CARGO_PKG_VERSION") },
        "contexts": { "os": { "name": std::env::consts::OS }, "device": { "arch": std::env::consts::ARCH } },
    });
    if let Some(backtrace) = &report.backtrace {
        event["extra"] = serde_json::json!({ "backtrace": clip(backtrace, MAX_BACKTRACE_CHARS) });
    }
    let payload = event.to_string();
    format!(
        "{{\"event_id\":\"{event_id}\",\"dsn\":\"https://{}@{}/{}\"}}\n{{\"type\":\"event\",\"length\":{}}}\n{payload}\n",
        dsn.key,
        dsn.host,
        dsn.project_id,
        payload.len()
    )
}

/// Sends reports when enabled and a DSN is known. Clone-cheap; share it with the panic hook.
pub struct Reporter {
    dsn: Option<Dsn>,
    enabled: AtomicBool,
    release: String,
    scrubber: Scrubber,
    counter: AtomicU64,
    client: Client,
}

impl Reporter {
    pub fn new(dsn: Option<Dsn>, enabled: bool, release: impl Into<String>) -> Arc<Self> {
        Arc::new(Self {
            dsn,
            enabled: AtomicBool::new(enabled),
            release: release.into(),
            scrubber: Scrubber::from_environment(),
            counter: AtomicU64::new(0),
            client: Client::new(),
        })
    }

    /// The settings switch.
    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Relaxed);
    }

    pub fn is_active(&self) -> bool {
        self.dsn.is_some() && self.enabled.load(Ordering::Relaxed)
    }

    /// Queues a report on a background thread; returns at once and never fails.
    pub fn capture(self: &Arc<Self>, report: Report) {
        if !self.is_active() {
            return;
        }
        let this = Arc::clone(self);
        let _ = std::thread::Builder::new()
            .name("skradle-report".into())
            .spawn(move || {
                let _ = this.send_now(&report);
            });
    }

    fn send_now(&self, report: &Report) -> Result<(), crate::net::NetError> {
        let Some(dsn) = &self.dsn else { return Ok(()) };
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        let id = self.event_id(now.as_nanos());
        let body = build_envelope(
            report,
            dsn,
            &id,
            now.as_secs(),
            &self.scrubber,
            &self.release,
        );
        self.client
            .post(
                &dsn.envelope_url(),
                &[("X-Sentry-Auth", &dsn.auth_header())],
                "application/x-sentry-envelope",
                body.as_bytes(),
                64 * 1024,
            )
            .map(|_| ())
    }

    fn event_id(&self, nanos: u128) -> String {
        let n = self.counter.fetch_add(1, Ordering::Relaxed);
        sha256_hex(format!("{nanos}-{}-{n}", std::process::id()).as_bytes())[..32].to_owned()
    }

    /// Reports panics (the previous hook still runs, so the usual message is printed).
    pub fn install_panic_hook(self: &Arc<Self>) {
        let this = Arc::clone(self);
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            previous(info);
            if this.is_active() {
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
                let mut tags = BTreeMap::new();
                tags.insert("kind".to_owned(), "panic".to_owned());
                tags.insert("location".to_owned(), location);
                // Send before the process may die: a panic usually ends the program.
                let report = Report {
                    level: Level::Fatal,
                    message,
                    backtrace: Some(std::backtrace::Backtrace::force_capture().to_string()),
                    tags,
                };
                let _ = this.send_now(&report);
            }
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scrubber() -> Scrubber {
        Scrubber {
            home: Some("/Users/rayhan".into()),
            username: Some("rayhan".into()),
        }
    }

    #[test]
    fn parses_only_plain_https_dsns() {
        let dsn = Dsn::parse("https://abc123def456@o1.ingest.example.io/4501234").unwrap();
        assert_eq!(
            dsn,
            Dsn {
                key: "abc123def456".into(),
                host: "o1.ingest.example.io".into(),
                project_id: "4501234".into()
            }
        );
        assert_eq!(
            dsn.envelope_url(),
            "https://o1.ingest.example.io/api/4501234/envelope/"
        );
        assert!(dsn.auth_header().contains("sentry_key=abc123def456"));
        for bad in [
            "",
            "http://k@host.io/1",
            "https://k@host.io:8080/1",
            "https://k:secret@host.io/1",
            "https://k@host.io/abc",
            "https://k@host.io/1/extra",
            "https://@host.io/1",
            "https://k@/1",
            "https://k@ho st.io/1",
        ] {
            assert_eq!(Dsn::parse(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn scrubs_homes_users_ips_emails_and_tokens() {
        let s = scrubber();
        assert_eq!(
            s.scrub("/Users/rayhan/Library/Application Support/Steam/x.jar"),
            "~/Library/Application Support/Steam/x.jar"
        );
        assert_eq!(
            s.scrub(r"C:\Users\Someone\AppData\Local\x"),
            r"C:\Users\<user>\AppData\Local\x"
        );
        assert_eq!(
            s.scrub("/home/bob/.local/share/x"),
            "/home/<user>/.local/share/x"
        );
        assert_eq!(
            s.scrub("connect to 192.168.1.20:47624 failed"),
            "connect to <ip>:47624 failed"
        );
        assert_eq!(
            s.scrub("mail me at someone@example.com now"),
            "mail me at <email> now"
        );
        assert_eq!(
            s.scrub("key 3f2a9c4e8b7d1a6f5e0c9b8a7d6e5f4a3b2c1d0e done"),
            "key <token> done"
        );
        assert_eq!(s.scrub("hello rayhan, welcome"), "hello <user>, welcome");
        assert_eq!(
            s.scrub("version 1.2.3 is fine"),
            "version 1.2.3 is fine",
            "versions are not IPs"
        );
        assert_eq!(s.scrub("nothing sensitive"), "nothing sensitive");
    }

    #[test]
    fn scrubbing_handles_windows_home_in_both_slash_styles() {
        let s = Scrubber {
            home: Some(r"C:\Users\me".into()),
            username: None,
        };
        assert_eq!(s.scrub(r"C:\Users\me\Documents\f"), r"~\Documents\f");
        assert_eq!(s.scrub("C:/Users/me/Documents/f"), "~/Documents/f");
    }

    #[test]
    fn the_envelope_is_well_formed_and_scrubbed() {
        let dsn = Dsn::parse("https://abc@sentry.example.io/42").unwrap();
        let mut tags = BTreeMap::new();
        tags.insert("kind".to_owned(), "panic".to_owned());
        let report = Report {
            level: Level::Fatal,
            message: "failed at /Users/rayhan/secret/file from 10.0.0.5".into(),
            backtrace: Some("frame 0: /Users/rayhan/src/main.rs:10".into()),
            tags,
        };
        let id = "0123456789abcdef0123456789abcdef";
        let envelope = build_envelope(&report, &dsn, id, 1_795_000_000, &scrubber(), "0.1.0");
        let lines: Vec<&str> = envelope.lines().collect();
        assert_eq!(lines.len(), 3);
        let header: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(header["event_id"], id);
        let item: serde_json::Value = serde_json::from_str(lines[1]).unwrap();
        assert_eq!(item["type"], "event");
        assert_eq!(
            item["length"].as_u64().unwrap() as usize,
            lines[2].len(),
            "the declared length is the payload's"
        );
        let event: serde_json::Value = serde_json::from_str(lines[2]).unwrap();
        assert_eq!(event["level"], "fatal");
        assert_eq!(event["release"], "0.1.0");
        assert_eq!(
            event["message"]["formatted"],
            "failed at ~/secret/file from <ip>"
        );
        assert_eq!(event["extra"]["backtrace"], "frame 0: ~/src/main.rs:10");
        assert!(!envelope.contains("rayhan") && !envelope.contains("10.0.0.5"));
    }

    #[test]
    fn long_messages_are_clipped() {
        let dsn = Dsn::parse("https://abc@sentry.example.io/42").unwrap();
        let report = Report {
            level: Level::Error,
            message: "x".repeat(10_000),
            backtrace: None,
            tags: BTreeMap::new(),
        };
        let envelope = build_envelope(
            &report,
            &dsn,
            "0".repeat(32).as_str(),
            1,
            &Scrubber::default(),
            "0",
        );
        let event: serde_json::Value =
            serde_json::from_str(envelope.lines().nth(2).unwrap()).unwrap();
        assert!(event["message"]["formatted"].as_str().unwrap().len() <= MAX_MESSAGE_CHARS);
    }

    #[test]
    fn nothing_is_sent_when_disabled_or_without_a_dsn() {
        let none = Reporter::new(None, true, "0.1.0");
        assert!(!none.is_active());
        let dsn = Dsn::parse("https://abc@sentry.invalid/42");
        let off = Reporter::new(dsn.clone(), false, "0.1.0");
        assert!(!off.is_active());
        off.set_enabled(true);
        assert!(off.is_active());
        off.set_enabled(false);
        assert!(!off.is_active());
        // Capturing while inactive returns at once and does nothing.
        none.capture(Report {
            level: Level::Error,
            message: "x".into(),
            backtrace: None,
            tags: BTreeMap::new(),
        });
    }

    #[test]
    fn event_ids_are_32_hex_and_unique() {
        let reporter = Reporter::new(None, true, "0");
        let a = reporter.event_id(1);
        let b = reporter.event_id(1);
        assert_eq!(a.len(), 32);
        assert!(a.bytes().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b);
    }
}
