//! Live events from the launcher's backend, for the Home page.
//!
//! `GET <BACKEND_BASE_URL>/launcher/v1/events` answers (unsigned JSON, HTTPS only)
//! `{"schema":1,"generated_at":"<RFC3339>","events":[{"id","title","description","starts_at"(RFC3339 or null),
//! "ends_at"(RFC3339),"url"(https or null)}]}`. Everything is validated here: bounded sizes, RFC3339 times,
//! https-only links, and an invalid item is skipped without failing the rest. The last good answer is kept on disk
//! and served (marked offline) when a request fails; a 404 means the backend has no events endpoint yet and the
//! card is simply hidden. All requests go through [`crate::net`].
//!
//! [`countdown`] is the pure part of "Ends in 2d 5h": it turns timestamps into numbers; the words are the UI's.

use std::path::Path;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::jar::write_atomic;
use crate::net::{is_plain_https_url, Client, NetError};

/// The launcher's backend (the same host that serves the signed runtime index). One place to change it.
pub const BACKEND_BASE_URL: &str = "https://dank.site";
pub const EVENTS_PATH: &str = "/launcher/v1/events";
/// The only response schema this client understands.
pub const SCHEMA: i64 = 1;

pub const MAX_RESPONSE_BYTES: u64 = 256 * 1024;
const MAX_CACHE_BYTES: u64 = 512 * 1024;
/// At most this many events are kept.
pub const MAX_EVENTS: usize = 8;
pub const MAX_ID_CHARS: usize = 64;
pub const MAX_TITLE_CHARS: usize = 120;
pub const MAX_DESCRIPTION_CHARS: usize = 600;
pub const MAX_URL_CHARS: usize = 512;
/// An event that ended longer ago than this is no longer shown.
pub const SHOW_ENDED_FOR_SECS: i64 = 24 * 3600;

pub fn events_url() -> String {
    format!("{BACKEND_BASE_URL}{EVENTS_PATH}")
}

/// One event, validated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    pub id: String,
    pub title: String,
    pub description: String,
    /// Unix seconds; `None` when the event has already started (or the backend did not say).
    pub starts_at: Option<i64>,
    /// Unix seconds.
    pub ends_at: i64,
    /// A plain `https://` address, opened in the system browser; `None` shows no button.
    pub url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventsError {
    /// The backend has no events endpoint (yet): not an error to show.
    NotFound,
    /// The request failed (offline, timeout, other status, too large).
    Network(String),
    /// The answer was not the expected JSON or schema.
    Format(String),
}

impl std::fmt::Display for EventsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EventsError::NotFound => write!(f, "no events endpoint"),
            EventsError::Network(why) => write!(f, "could not reach the events service: {why}"),
            EventsError::Format(why) => write!(f, "unexpected events answer: {why}"),
        }
    }
}

impl std::error::Error for EventsError {}

/// The request, so tests (and the screenshot driver) answer without a network.
pub trait EventsApi: Send + Sync {
    fn events(&self) -> Result<Vec<Event>, EventsError>;
}

/// The real backend.
pub struct HttpEvents {
    client: Client,
    url: String,
}

