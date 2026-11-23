//! Live events for the Home page, read from the same feed the owner's mobile launchers read.
//!
//! The feed is third-party, cleartext and optional: `http://flamingo.knightlauncher.com:6060/server-list/?machineId=
//! skradle` (plain HTTP, the service has no TLS listener; see [`crate::net`] for the one narrow exception that
//! allows it). The launcher must work normally when it is unreachable. Everything it returns is untrusted data: the
//! only things used are text for the card (control characters removed, lengths capped) and `https://` links and
//! images (images are size capped, PNG or JPEG only, with a pixel cap, and cached on disk).
//!
//! Behaviour, as in the mobile launchers:
//! - the answer is `{"serverlist":[{"name":"Official", ..., "events":[{"banner","content","link","startsAt",
//!   "endsAt"}], "defaultEventBanner","defaultEventContent","defaultEventLink","defaultEventStartsAt",
//!   "defaultEventEndsAt"}]}`; the service writes a trailing comma after the last field of every event, which is
//!   tolerated ([`strip_trailing_commas`]);
//! - the server named `Official` is used (the first one when there is none); its events that already ended are
//!   dropped (upcoming ones stay) and the `defaultEvent*` fields are used only when nothing else is left;
//! - `content` is `title|description` (a literal `\n` in the description is a line break), `banner` is `url|dimming`;
//!   an event without a title (or titled `null`) is skipped; times are milliseconds as strings, 0 means none.
//!
//! The last good answer is kept on disk; when the request fails it is shown (marked offline).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::jar::write_atomic;
use crate::net::{
    is_plain_https_url, sha256_hex, Client, NetError, EVENTS_FEED_HOST, EVENTS_FEED_PATH,
    EVENTS_FEED_PORT,
};

/// A fixed label, not a device id: the service uses it for beta codes, which SKradle never redeems.
pub const MACHINE_ID: &str = "skradle";

pub const MAX_FEED_BYTES: u64 = 256 * 1024;
pub const MAX_IMAGE_BYTES: u64 = 1024 * 1024;
const MAX_IMAGE_SIDE: u32 = 4096;
const MAX_IMAGE_PIXELS: u64 = 8 * 1024 * 1024;
const MAX_CACHE_BYTES: u64 = 512 * 1024;
/// At most this many events are kept.
pub const MAX_EVENTS: usize = 8;
pub const MAX_TITLE_CHARS: usize = 120;
pub const MAX_DESCRIPTION_CHARS: usize = 600;
pub const MAX_URL_CHARS: usize = 512;

/// The feed's address.
pub fn feed_url() -> String {
    format!("http://{EVENTS_FEED_HOST}:{EVENTS_FEED_PORT}{EVENTS_FEED_PATH}?machineId={MACHINE_ID}")
}

/// One event, validated and cleaned.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    pub title: String,
    /// May hold line breaks; empty when the feed gave none.
    pub description: String,
    /// A plain `https://` image address; `None` when absent or not HTTPS.
    pub image_url: Option<String>,
    /// A plain `https://` address for the Learn more button; `None` shows no button.
    pub link: Option<String>,
    /// Unix seconds; `None` when not given.
    pub starts_at: Option<i64>,
    /// Unix seconds; `None` for an event without an end.
    pub ends_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventsError {
    /// The request failed (offline, timeout, other status, too large).
    Network(String),
    /// The answer was not the expected JSON.
    Format(String),
}

impl std::fmt::Display for EventsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EventsError::Network(why) => write!(f, "could not reach the events feed: {why}"),
            EventsError::Format(why) => write!(f, "unexpected events answer: {why}"),
        }
    }
}

impl std::error::Error for EventsError {}

/// The request, so tests (and the screenshot driver) answer without a network. Returns the raw feed text.
pub trait EventsApi: Send + Sync {
    fn fetch(&self) -> Result<String, EventsError>;
}

/// The real feed, through the one plain-HTTP exception in [`crate::net`].
#[derive(Default)]
pub struct HttpEvents;

impl EventsApi for HttpEvents {
    fn fetch(&self) -> Result<String, EventsError> {
        Client::get_text_insecure_for_events(&feed_url(), MAX_FEED_BYTES)
            .map_err(|e: NetError| EventsError::Network(e.to_string()))
    }
}