impl Default for HttpEvents {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpEvents {
    pub fn new() -> Self {
        Self {
            client: Client::with_timeouts(Duration::from_secs(4), Duration::from_secs(8)),
            url: events_url(),
        }
    }
}

impl EventsApi for HttpEvents {
    fn events(&self) -> Result<Vec<Event>, EventsError> {
        match self.client.get_text(&self.url, MAX_RESPONSE_BYTES) {
            Ok(text) => parse_events(&text),
            Err(NetError::Status(404)) => Err(EventsError::NotFound),
            Err(e) => Err(EventsError::Network(e.to_string())),
        }
    }
}

#[derive(Deserialize)]
struct Envelope {
    schema: i64,
    events: Vec<serde_json::Value>,
}

#[derive(Deserialize)]
struct RawEvent {
    id: String,
    title: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    starts_at: Option<String>,
    ends_at: String,
    #[serde(default)]
    url: Option<String>,
}

/// Parses the answer. A wrong schema or a missing list is an error; an invalid item is skipped; the result is capped
/// at [`MAX_EVENTS`] and sorted by end time, soonest first.
pub fn parse_events(json: &str) -> Result<Vec<Event>, EventsError> {
    let envelope: Envelope =
        serde_json::from_str(json).map_err(|e| EventsError::Format(e.to_string()))?;
    if envelope.schema != SCHEMA {
        return Err(EventsError::Format(format!(
            "schema {} is not supported",
            envelope.schema
        )));
    }
    let mut seen = std::collections::HashSet::new();
    let mut events: Vec<Event> = envelope
        .events
        .into_iter()
        .filter_map(|value| serde_json::from_value::<RawEvent>(value).ok())
        .filter_map(validate)
        .filter(|e| seen.insert(e.id.clone()))
        .collect();
    events.sort_by_key(|e| e.ends_at);
    events.truncate(MAX_EVENTS);
    Ok(events)
}

fn clean(text: &str, max_chars: usize) -> String {
    let text: String = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let joined = text.split_whitespace().collect::<Vec<_>>().join(" ");
    crate::steam_api::truncate_words(&joined, max_chars)
}

fn validate(raw: RawEvent) -> Option<Event> {
    let id = raw.id.trim();
    if id.is_empty()
        || id.chars().count() > MAX_ID_CHARS
        || id.chars().any(|c| c.is_control() || c.is_whitespace())
    {
        return None;
    }
    let title = clean(&raw.title, MAX_TITLE_CHARS);
    if title.is_empty() {
        return None;
    }
    let ends_at = parse_rfc3339(&raw.ends_at)?;
    let starts_at = match raw.starts_at.as_deref().map(str::trim) {
        None | Some("") => None,
        Some(text) => Some(parse_rfc3339(text)?),
    };
    if starts_at.is_some_and(|s| s >= ends_at) {
        return None;
    }
    // A bad link drops the button, not the event.
    let url = raw
        .url
        .as_deref()
        .map(str::trim)
        .filter(|u| u.chars().count() <= MAX_URL_CHARS && is_plain_https_url(u))
        .map(str::to_owned);
    Some(Event {
        id: id.to_owned(),
        title,
        description: clean(
            raw.description.as_deref().unwrap_or(""),
            MAX_DESCRIPTION_CHARS,
        ),
        starts_at,
        ends_at,
        url,
    })
}

/// Parses an RFC 3339 timestamp (`2026-12-01T18:00:00Z`, `...+02:00`, optional fraction) to Unix seconds.
pub fn parse_rfc3339(text: &str) -> Option<i64> {
    let b = text.trim().as_bytes();
    if b.len() < 20 || !b.is_ascii() {
        return None;
    }
    let num = |from: usize, len: usize| -> Option<i64> {
        let digits = b.get(from..from + len)?;
        if !digits.iter().all(u8::is_ascii_digit) {
            return None;
        }
        std::str::from_utf8(digits).ok()?.parse().ok()
    };
    let punct = |at: usize, want: &[u8]| b.get(at).is_some_and(|c| want.contains(c));
    if !(punct(4, b"-")
        && punct(7, b"-")
        && punct(10, b"Tt ")
        && punct(13, b":")
        && punct(16, b":"))
    {
        return None;
    }
    let (year, month, day) = (num(0, 4)?, num(5, 2)?, num(8, 2)?);
    let (hour, minute, second) = (num(11, 2)?, num(14, 2)?, num(17, 2)?);
    if !(1..=12).contains(&month)
        || day < 1
        || day > days_in_month(year, month)
        || hour > 23
        || minute > 59
        || second > 60
    {
        return None;
    }
    let mut at = 19;
    if punct(at, b".") {
        at += 1;
        let start = at;
        while b.get(at).is_some_and(u8::is_ascii_digit) {
            at += 1;
        }
        if at == start {
            return None;
        }
    }
    let offset = match b.get(at)? {
        b'Z' | b'z' if at + 1 == b.len() => 0,
        sign @ (b'+' | b'-') if at + 6 == b.len() && punct(at + 3, b":") => {
            let (oh, om) = (num(at + 1, 2)?, num(at + 4, 2)?);
            if oh > 23 || om > 59 {
                return None;
            }
            let secs = oh * 3600 + om * 60;
            if *sign == b'+' {
                secs
            } else {
                -secs
            }
        }
        _ => return None,
    };
    let days = days_from_civil(year, month, day);
    Some(days * 86_400 + hour * 3600 + minute * 60 + second.min(59) - offset)
}

fn days_in_month(year: i64, month: i64) -> i64 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 => 29,
        _ => 28,
    }
}