/// Removes a comma that directly precedes a `}` or `]` (outside strings). The service writes `"endsAt":"...",}`.
pub fn strip_trailing_commas(json: &str) -> String {
    let mut out = String::with_capacity(json.len());
    let (mut in_string, mut escaped) = (false, false);
    for c in json.chars() {
        if in_string {
            match c {
                _ if escaped => escaped = false,
                '\\' => escaped = true,
                '"' => in_string = false,
                _ => {}
            }
        } else if c == '"' {
            in_string = true;
        } else if c == '}' || c == ']' {
            let kept = out.trim_end().len();
            if out[..kept].ends_with(',') {
                out.truncate(kept - 1);
            }
        }
        out.push(c);
    }
    out
}

/// A field as text: strings as they are, numbers in decimal form, anything else empty.
fn field(object: &serde_json::Value, key: &str) -> String {
    match object.get(key) {
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(serde_json::Value::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}

/// Text for a single line: control characters become spaces, runs of white space one space, cut at a word.
fn clean_line(text: &str, max: usize) -> String {
    let spaced: String = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    crate::steam_api::truncate_words(
        &spaced.split_whitespace().collect::<Vec<_>>().join(" "),
        max,
    )
}

/// Text with line breaks: each line cleaned like [`clean_line`], empty lines dropped, the whole cut at `max`.
fn clean_block(text: &str, max: usize) -> String {
    let lines: Vec<String> = text
        .split('\n')
        .map(|l| clean_line(l, max))
        .filter(|l| !l.is_empty())
        .collect();
    crate::steam_api::truncate_words(&lines.join("\n"), max)
}

/// Milliseconds in text to Unix seconds; 0, negative or not a number means none.
fn seconds(ms: &str) -> Option<i64> {
    ms.trim()
        .parse::<i64>()
        .ok()
        .filter(|&m| m > 0)
        .map(|m| m / 1000)
}

fn https_only(url: &str) -> Option<String> {
    let url = url.trim();
    (url.chars().count() <= MAX_URL_CHARS && is_plain_https_url(url)).then(|| url.to_owned())
}

/// One event, or `None` when it has no title to show.
fn event(banner: &str, content: &str, link: &str, starts: &str, ends: &str) -> Option<Event> {
    let (head, body) = match content.split_once('|') {
        Some((head, body)) => (head, Some(body)),
        None => (content, None),
    };
    let title = clean_line(head, MAX_TITLE_CHARS);
    if title.is_empty() || title.eq_ignore_ascii_case("null") {
        return None;
    }
    let description = body
        .map(|b| clean_block(&b.replace("\\n", "\n"), MAX_DESCRIPTION_CHARS))
        .unwrap_or_default();
    Some(Event {
        title,
        description,
        image_url: https_only(banner.split('|').next().unwrap_or("")),
        link: https_only(link),
        starts_at: seconds(starts),
        ends_at: seconds(ends),
    })
}

/// Parses the feed at `now` (Unix seconds): the Official server's events that have not ended, or the default event
/// when none is left. A document that is not the expected JSON is an error; a valid one without events is empty.
pub fn parse_feed(text: &str, now: i64) -> Result<Vec<Event>, EventsError> {
    let json: serde_json::Value = serde_json::from_str(&strip_trailing_commas(text))
        .map_err(|e| EventsError::Format(e.to_string()))?;
    let servers = json
        .get("serverlist")
        .and_then(|s| s.as_array())
        .ok_or_else(|| EventsError::Format("no server list".into()))?;
    let objects = || servers.iter().filter(|s| s.is_object());
    let Some(server) = objects()
        .find(|s| field(s, "name") == "Official")
        .or_else(|| objects().next())
    else {
        return Ok(vec![]);
    };

    let listed = server
        .get("events")
        .and_then(|e| e.as_array())
        .map(Vec::as_slice)
        .unwrap_or_default();
    let events: Vec<Event> = listed
        .iter()
        .filter(|e| e.is_object())
        .filter_map(|e| {
            event(
                &field(e, "banner"),
                &field(e, "content"),
                &field(e, "link"),
                &field(e, "startsAt"),
                &field(e, "endsAt"),
            )
        })
        .filter(|e| e.ends_at.is_none_or(|end| end > now))
        .take(MAX_EVENTS)
        .collect();
    if !events.is_empty() {
        return Ok(events);
    }
    Ok(event(
        &field(server, "defaultEventBanner"),
        &field(server, "defaultEventContent"),
        &field(server, "defaultEventLink"),
        &field(server, "defaultEventStartsAt"),
        &field(server, "defaultEventEndsAt"),
    )
    .into_iter()
    .collect())
}

/// What the countdown line says, as numbers. The words are the UI's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Countdown {
    /// Neither a start nor an end is known: nothing to count.
    None,
    /// Not started yet: `Starts in ...`.
    Starts { days: i64, hours: i64, minutes: i64 },
    /// A day or more left: `Ends in 2d 5h`.
    Ends { days: i64, hours: i64, minutes: i64 },
    /// Less than a day left: `Ends today, 5h 12m left`.
    EndsToday { days: i64, hours: i64, minutes: i64 },
    /// The end has passed.
    Ended,
}

fn split(secs: i64) -> (i64, i64, i64) {
    let secs = secs.max(0);
    (secs / 86_400, secs % 86_400 / 3600, secs % 3600 / 60)
}

/// The countdown at `now` (Unix seconds). Whole minutes are shown, rounded down; the UI refreshes each minute.
pub fn countdown(starts_at: Option<i64>, ends_at: Option<i64>, now: i64) -> Countdown {
    if let Some(start) = starts_at.filter(|&s| s > now) {
        let (days, hours, minutes) = split(start - now);
        return Countdown::Starts {
            days,
            hours,
            minutes,
        };
    }
    let Some(end) = ends_at else {
        return Countdown::None;
    };
    if end <= now {
        return Countdown::Ended;
    }
    let (days, hours, minutes) = split(end - now);
    if days == 0 {
        Countdown::EndsToday {
            days,
            hours,
            minutes,
        }
    } else {
        Countdown::Ends {
            days,
            hours,
            minutes,
        }
    }
}

/// What the Home page shows for the events.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EventsFeed {
    pub events: Vec<Event>,
    /// The request failed; `events` come from the last good answer (or are empty when there is none).
    pub offline: bool,
    /// When `events` were fetched, Unix seconds (0 when unknown).
    pub fetched_at: i64,
}

#[derive(Serialize, Deserialize)]
struct CacheFile {
    #[serde(rename = "fetchedAt")]
    fetched_at: i64,
    /// The raw answer, parsed again on every read: the file sits in a folder the player can edit.
    body: String,
}

fn read_cache(path: &Path, now: i64) -> Option<(Vec<Event>, i64)> {
    let bytes = crate::bounded::read_regular(path, MAX_CACHE_BYTES).ok()?;
    let cache: CacheFile = serde_json::from_slice(&bytes).ok()?;
    let events = parse_feed(&cache.body, now).ok()?;
    Some((events, cache.fetched_at))
}

fn write_cache(path: &Path, body: &str, now: i64) {
    let file = CacheFile {
        fetched_at: now,
        body: body.to_owned(),
    };
    if let Ok(mut bytes) = serde_json::to_vec_pretty(&file) {
        bytes.push(b'\n');
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        // A cache that cannot be written only costs the offline fallback.
        let _ = write_atomic(path, &bytes);
    }
}

/// The last good answer straight from disk, with no request: the Home page shows it at once while the request runs.
/// `None` when nothing usable is saved. Not marked offline: nothing is known about the network yet.
pub fn cached_feed(cache: &Path, now: i64) -> Option<EventsFeed> {
    let (events, fetched_at) = read_cache(cache, now)?;
    Some(EventsFeed {
        events,
        offline: false,
        fetched_at,
    })
}

/// Fetches the events. An answer with events is cached; an answer that is valid but empty means no events (the card
/// hides); a failed or unreadable answer returns the cache, marked offline. `now` is Unix seconds.
pub fn load_events(api: &dyn EventsApi, cache: &Path, now: i64) -> EventsFeed {
    if let Ok(body) = api.fetch() {
        // Parsed before it is kept: a captive portal's answer must not replace a good cache.
        if let Ok(events) = parse_feed(&body, now) {
            if !events.is_empty() {
                write_cache(cache, &body, now);
            }
            return EventsFeed {
                events,
                offline: false,
                fetched_at: now,
            };
        }
    }
    let (events, fetched_at) = read_cache(cache, now).unwrap_or_default();
    EventsFeed {
        events,
        offline: true,
        fetched_at,
    }
}