/// Days since 1970-01-01 of a proleptic Gregorian date.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// What the countdown line says, as numbers. The words are the UI's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Countdown {
    /// Not started yet: `Starts in ...`.
    Starts { days: i64, hours: i64, minutes: i64 },
    /// A day or more left: `Ends in 2d 5h`.
    Ends { days: i64, hours: i64, minutes: i64 },
    /// Less than a day left: `Ends today, 5h 12m left`.
    EndsToday { days: i64, hours: i64, minutes: i64 },
    /// `ends_at` has passed.
    Ended,
}

fn split(secs: i64) -> (i64, i64, i64) {
    let secs = secs.max(0);
    (secs / 86_400, secs % 86_400 / 3600, secs % 3600 / 60)
}

/// The countdown for an event at `now` (Unix seconds). Whole minutes are shown, rounded down; the UI refreshes each
/// minute.
pub fn countdown(starts_at: Option<i64>, ends_at: i64, now: i64) -> Countdown {
    if let Some(start) = starts_at.filter(|&s| s > now) {
        let (days, hours, minutes) = split(start - now);
        return Countdown::Starts {
            days,
            hours,
            minutes,
        };
    }
    let left = ends_at - now;
    if left <= 0 {
        return Countdown::Ended;
    }
    let (days, hours, minutes) = split(left);
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

/// Events worth showing at `now`: those that ended more than [`SHOW_ENDED_FOR_SECS`] ago are dropped.
pub fn visible(events: Vec<Event>, now: i64) -> Vec<Event> {
    events
        .into_iter()
        .filter(|e| e.ends_at + SHOW_ENDED_FOR_SECS > now)
        .collect()
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
    events: Vec<Event>,
}

/// Reads the cache; every event is validated again, because the file sits in a folder the player can edit.
pub fn read_cache(path: &Path) -> Option<(Vec<Event>, i64)> {
    let bytes = crate::bounded::read_regular(path, MAX_CACHE_BYTES).ok()?;
    let cache: CacheFile = serde_json::from_slice(&bytes).ok()?;
    let events = cache
        .events
        .into_iter()
        .filter_map(|e| {
            validate(RawEvent {
                id: e.id,
                title: e.title,
                description: Some(e.description),
                starts_at: e.starts_at.map(rfc3339),
                ends_at: rfc3339(e.ends_at),
                url: e.url,
            })
        })
        .take(MAX_EVENTS)
        .collect();
    Some((events, cache.fetched_at))
}

/// Unix seconds back to `YYYY-MM-DDTHH:MM:SSZ` (the cache round trip goes through the same validation).
fn rfc3339(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

fn write_cache(path: &Path, events: &[Event], now: i64) {
    let file = CacheFile {
        fetched_at: now,
        events: events.to_vec(),
    };
    if let Ok(bytes) = serde_json::to_vec(&file) {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        // A cache that cannot be written only costs the offline fallback.
        let _ = write_atomic(path, &bytes);
    }
}

/// Fetches the events. A good answer (even an empty one: the events ended) is cached and returned; a 404 clears the
/// cache and returns nothing; any other failure returns the cache, marked offline. `now` is Unix seconds.
pub fn load_events(api: &dyn EventsApi, cache: &Path, now: i64) -> EventsFeed {
    match api.events() {
        Ok(events) => {
            write_cache(cache, &events, now);
            EventsFeed {
                events: visible(events, now),
                offline: false,
                fetched_at: now,
            }
        }
        Err(EventsError::NotFound) => {
            write_cache(cache, &[], now);
            EventsFeed {
                events: vec![],
                offline: false,
                fetched_at: now,
            }
        }
        Err(_) => {
            let (events, fetched_at) = read_cache(cache).unwrap_or_default();
            EventsFeed {
                events: visible(events, now),
                offline: true,
                fetched_at,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = r#"{"schema":1,"generated_at":"2026-12-01T10:00:00Z","events":[
        {"id":"winter","title":"Winter Event","description":"Snow  and\nloot.","starts_at":null,
         "ends_at":"2026-12-10T12:00:00Z","url":"https://example.org/winter"},
        {"id":"early","title":"Soon","description":"","starts_at":"2026-12-02T00:00:00+02:00",
         "ends_at":"2026-12-03T00:00:00.500Z","url":null}]}"#;

    struct Fake(Result<Vec<Event>, EventsError>);
    impl EventsApi for Fake {
        fn events(&self) -> Result<Vec<Event>, EventsError> {
            self.0.clone()
        }
    }

    fn event(id: &str, ends_at: i64) -> Event {
        Event {
            id: id.into(),
            title: format!("Event {id}"),
            description: "About".into(),
            starts_at: None,
            ends_at,
            url: Some("https://example.org/e".into()),
        }
    }

    #[test]
    fn the_events_address_is_the_backend_path() {
        assert_eq!(
            events_url(),
            format!("{BACKEND_BASE_URL}/launcher/v1/events")
        );
        assert!(BACKEND_BASE_URL.starts_with("https://"));
    }

    #[test]
    fn rfc3339_times_are_exact() {
        assert_eq!(parse_rfc3339("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_rfc3339("2026-12-10T12:00:00Z"), Some(1_796_904_000));
        assert_eq!(
            parse_rfc3339("2026-12-10T14:00:00+02:00"),
            Some(1_796_904_000)
        );
        assert_eq!(
            parse_rfc3339("2026-12-10T07:30:00-04:30"),
            Some(1_796_904_000)
        );
        assert_eq!(
            parse_rfc3339("2026-12-10T12:00:00.987Z"),
            Some(1_796_904_000)
        );
        assert_eq!(parse_rfc3339("2026-12-10t12:00:00z"), Some(1_796_904_000));
        assert_eq!(parse_rfc3339("2024-02-29T00:00:00Z"), Some(1_709_164_800));
        assert_eq!(parse_rfc3339("1969-12-31T23:59:59Z"), Some(-1));
        for bad in [
            "",
            "2026-12-10",
            "2026-12-10T12:00:00",
            "2026-12-10T12:00Z",
            "2026-13-10T12:00:00Z",
            "2026-02-29T12:00:00Z",
            "2026-12-32T12:00:00Z",
            "2026-12-10T24:00:00Z",
            "2026-12-10T12:60:00Z",
            "2026-12-10T12:00:00.Z",
            "2026-12-10T12:00:00+0200",
            "2026-12-10T12:00:00+24:00",
            "2026-12-10T12:00:00Zjunk",
            "2026-12-10T12:00:00é",
        ] {
            assert_eq!(parse_rfc3339(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn the_cache_time_round_trips() {
        for secs in [0, -1, 1_796_904_000, 1_709_164_800, 4_102_444_799] {
            assert_eq!(parse_rfc3339(&rfc3339(secs)), Some(secs), "{secs}");
        }
    }

    #[test]
    fn a_good_answer_is_parsed_cleaned_and_sorted() {
        let events = parse_events(GOOD).unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].id, "early");
        assert_eq!(events[0].starts_at, Some(1_796_162_400));
        assert_eq!(events[0].ends_at, 1_796_256_000);
        assert_eq!(events[0].url, None);
        assert_eq!(events[1].description, "Snow and loot.");
        assert_eq!(events[1].url.as_deref(), Some("https://example.org/winter"));
        assert_eq!(events[1].starts_at, None);
    }

    #[test]
    fn invalid_items_are_skipped_and_bad_links_only_lose_the_button() {
        let json = r#"{"schema":1,"events":[
            {"id":"","title":"No id","ends_at":"2026-12-10T12:00:00Z"},
            {"id":"a","title":"   ","ends_at":"2026-12-10T12:00:00Z"},
            {"id":"b","title":"Bad end","ends_at":"tomorrow"},
            {"id":"c","title":"Bad start","starts_at":"nope","ends_at":"2026-12-10T12:00:00Z"},
            {"id":"d","title":"Backwards","starts_at":"2026-12-11T00:00:00Z","ends_at":"2026-12-10T12:00:00Z"},
            {"id":"e e","title":"Space in id","ends_at":"2026-12-10T12:00:00Z"},
            {"title":"No id field","ends_at":"2026-12-10T12:00:00Z"},
            "junk", 7, null,
            {"id":"f","title":"Http","ends_at":"2026-12-10T12:00:00Z","url":"http://example.org/x"},
            {"id":"g","title":"Script","ends_at":"2026-12-10T12:00:00Z","url":"javascript:alert(1)"},
            {"id":"h","title":"Userinfo","ends_at":"2026-12-10T12:00:00Z","url":"https://a@evil.example/x"},
            {"id":"i","title":"Fine","ends_at":"2026-12-10T12:00:00Z","url":"https://example.org/ok?x=1"},
            {"id":"i","title":"Duplicate id","ends_at":"2026-12-10T12:00:00Z"}]}"#;
        let events = parse_events(json).unwrap();
        let ids: Vec<_> = events.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, ["f", "g", "h", "i"]);
        assert_eq!(events[0].url, None);
        assert_eq!(events[1].url, None);
        assert_eq!(events[2].url, None);
        assert_eq!(events[3].url.as_deref(), Some("https://example.org/ok?x=1"));
        assert_eq!(events[3].title, "Fine", "the first of two equal ids wins");
    }

    #[test]
    fn sizes_are_bounded() {
        let long = "x".repeat(5000);
        let json = format!(
            r#"{{"schema":1,"events":[{{"id":"{}","title":"Long id","ends_at":"2026-12-10T12:00:00Z"}},
            {{"id":"a","title":"{long}","description":"{long}","ends_at":"2026-12-10T12:00:00Z","url":"https://example.org/{long}"}}]}}"#,
            "i".repeat(MAX_ID_CHARS + 1)
        );
        let events = parse_events(&json).unwrap();
        assert_eq!(events.len(), 1, "the over-long id is invalid");
        assert!(events[0].title.chars().count() <= MAX_TITLE_CHARS);
        assert!(events[0].description.chars().count() <= MAX_DESCRIPTION_CHARS);
        assert_eq!(events[0].url, None);

        let many: Vec<String> = (0..30)
            .map(|i| {
                format!(
                    r#"{{"id":"e{i}","title":"T","ends_at":"2026-12-{:02}T12:00:00Z"}}"#,
                    i % 28 + 1
                )
            })
            .collect();
        let json = format!(r#"{{"schema":1,"events":[{}]}}"#, many.join(","));
        assert_eq!(parse_events(&json).unwrap().len(), MAX_EVENTS);
    }

    #[test]
    fn a_wrong_schema_or_shape_is_an_error() {
        for bad in [
            "",
            "nope",
            "{}",
            r#"{"schema":2,"events":[]}"#,
            r#"{"schema":"1","events":[]}"#,
            r#"{"schema":1}"#,
            r#"{"schema":1,"events":{}}"#,
        ] {
            assert!(
                matches!(parse_events(bad), Err(EventsError::Format(_))),
                "{bad:?}"
            );
        }
        assert_eq!(parse_events(r#"{"schema":1,"events":[]}"#).unwrap(), vec![]);
    }

    #[test]
    fn the_countdown_counts_down_and_flips_at_the_edges() {
        let now = 1_000_000;
        let end = |left: i64| countdown(None, now + left, now);
        let ends = |days, hours, minutes| Countdown::Ends {
            days,
            hours,
            minutes,
        };
        let today = |hours, minutes| Countdown::EndsToday {
            days: 0,
            hours,
            minutes,
        };
        assert_eq!(end(2 * 86_400 + 5 * 3600 + 59), ends(2, 5, 0));
        assert_eq!(end(86_400), ends(1, 0, 0));
        assert_eq!(end(86_399), today(23, 59));
        assert_eq!(end(59), today(0, 0));
        assert_eq!(end(1), today(0, 0));
        assert_eq!(end(0), Countdown::Ended);
        assert_eq!(end(-5), Countdown::Ended);
        // Not started: the start wins; a start in the past is ignored.
        assert_eq!(
            countdown(Some(now + 3 * 3600 + 120), now + 99_999, now),
            Countdown::Starts {
                days: 0,
                hours: 3,
                minutes: 2
            }
        );
        assert_eq!(countdown(Some(now - 1), now + 7200, now), today(2, 0));
        assert_eq!(countdown(Some(now), now + 60, now), today(0, 1));
    }

    #[test]
    fn the_countdown_serialises_for_the_ui() {
        let json = serde_json::to_string(&Countdown::Ends {
            days: 2,
            hours: 5,
            minutes: 9,
        })
        .unwrap();
        assert_eq!(json, r#"{"kind":"ends","days":2,"hours":5,"minutes":9}"#);
        assert_eq!(
            serde_json::to_string(&Countdown::Ended).unwrap(),
            r#"{"kind":"ended"}"#
        );
        let json = serde_json::to_string(&Countdown::EndsToday {
            days: 0,
            hours: 1,
            minutes: 2,
        })
        .unwrap();
        assert!(json.starts_with(r#"{"kind":"endsToday""#), "{json}");
    }

    #[test]
    fn events_that_ended_long_ago_are_not_shown() {
        let now = 10_000_000;
        let kept = visible(
            vec![
                event("old", now - SHOW_ENDED_FOR_SECS - 1),
                event("just", now - 60),
                event("live", now + 60),
            ],
            now,
        );
        let ids: Vec<_> = kept.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, ["just", "live"]);
    }

    #[test]
    fn events_are_cached_and_served_offline() {
        let dir = tempfile::tempdir().unwrap();
        let cache = dir.path().join("sub/events.json");
        let now = 1_796_000_000;
        let online = Fake(Ok(vec![event("a", now + 3600), event("b", now + 7200)]));
        let feed = load_events(&online, &cache, now);
        assert_eq!(
            (feed.events.len(), feed.offline, feed.fetched_at),
            (2, false, now)
        );

        let down = Fake(Err(EventsError::Network("down".into())));
        let feed = load_events(&down, &cache, now + 600);
        assert!(feed.offline);
        assert_eq!(feed.fetched_at, now);
        assert_eq!(
            feed.events,
            vec![event("a", now + 3600), event("b", now + 7200)]
        );

        let junk = Fake(Err(EventsError::Format("junk".into())));
        assert!(load_events(&junk, &cache, now).offline);
    }

    #[test]
    fn offline_without_a_cache_is_empty_and_a_404_hides_the_card() {
        let dir = tempfile::tempdir().unwrap();
        let cache = dir.path().join("events.json");
        let now = 1_796_000_000;
        let down = Fake(Err(EventsError::Network("down".into())));
        let feed = load_events(&down, &cache, now);
        assert_eq!(
            feed,
            EventsFeed {
                events: vec![],
                offline: true,
                fetched_at: 0
            }
        );

        let ok = Fake(Ok(vec![event("a", now + 3600)]));
        load_events(&ok, &cache, now);
        let gone = Fake(Err(EventsError::NotFound));
        let feed = load_events(&gone, &cache, now);
        assert!(
            feed.events.is_empty() && !feed.offline,
            "404 is not an offline note"
        );
        // The cache was cleared, so a later outage does not bring the old events back.
        assert!(load_events(&down, &cache, now).events.is_empty());
    }

    #[test]
    fn a_tampered_cache_is_validated_again() {
        let dir = tempfile::tempdir().unwrap();
        let cache = dir.path().join("events.json");
        std::fs::write(&cache, "not json").unwrap();
        assert!(read_cache(&cache).is_none());
        std::fs::write(
            &cache,
            r#"{"fetchedAt":3,"events":[
              {"id":"a","title":"T","description":"","starts_at":null,"ends_at":5,"url":"file:///etc/passwd"},
              {"id":"","title":"No id","description":"","starts_at":null,"ends_at":5,"url":null}]}"#,
        )
        .unwrap();
        let (events, at) = read_cache(&cache).unwrap();
        assert_eq!(at, 3);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].url, None);
    }

    #[test]
    fn the_client_refuses_a_plain_http_address_without_touching_the_network() {
        let api = HttpEvents {
            client: Client::with_timeouts(Duration::from_millis(200), Duration::from_millis(400)),
            url: "http://127.0.0.1:9/launcher/v1/events".into(),
        };
        assert!(matches!(api.events(), Err(EventsError::Network(_))));
    }
}