// ---- Banner images --------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageKind {
    Png,
    Jpeg,
}

/// The kind and size of a PNG or JPEG, from its header; `None` for anything else.
fn image_header(bytes: &[u8]) -> Option<(ImageKind, u32, u32)> {
    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n";
    let be32 = |at: usize| -> Option<u32> {
        Some(u32::from_be_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
    };
    let be16 = |at: usize| -> Option<u32> {
        Some(u32::from(u16::from_be_bytes(
            bytes.get(at..at + 2)?.try_into().ok()?,
        )))
    };
    if bytes.starts_with(PNG) {
        if bytes.get(12..16)? != b"IHDR" {
            return None;
        }
        return Some((ImageKind::Png, be32(16)?, be32(20)?));
    }
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        let mut at = 2;
        loop {
            while *bytes.get(at)? != 0xFF {
                at += 1;
            }
            while *bytes.get(at)? == 0xFF {
                at += 1;
            }
            let marker = *bytes.get(at)?;
            at += 1;
            match marker {
                0xD8 | 0x01 | 0xD0..=0xD7 => continue,
                0xD9 | 0xDA => return None,
                // Start-of-frame markers carry the size.
                0xC0..=0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF => {
                    return Some((ImageKind::Jpeg, be16(at + 5)?, be16(at + 3)?));
                }
                _ => at += usize::try_from(be16(at)?).ok()?,
            }
        }
    }
    None
}

/// Whether `bytes` is a PNG or JPEG within the size caps (bytes, side length, pixel count).
pub fn is_acceptable_image(bytes: &[u8]) -> bool {
    if bytes.is_empty() || bytes.len() as u64 > MAX_IMAGE_BYTES {
        return false;
    }
    image_header(bytes).is_some_and(|(_, w, h)| {
        w > 0
            && h > 0
            && w <= MAX_IMAGE_SIDE
            && h <= MAX_IMAGE_SIDE
            && u64::from(w) * u64::from(h) <= MAX_IMAGE_PIXELS
    })
}

/// The extension a cached banner of this kind gets: what the file really is, so the folder is readable by hand.
fn extension(kind: ImageKind) -> &'static str {
    match kind {
        ImageKind::Png => "png",
        ImageKind::Jpeg => "jpg",
    }
}

/// The extensions a cached banner can have: by kind, and `img`, what earlier versions wrote for every banner.
const CACHE_EXTENSIONS: [&str; 3] = ["png", "jpg", "img"];

/// Where a banner of this kind is cached: a name derived from the address (never from the feed's text or the
/// address's own extension, which cannot be trusted) plus the extension of what the file really is.
pub fn image_cache_path(dir: &Path, url: &str, kind: ImageKind) -> PathBuf {
    dir.join(format!(
        "{}.{}",
        &sha256_hex(url.as_bytes())[..32],
        extension(kind)
    ))
}

/// The cached file of a banner, downloading it with `download` when it is not there (or no longer acceptable).
/// `None` when it cannot be had; the card then shows no picture.
pub fn cached_image(
    dir: &Path,
    url: &str,
    download: &dyn Fn(&str) -> Option<Vec<u8>>,
) -> Option<PathBuf> {
    let stem = &sha256_hex(url.as_bytes())[..32];
    for extension in CACHE_EXTENSIONS {
        let path = dir.join(format!("{stem}.{extension}"));
        if let Ok(bytes) = crate::bounded::read_regular(&path, MAX_IMAGE_BYTES) {
            if is_acceptable_image(&bytes) {
                return Some(path);
            }
        }
    }
    let bytes = download(url).filter(|b| is_acceptable_image(b))?;
    let (kind, _, _) = image_header(&bytes)?;
    std::fs::create_dir_all(dir).ok()?;
    let path = image_cache_path(dir, url, kind);
    write_atomic(&path, &bytes).ok()?;
    Some(path)
}

/// The real download: HTTPS only, size capped, short timeouts.
pub fn download_image(url: &str) -> Option<Vec<u8>> {
    let client = Client::with_timeouts(
        std::time::Duration::from_secs(4),
        std::time::Duration::from_secs(10),
    );
    client.get_bytes(url, MAX_IMAGE_BYTES).ok()
}

/// Removes cached banners that none of `keep` refers to, so the folder cannot grow.
pub fn prune_images(dir: &Path, keep: &[PathBuf]) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let ours = path
            .extension()
            .is_some_and(|e| CACHE_EXTENSIONS.iter().any(|x| e == *x));
        if ours && !keep.contains(&path) {
            let _ = std::fs::remove_file(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../tests/fixtures/live_events.json");
    /// 2026-11-20 about noon UTC, between the fixture's start and end times.
    const NOW: i64 = 1_793_950_000;

    struct Fake(Result<String, EventsError>);
    impl EventsApi for Fake {
        fn fetch(&self) -> Result<String, EventsError> {
            self.0.clone()
        }
    }

    #[test]
    fn the_saved_answer_is_available_without_a_request() {
        let dir = tempfile::tempdir().unwrap();
        let cache = dir.path().join("live-events.json");
        assert!(cached_feed(&cache, NOW).is_none());
        let _ = load_events(&Fake(Ok(FIXTURE.to_owned())), &cache, NOW);
        let saved = cached_feed(&cache, NOW).expect("saved");
        assert!(!saved.events.is_empty());
        assert!(!saved.offline);
        assert_eq!(saved.fetched_at, NOW);
    }

    #[test]
    fn the_feed_address_is_the_one_allowed_exception() {
        let url = feed_url();
        assert_eq!(
            url,
            "http://flamingo.knightlauncher.com:6060/server-list/?machineId=skradle"
        );
        assert!(crate::net::is_events_feed_url(&url));
    }

    #[test]
    fn trailing_commas_go_but_commas_in_strings_stay() {
        assert_eq!(
            strip_trailing_commas(r#"{"a":"x,}",  "b":[1,2, ],}"#),
            r#"{"a":"x,}",  "b":[1,2]}"#
        );
        assert_eq!(
            strip_trailing_commas(r#"{"a":"q\",}"}"#),
            r#"{"a":"q\",}"}"#
        );
    }

    #[test]
    fn the_official_servers_running_and_upcoming_events_are_kept_in_order() {
        let events = parse_feed(FIXTURE, NOW).unwrap();
        let titles: Vec<&str> = events.iter().map(|e| e.title.as_str()).collect();
        assert_eq!(
            titles,
            ["Spiked Beasts Rampage!", "A Chill Arrives", "Plain links"]
        );
        let first = &events[0];
        assert_eq!(
            first.description,
            "Mysterious energy spikes fill the Clockworks...\nSnatch them up or face super-spiked showdowns!"
        );
        assert_eq!(
            first.image_url.as_deref(),
            Some("https://cdn.example.org/spiked.png")
        );
        assert_eq!(first.link.as_deref(), Some("https://example.org/spiked"));
        assert_eq!(first.starts_at, Some(1_793_910_668));
        assert_eq!(first.ends_at, Some(1_796_112_000));
    }

    #[test]
    fn an_event_not_started_yet_stays_and_one_that_ended_goes() {
        let before = parse_feed(FIXTURE, 1_793_000_000).unwrap();
        assert_eq!(before.len(), 3);
        assert!(before[0].starts_at.unwrap() > 1_793_000_000);
        // After the second event ended only the others are left.
        let after = parse_feed(FIXTURE, 1_795_600_000).unwrap();
        assert_eq!(after.len(), 2);
        assert!(after.iter().all(|e| e.title != "A Chill Arrives"));
    }

    #[test]
    fn links_and_images_must_be_https_and_the_event_still_shows() {
        let events = parse_feed(FIXTURE, NOW).unwrap();
        let plain = &events[2];
        assert_eq!(plain.title, "Plain links");
        assert_eq!(plain.image_url, None);
        assert_eq!(plain.link, None);
    }

    #[test]
    fn the_default_event_is_used_only_when_nothing_else_is_left() {
        let events = parse_feed(FIXTURE, 1_800_000_000).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].title, "Breach The Vault");
        assert_eq!(
            events[0].description,
            "Hidden in the Clockworks are mysterious Treasure Vaults\ncontaining untold wealth."
        );
        assert_eq!(events[0].starts_at, None);
        assert_eq!(events[0].ends_at, None);
        assert_eq!(
            events[0].link.as_deref(),
            Some("https://example.org/default")
        );
    }

    #[test]
    fn the_first_server_is_used_when_none_is_called_official() {
        let json = r#"{"serverlist":[{"name":"Other","events":[{"content":"Only|one","startsAt":"0","endsAt":"0",}]}]}"#;
        assert_eq!(parse_feed(json, NOW).unwrap()[0].title, "Only");
    }

    #[test]
    fn untitled_or_null_events_are_skipped_and_numbers_are_accepted() {
        let json = r#"{"serverlist":[{"name":"Official","events":[
            {"content":"null"},{"content":"|body"},{"content":"  "},{"content":"Ok","endsAt":1796112000000}]}]}"#;
        let events = parse_feed(json, NOW).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].ends_at, Some(1_796_112_000));
        assert_eq!(events[0].description, "");
    }

    #[test]
    fn text_is_cleaned_and_capped() {
        let long = "word ".repeat(400);
        let json = format!(
            r#"{{"serverlist":[{{"name":"Official","events":[{{"content":"Ti\u0007tle\u001b[31m x|a\r\nb\\nc\u0000d {long}"}}]}}]}}"#
        );
        let e = &parse_feed(&json, NOW).unwrap()[0];
        assert_eq!(e.title, "Ti tle [31m x");
        assert!(!e.description.chars().any(|c| c.is_control() && c != '\n'));
        assert!(e.description.starts_with("a\nb\nc d word"));
        assert!(e.description.chars().count() <= MAX_DESCRIPTION_CHARS);
    }

    #[test]
    fn links_are_length_capped() {
        let json = format!(
            r#"{{"serverlist":[{{"name":"Official","events":[{{"content":"T","link":"https://example.org/{}"}}]}}]}}"#,
            "a".repeat(600)
        );
        assert_eq!(parse_feed(&json, NOW).unwrap()[0].link, None);
    }

    #[test]
    fn not_the_expected_json_is_an_error_and_an_empty_list_is_not() {
        assert!(parse_feed("<html>captive portal</html>", NOW).is_err());
        assert!(parse_feed(r#"{"other":1}"#, NOW).is_err());
        assert_eq!(parse_feed(r#"{"serverlist":[]}"#, NOW).unwrap(), vec![]);
        assert_eq!(
            parse_feed(r#"{"serverlist":[{"name":"Official"}]}"#, NOW).unwrap(),
            vec![]
        );
    }

    #[test]
    fn the_countdown_reads_as_the_mobile_chips_do() {
        let now = 1_000_000;
        assert_eq!(countdown(None, None, now), Countdown::None);
        assert_eq!(countdown(Some(10), None, now), Countdown::None);
        assert_eq!(
            countdown(
                Some(now + 3 * 86_400 + 2 * 3600 + 30),
                Some(now + 9 * 86_400),
                now
            ),
            Countdown::Starts {
                days: 3,
                hours: 2,
                minutes: 0
            }
        );
        assert_eq!(
            countdown(None, Some(now + 2 * 86_400 + 5 * 3600 + 30), now),
            Countdown::Ends {
                days: 2,
                hours: 5,
                minutes: 0
            }
        );
        assert_eq!(
            countdown(None, Some(now + 5 * 3600 + 12 * 60 + 30), now),
            Countdown::EndsToday {
                days: 0,
                hours: 5,
                minutes: 12
            }
        );
        assert_eq!(countdown(Some(5), Some(now), now), Countdown::Ended);
    }

    #[test]
    fn a_good_answer_is_cached_and_served_when_the_request_fails() {
        let dir = tempfile::tempdir().unwrap();
        let cache = dir.path().join("live-events.json");
        let good = load_events(&Fake(Ok(FIXTURE.to_owned())), &cache, NOW);
        assert_eq!(
            (good.events.len(), good.offline, good.fetched_at),
            (3, false, NOW)
        );

        let down = load_events(
            &Fake(Err(EventsError::Network("down".into()))),
            &cache,
            NOW + 60,
        );
        assert_eq!(down.events, good.events);
        assert!(down.offline);
        assert_eq!(down.fetched_at, NOW);

        // A captive portal's page changes nothing; events that ended since are not shown from the cache.
        let portal = load_events(&Fake(Ok("<html>".into())), &cache, 1_795_600_000);
        assert!(portal.offline);
        assert_eq!(portal.events.len(), 2);
    }

    #[test]
    fn no_events_and_no_cache_means_an_empty_card() {
        let dir = tempfile::tempdir().unwrap();
        let cache = dir.path().join("live-events.json");
        let none = load_events(&Fake(Err(EventsError::Network("x".into()))), &cache, NOW);
        assert!(none.events.is_empty());
        let empty = load_events(&Fake(Ok(r#"{"serverlist":[]}"#.into())), &cache, NOW);
        assert!(empty.events.is_empty() && !empty.offline);
        assert!(!cache.exists());
    }

    #[test]
    fn a_damaged_cache_is_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let cache = dir.path().join("live-events.json");
        std::fs::write(&cache, b"{not json").unwrap();
        let feed = load_events(&Fake(Err(EventsError::Network("x".into()))), &cache, NOW);
        assert!(feed.events.is_empty());
    }

    fn png(w: u32, h: u32) -> Vec<u8> {
        let mut b = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
        b.extend(w.to_be_bytes());
        b.extend(h.to_be_bytes());
        b.extend([8, 6, 0, 0, 0]);
        b
    }

    fn jpeg(w: u16, h: u16) -> Vec<u8> {
        let mut b = vec![0xFF, 0xD8, 0xFF, 0xE0, 0, 4, 0, 0, 0xFF, 0xC0, 0, 11, 8];
        b.extend(h.to_be_bytes());
        b.extend(w.to_be_bytes());
        b.extend([1, 1, 0x11, 0]);
        b
    }

    #[test]
    fn only_small_png_and_jpeg_pictures_are_accepted() {
        assert_eq!(
            image_header(&png(640, 200)),
            Some((ImageKind::Png, 640, 200))
        );
        assert_eq!(
            image_header(&jpeg(800, 300)),
            Some((ImageKind::Jpeg, 800, 300))
        );
        assert!(is_acceptable_image(&png(640, 200)));
        assert!(is_acceptable_image(&jpeg(800, 300)));
        assert!(!is_acceptable_image(&png(5000, 10)));
        assert!(!is_acceptable_image(&png(4000, 4000)));
        assert!(!is_acceptable_image(&png(0, 10)));
        assert!(!is_acceptable_image(b"GIF89a\x01\0\x01\0"));
        assert!(!is_acceptable_image(
            b"<svg xmlns='http://www.w3.org/2000/svg'/>"
        ));
        assert!(!is_acceptable_image(b""));
        let mut big = png(10, 10);
        big.resize(MAX_IMAGE_BYTES as usize + 1, 0);
        assert!(!is_acceptable_image(&big));
        assert!(!is_acceptable_image(&[0xFF, 0xD8, 0xFF]));
    }

    #[test]
    fn a_banner_is_downloaded_once_then_served_from_disk() {
        let dir = tempfile::tempdir().unwrap();
        let url = "https://cdn.example.org/a.png";
        let calls = std::cell::Cell::new(0);
        let fetch = |_: &str| {
            calls.set(calls.get() + 1);
            Some(png(10, 10))
        };
        let path = cached_image(dir.path(), url, &fetch).unwrap();
        assert!(path.starts_with(dir.path()) && path.extension().unwrap() == "png");
        // A file an earlier version wrote (`.img`) is still found, and pruned with the others.
        let old = dir.path().join(format!(
            "{}.img",
            &sha256_hex("https://cdn.example.org/old.png".as_bytes())[..32]
        ));
        std::fs::write(&old, png(10, 10)).unwrap();
        assert_eq!(
            cached_image(dir.path(), "https://cdn.example.org/old.png", &|_| None).unwrap(),
            old
        );
        prune_images(dir.path(), std::slice::from_ref(&path));
        assert!(!old.exists() && path.exists());
        assert_eq!(cached_image(dir.path(), url, &fetch).unwrap(), path);
        assert_eq!(calls.get(), 1);

        // A download that is not a picture leaves nothing behind.
        assert!(
            cached_image(dir.path(), "https://cdn.example.org/b.png", &|_| Some(
                b"nope".to_vec()
            ))
            .is_none()
        );
        assert!(cached_image(dir.path(), "https://cdn.example.org/c.png", &|_| None).is_none());
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);

        prune_images(dir.path(), &[]);
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }
}
