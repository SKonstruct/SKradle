//! Spiral Knights' public Steam data for the Home page: the latest news and the number of players on Steam right now.
//!
//! Both are Steam's public Web API, which works without a key:
//!
//! - `ISteamNews/GetNewsForApp/v2/?appid=99900&count=N&maxlength=M`: `appnews.newsitems[]` with `title`, `url`,
//!   `date` (Unix seconds), `feedlabel` and `contents` (Steam strips the markup when `maxlength` is given and cuts
//!   the text there; without it the text is BBCode with `{STEAM_CLAN_IMAGE}` placeholders). [`clean_contents`]
//!   handles both.
//! - `ISteamUserStats/GetNumberOfCurrentPlayers/v1/?appid=<id>`: `{"response":{"player_count":N,"result":1}}`, for
//!   Live (99900) and Preview (99920). The number counts players on Steam only, not those who play through the
//!   standalone client, so the Home page shows an estimate: each count times [`STANDALONE_FACTOR`], summed.
//!
//! The network sits behind [`SteamApi`] so tests use a fake. [`load_news`] falls back to the last good answer on
//! disk (marked offline) when the request fails. All requests go through [`crate::net`]: HTTPS only, size caps,
//! short timeouts.

use std::path::Path;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::jar::write_atomic;
use crate::net::Client;

/// Steam app ids of the two servers: the live game and the Preview server.
pub const APP_ID_LIVE: u32 = 99900;
pub const APP_ID_PREVIEW: u32 = 99920;
/// The app the news comes from.
pub const APP_ID: u32 = APP_ID_LIVE;
/// Steam's own count is only part of the player base; the rest play through the standalone client. The shown
/// estimate multiplies each server's Steam count by this factor (the owner's rule of thumb, not a measurement).
pub const STANDALONE_FACTOR: f64 = 1.4;
/// Where an article without a usable address is sent.
pub const NEWS_HUB_URL: &str = "https://store.steampowered.com/news/app/99900";
/// Items asked for, and the text length asked of Steam (characters, before cleaning).
pub const NEWS_COUNT: u32 = 3;
pub const NEWS_MAX_LENGTH: u32 = 800;
/// Longest teaser shown, in characters.
pub const TEASER_CHARS: usize = 220;
const MAX_RESPONSE_BYTES: u64 = 1024 * 1024;
const MAX_CACHE_BYTES: u64 = 1024 * 1024;

pub fn news_url(count: u32, max_length: u32) -> String {
    format!(
        "https://api.steampowered.com/ISteamNews/GetNewsForApp/v2/?appid={APP_ID}&count={count}&maxlength={max_length}"
    )
}

pub fn players_url(app_id: u32) -> String {
    format!(
        "https://api.steampowered.com/ISteamUserStats/GetNumberOfCurrentPlayers/v1/?appid={app_id}"
    )
}

/// Steam's count scaled by [`STANDALONE_FACTOR`], rounded to the nearest integer.
pub fn estimate_total(steam: u64) -> u64 {
    // Counts are far below 2^52, so the float is exact enough; the cast saturates on absurd input.
    (steam as f64 * STANDALONE_FACTOR).round() as u64
}

/// The player estimate for both servers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayerEstimate {
    pub live_steam: u64,
    pub live_total: u64,
    pub preview_steam: u64,
    pub preview_total: u64,
    /// `live_total + preview_total` (each server rounded first).
    pub total: u64,
}

impl PlayerEstimate {
    pub fn from_steam(live_steam: u64, preview_steam: u64) -> Self {
        let live_total = estimate_total(live_steam);
        let preview_total = estimate_total(preview_steam);
        Self {
            live_steam,
            live_total,
            preview_steam,
            preview_total,
            total: live_total.saturating_add(preview_total),
        }
    }
}

/// Asks Steam for both servers; an error in either one is an error (half an estimate would mislead).
pub fn players_estimate(api: &dyn SteamApi) -> Result<PlayerEstimate, SteamError> {
    let live = api.players(APP_ID_LIVE)?;
    let preview = api.players(APP_ID_PREVIEW)?;
    Ok(PlayerEstimate::from_steam(live, preview))
}

/// One news item, ready to show.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewsItem {
    pub title: String,
    /// An `https://` address (never anything else: it is opened in the system browser).
    pub url: String,
    /// Unix seconds.
    pub date: i64,
    pub label: String,
    /// A short plain-text teaser (may be empty).
    pub teaser: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SteamError {
    /// The request failed (offline, timeout, bad status, too large).
    Network(String),
    /// The answer was not the expected JSON.
    Format(String),
}

impl std::fmt::Display for SteamError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SteamError::Network(why) => write!(f, "could not reach Steam: {why}"),
            SteamError::Format(why) => write!(f, "unexpected answer from Steam: {why}"),
        }
    }
}

impl std::error::Error for SteamError {}

/// The two requests, so tests (and the screenshot driver) can answer without a network.
pub trait SteamApi: Send + Sync {
    fn news(&self, count: u32, max_length: u32) -> Result<Vec<NewsItem>, SteamError>;
    /// Players on Steam now for one app; `Err(Format)` when Steam answered without a count.
    fn players(&self, app_id: u32) -> Result<u64, SteamError>;
}

/// The real Steam Web API.
pub struct HttpSteam {
    client: Client,
}

impl Default for HttpSteam {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpSteam {
    pub fn new() -> Self {
        Self {
            client: Client::with_timeouts(Duration::from_secs(4), Duration::from_secs(8)),
        }
    }
}

impl SteamApi for HttpSteam {
    fn news(&self, count: u32, max_length: u32) -> Result<Vec<NewsItem>, SteamError> {
        let text = self
            .client
            .get_text(&news_url(count, max_length), MAX_RESPONSE_BYTES)
            .map_err(|e| SteamError::Network(e.to_string()))?;
        parse_news(&text)
    }

    fn players(&self, app_id: u32) -> Result<u64, SteamError> {
        let text = self
            .client
            .get_text(&players_url(app_id), 64 * 1024)
            .map_err(|e| SteamError::Network(e.to_string()))?;
        parse_players(&text)
    }
}

#[derive(Deserialize)]
struct NewsEnvelope {
    appnews: NewsApp,
}

#[derive(Deserialize)]
struct NewsApp {
    #[serde(default)]
    newsitems: Vec<RawItem>,
}

#[derive(Deserialize)]
struct RawItem {
    #[serde(default)]
    title: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    date: i64,
    #[serde(default)]
    feedlabel: String,
    #[serde(default)]
    contents: String,
}

/// Parses the news answer into cleaned items. Items without a title are dropped.
pub fn parse_news(json: &str) -> Result<Vec<NewsItem>, SteamError> {
    let envelope: NewsEnvelope =
        serde_json::from_str(json).map_err(|e| SteamError::Format(e.to_string()))?;
    Ok(envelope
        .appnews
        .newsitems
        .into_iter()
        .filter_map(|raw| {
            let title = clean_contents(&raw.title, 160);
            if title.is_empty() {
                return None;
            }
            let mut teaser = clean_contents(&raw.contents, usize::MAX);
            // Steam's plain text often begins with the headline again.
            if let Some(rest) = teaser.strip_prefix(&title) {
                teaser = rest.trim_start_matches([' ', ':', '-', '.']).to_owned();
            }
            Some(NewsItem {
                title,
                url: article_url(&raw.url),
                date: raw.date,
                label: clean_contents(&raw.feedlabel, 60),
                teaser: truncate_words(&teaser, TEASER_CHARS),
            })
        })
        .collect())
}

#[derive(Deserialize)]
struct PlayersEnvelope {
    response: PlayersResponse,
}

#[derive(Deserialize)]
struct PlayersResponse {
    player_count: Option<u64>,
    result: Option<i64>,
}

/// Parses the player-count answer. `result` other than 1 (Steam's "success") or a missing count is an error, so
/// "0 players" is never shown for a failure.
pub fn parse_players(json: &str) -> Result<u64, SteamError> {
    let envelope: PlayersEnvelope =
        serde_json::from_str(json).map_err(|e| SteamError::Format(e.to_string()))?;
    match (envelope.response.result, envelope.response.player_count) {
        (Some(1), Some(count)) => Ok(count),
        (result, _) => Err(SteamError::Format(format!(
            "no player count (result {result:?})"
        ))),
    }
}

/// The article address, only when it is a plain `https://` address with a host; otherwise the news hub.
pub fn article_url(url: &str) -> String {
    let url = url.trim();
    if crate::net::is_plain_https_url(url) {
        url.to_owned()
    } else {
        NEWS_HUB_URL.to_owned()
    }
}

fn re(cell: &'static OnceLock<Regex>, pattern: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(pattern).expect("a fixed pattern"))
}

/// Steam BBCode/HTML to a single line of plain text: image, video and URL-less embeds go, `[url=..]text[/url]` keeps
/// its text, block tags become spaces, entities are decoded, `{STEAM_CLAN_IMAGE}`-style placeholders and bare
/// addresses are removed, a tag cut off by the length limit at the end is dropped, and whitespace is collapsed.
/// `max_chars` cuts at a word with `…` (pass `usize::MAX` for no cut).
pub fn clean_contents(raw: &str, max_chars: usize) -> String {
    static EMBEDS: OnceLock<Regex> = OnceLock::new();
    static CUT_EMBED: OnceLock<Regex> = OnceLock::new();
    static PLACEHOLDER: OnceLock<Regex> = OnceLock::new();
    static BBCODE: OnceLock<Regex> = OnceLock::new();
    static HTML: OnceLock<Regex> = OnceLock::new();
    static CUT_TAG: OnceLock<Regex> = OnceLock::new();
    static BARE_URL: OnceLock<Regex> = OnceLock::new();

    let mut text = re(
        &EMBEDS,
        r"(?is)\[(img|previewyoutube|video|youtube|carousel|spoiler)\b[^\]]*\].*?\[/(img|previewyoutube|video|youtube|carousel|spoiler)\]",
    )
    .replace_all(raw, " ")
    .into_owned();
    // An embed whose closing tag was cut off by the length limit.
    text = re(
        &CUT_EMBED,
        r"(?is)\[(img|previewyoutube|video|youtube|carousel)\b[^\]]*\][^\[]*$",
    )
    .replace_all(&text, " ")
    .into_owned();
    text = re(&PLACEHOLDER, r"\{STEAM_[A-Z_]+\}[^\s\[\]<>]*")
        .replace_all(&text, " ")
        .into_owned();
    // Inline tags vanish (so `rejoice[/b]!` keeps its punctuation); block tags and lists separate words.
    text = re(&BBCODE, r"\[/?([A-Za-z0-9*]+)(?:[= ][^\]]*)?\]")
        .replace_all(&text, |caps: &regex::Captures<'_>| {
            match caps[1].to_ascii_lowercase().as_str() {
                "b" | "i" | "u" | "s" | "strike" | "url" | "code" | "color" | "size"
                | "noparse" => "",
                _ => " ",
            }
        })
        .into_owned();
    text = re(&HTML, r"(?s)</?[A-Za-z][^>]*>")
        .replace_all(&text, " ")
        .into_owned();
    // A tag cut in the middle at the very end: `[b` or `<a hre`.
    text = re(&CUT_TAG, r"(?s)(\[[^\]\s]*|<[^>]*)$")
        .replace_all(&text, " ")
        .into_owned();
    text = decode_entities(&text);
    text = re(&BARE_URL, r"https?://[^\s]+")
        .replace_all(&text, " ")
        .into_owned();
    let joined = text.split_whitespace().collect::<Vec<_>>().join(" ");
    // Steam marks its own cut with dots; ours replaces it.
    let steam_cut = joined.ends_with("...") || joined.ends_with('…');
    let base = if steam_cut {
        joined.trim_end_matches(['.', '…']).trim_end().to_owned()
    } else {
        joined
    };
    let mut out = truncate_words(&base, max_chars);
    if steam_cut && out == base {
        out.push('…');
    }
    out
}

/// Cuts `text` to at most `max` characters at a word boundary and adds `…`; a shorter text is returned as it is.
pub fn truncate_words(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let kept: String = text.chars().take(max.saturating_sub(1)).collect();
    let cut = match kept.rfind(' ') {
        Some(at) if at > kept.len() / 2 => &kept[..at],
        _ => kept.as_str(),
    };
    format!("{}…", cut.trim_end_matches([' ', ',', ';', ':', '-', '.']))
}

fn decode_entities(text: &str) -> String {
    static ENTITY: OnceLock<Regex> = OnceLock::new();
    re(
        &ENTITY,
        r"&(#[0-9]{1,7}|#[xX][0-9a-fA-F]{1,6}|[A-Za-z]{2,8});",
    )
    .replace_all(text, |caps: &regex::Captures<'_>| {
        let name = &caps[1];
        let decoded = match name {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some(' '),
            "hellip" => Some('…'),
            "ndash" => Some('–'),
            "mdash" => Some('—'),
            "rsquo" | "lsquo" => Some('\''),
            "rdquo" | "ldquo" => Some('"'),
            _ => name.strip_prefix('#').and_then(|n| {
                let code = match n.strip_prefix(['x', 'X']) {
                    Some(hex) => u32::from_str_radix(hex, 16).ok(),
                    None => n.parse().ok(),
                };
                code.and_then(char::from_u32)
            }),
        };
        decoded.map_or_else(|| caps[0].to_owned(), String::from)
    })
    .into_owned()
}

/// What the Home page shows for the news.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NewsFeed {
    pub items: Vec<NewsItem>,
    /// The request failed; `items` come from the last good answer (or are empty when there is none).
    pub offline: bool,
    /// When `items` were fetched, Unix seconds (0 when unknown).
    pub fetched_at: i64,
}

#[derive(Serialize, Deserialize)]
struct CacheFile {
    #[serde(rename = "fetchedAt")]
    fetched_at: i64,
    items: Vec<NewsItem>,
}

/// Reads the cached news; `None` when there is no usable cache.
pub fn read_cache(path: &Path) -> Option<(Vec<NewsItem>, i64)> {
    let bytes = crate::bounded::read_regular(path, MAX_CACHE_BYTES).ok()?;
    let cache: CacheFile = serde_json::from_slice(&bytes).ok()?;
    // The file is ours, but it sits in a folder the player can edit: apply the same rule to its addresses.
    let items = cache
        .items
        .into_iter()
        .take(NEWS_COUNT as usize)
        .map(|mut item| {
            item.url = article_url(&item.url);
            item
        })
        .collect();
    Some((items, cache.fetched_at))
}

fn write_cache(path: &Path, items: &[NewsItem], now: i64) {
    let file = CacheFile {
        fetched_at: now,
        items: items.to_vec(),
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

/// Fetches the news. A good answer is cached and returned; a failure returns the cache, marked offline.
/// `now` is Unix seconds.
pub fn load_news(api: &dyn SteamApi, cache: &Path, now: i64) -> NewsFeed {
    match api.news(NEWS_COUNT, NEWS_MAX_LENGTH) {
        Ok(mut items) if !items.is_empty() => {
            items.truncate(NEWS_COUNT as usize);
            write_cache(cache, &items, now);
            NewsFeed {
                items,
                offline: false,
                fetched_at: now,
            }
        }
        // An empty list is a valid answer but not worth replacing the cache with.
        Ok(_) => match read_cache(cache) {
            Some((items, fetched_at)) => NewsFeed {
                items,
                offline: false,
                fetched_at,
            },
            None => NewsFeed::default(),
        },
        Err(_) => {
            let (items, fetched_at) = read_cache(cache).unwrap_or_default();
            NewsFeed {
                items,
                offline: true,
                fetched_at,
            }
        }
    }
}

/// Allows an action at most once per `interval` (the player count: once a minute while the page is visible).
#[derive(Debug, Clone, Copy)]
pub struct MinInterval {
    interval: Duration,
    last: Option<Instant>,
}

impl MinInterval {
    pub fn new(interval: Duration) -> Self {
        Self {
            interval,
            last: None,
        }
    }

    /// True (and the time is recorded) when `interval` has passed since the last time this returned true.
    pub fn ready(&mut self, now: Instant) -> bool {
        match self.last {
            Some(last) if now.saturating_duration_since(last) < self.interval => false,
            _ => {
                self.last = Some(now);
                true
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    const NEWS: &str = include_str!("../tests/fixtures/steam_news.json");
    const PLAYERS: &str = include_str!("../tests/fixtures/steam_players.json");

    /// Answers from memory; counts the calls.
    struct Fake {
        news: Mutex<Result<Vec<NewsItem>, SteamError>>,
        players: Result<u64, SteamError>,
        calls: Mutex<u32>,
    }

    impl Fake {
        fn new(news: Result<Vec<NewsItem>, SteamError>) -> Self {
            Self {
                news: Mutex::new(news),
                players: Ok(7),
                calls: Mutex::new(0),
            }
        }
    }

    impl SteamApi for Fake {
        fn news(&self, _: u32, _: u32) -> Result<Vec<NewsItem>, SteamError> {
            *self.calls.lock().unwrap() += 1;
            self.news.lock().unwrap().clone()
        }
        fn players(&self, _: u32) -> Result<u64, SteamError> {
            self.players.clone()
        }
    }

    fn item(title: &str) -> NewsItem {
        NewsItem {
            title: title.into(),
            url: "https://example.org/a".into(),
            date: 1_700_000_000,
            label: "Community Announcements".into(),
            teaser: "Hello".into(),
        }
    }

    #[test]
    fn the_urls_are_the_documented_ones() {
        assert_eq!(
            news_url(3, 800),
            "https://api.steampowered.com/ISteamNews/GetNewsForApp/v2/?appid=99900&count=3&maxlength=800"
        );
        assert_eq!(
            players_url(APP_ID_LIVE),
            "https://api.steampowered.com/ISteamUserStats/GetNumberOfCurrentPlayers/v1/?appid=99900"
        );
        assert_eq!(
            players_url(APP_ID_PREVIEW),
            "https://api.steampowered.com/ISteamUserStats/GetNumberOfCurrentPlayers/v1/?appid=99920"
        );
    }

    #[test]
    fn parses_a_real_news_answer() {
        let items = parse_news(NEWS).unwrap();
        assert_eq!(items.len(), 3);
        assert_eq!(
            items[0].title,
            "Spiked Beasts Rampage Through the Clockworks"
        );
        assert_eq!(items[0].date, 1_794_001_921);
        assert_eq!(items[0].label, "Community Announcements");
        assert!(items[0]
            .url
            .starts_with("https://steamstore-a.akamaihd.net/news/"));
        assert!(
            items[0].teaser.starts_with("A Beastly Spike Storm"),
            "{}",
            items[0].teaser
        );
        assert!(items[0].teaser.ends_with('…'), "{}", items[0].teaser);
        for i in &items {
            assert!(i.teaser.chars().count() <= TEASER_CHARS, "{}", i.teaser);
            assert!(!i.teaser.contains("..."), "{}", i.teaser);
            assert!(!i.teaser.contains('['), "{}", i.teaser);
        }
    }

    #[test]
    fn a_missing_news_list_is_empty_and_junk_is_a_format_error() {
        assert_eq!(
            parse_news(r#"{"appnews":{"appid":99900}}"#).unwrap(),
            vec![]
        );
        assert!(matches!(parse_news("<html>"), Err(SteamError::Format(_))));
        assert!(matches!(parse_news("{}"), Err(SteamError::Format(_))));
    }

    #[test]
    fn items_without_a_title_are_dropped_and_odd_addresses_go_to_the_news_hub() {
        let json = r#"{"appnews":{"newsitems":[
            {"title":"","url":"https://a.example/x","date":1,"contents":"x"},
            {"title":"File","url":"file:///etc/passwd","date":2,"contents":"x"},
            {"title":"Steam","url":"steam://run/99900","date":3,"contents":"x"},
            {"title":"Http","url":"http://a.example/x","date":4,"contents":"x"},
            {"title":"Ok","url":"https://a.example/x?y=1","date":5,"contents":"x"}]}}"#;
        let items = parse_news(json).unwrap();
        assert_eq!(items.len(), 4);
        assert_eq!(items[0].url, NEWS_HUB_URL);
        assert_eq!(items[1].url, NEWS_HUB_URL);
        assert_eq!(items[2].url, NEWS_HUB_URL);
        assert_eq!(items[3].url, "https://a.example/x?y=1");
        assert_eq!(article_url("https://user@evil.example/x"), NEWS_HUB_URL);
        assert_eq!(article_url("https:///x"), NEWS_HUB_URL);
        assert_eq!(article_url("https://a.example/a b"), NEWS_HUB_URL);
    }

    #[test]
    fn parses_the_player_count_and_never_turns_a_failure_into_zero() {
        assert_eq!(parse_players(PLAYERS).unwrap(), 229);
        assert_eq!(
            parse_players(r#"{"response":{"player_count":0,"result":1}}"#).unwrap(),
            0
        );
        for bad in [
            r#"{"response":{"result":42}}"#,
            r#"{"response":{"player_count":5,"result":42}}"#,
            r#"{"response":{"player_count":5}}"#,
            r#"{"response":{}}"#,
            "{}",
            "nope",
        ] {
            assert!(parse_players(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn bbcode_html_images_and_placeholders_become_a_plain_teaser() {
        let raw = "[h2]Big News[/h2][p][img src=\"{STEAM_CLAN_IMAGE}/1/abc.jpg\"][/img][/p][p]Knights, \
                   [b]rejoice[/b]! Read [url=https://example.org/x]the notes[/url] &amp; see \
                   https://example.org/more now.<br>New &quot;line&quot; &#8211; ok[/p][list][*]one[*]two[/list]\
                   [previewyoutube=abc;full][/previewyoutube]";
        assert_eq!(
            clean_contents(raw, usize::MAX),
            "Big News Knights, rejoice! Read the notes & see now. New \"line\" – ok one two"
        );
    }

    #[test]
    fn a_tag_cut_off_by_the_length_limit_leaves_no_fragment() {
        for (raw, want) in [
            ("Hello [b]world[/b] and [im", "Hello world and"),
            ("Hello <a href=\"https://exa", "Hello"),
            (
                "Hello [img src=\"{STEAM_CLAN_IMAGE}/1/a.jpg\"]{STEAM_CLAN_IMAGE}/1/a",
                "Hello",
            ),
            ("Hello [url=https://x.example/very/long", "Hello"),
            ("Hello {STEAM_CLAN_IMAGE}/7/abc.png world", "Hello world"),
        ] {
            assert_eq!(clean_contents(raw, usize::MAX), want, "{raw}");
        }
    }

    #[test]
    fn truncation_is_by_characters_at_a_word_and_never_splits_a_character() {
        let text = "ä".repeat(30) + " " + &"ö".repeat(30);
        let cut = truncate_words(&text, 40);
        assert!(cut.ends_with('…'));
        assert!(cut.chars().count() <= 40);
        assert_eq!(truncate_words("short", 40), "short");
        let emoji = "😀".repeat(100);
        assert_eq!(truncate_words(&emoji, 10).chars().count(), 10);
        // Steam's own "..." is replaced by one ellipsis.
        assert_eq!(
            clean_contents("Some text that Steam cut...", 100),
            "Some text that Steam cut…"
        );
        assert_eq!(
            clean_contents("Complete sentence.", 100),
            "Complete sentence."
        );
    }

    #[test]
    fn news_is_cached_and_the_cache_is_served_offline() {
        let dir = tempfile::tempdir().unwrap();
        let cache = dir.path().join("sub/steam-news.json");
        let online = Fake::new(Ok(vec![item("One"), item("Two")]));
        let feed = load_news(&online, &cache, 1000);
        assert_eq!(
            (feed.items.len(), feed.offline, feed.fetched_at),
            (2, false, 1000)
        );

        let offline = Fake::new(Err(SteamError::Network("down".into())));
        let feed = load_news(&offline, &cache, 2000);
        assert_eq!(feed.items.len(), 2);
        assert!(feed.offline);
        assert_eq!(
            feed.fetched_at, 1000,
            "shows when the cached items are from"
        );
        assert_eq!(feed.items[0], item("One"));
    }

    #[test]
    fn offline_without_a_cache_is_empty_and_an_empty_answer_keeps_the_cache() {
        let dir = tempfile::tempdir().unwrap();
        let cache = dir.path().join("steam-news.json");
        let offline = Fake::new(Err(SteamError::Network("down".into())));
        let feed = load_news(&offline, &cache, 5);
        assert_eq!(
            feed,
            NewsFeed {
                items: vec![],
                offline: true,
                fetched_at: 0
            }
        );

        load_news(&Fake::new(Ok(vec![item("Kept")])), &cache, 10);
        let feed = load_news(&Fake::new(Ok(vec![])), &cache, 20);
        assert_eq!(feed.items, vec![item("Kept")]);
        assert!(!feed.offline);
    }

    #[test]
    fn a_damaged_or_tampered_cache_is_ignored_or_sanitised() {
        let dir = tempfile::tempdir().unwrap();
        let cache = dir.path().join("steam-news.json");
        std::fs::write(&cache, "not json").unwrap();
        assert!(read_cache(&cache).is_none());
        std::fs::write(
            &cache,
            r#"{"fetchedAt":3,"items":[{"title":"T","url":"file:///etc/passwd","date":1,"label":"","teaser":""}]}"#,
        )
        .unwrap();
        assert_eq!(read_cache(&cache).unwrap().0[0].url, NEWS_HUB_URL);
    }

    #[test]
    fn the_player_count_is_asked_at_most_once_per_interval() {
        let mut gate = MinInterval::new(Duration::from_secs(60));
        let start = Instant::now();
        assert!(gate.ready(start));
        assert!(!gate.ready(start + Duration::from_secs(10)));
        assert!(!gate.ready(start + Duration::from_secs(59)));
        assert!(gate.ready(start + Duration::from_secs(60)));
        assert!(!gate.ready(start + Duration::from_secs(61)));
    }

    #[test]
    fn the_fake_serves_both_shapes() {
        let fake = Fake::new(Ok(parse_news(NEWS).unwrap()));
        assert_eq!(fake.news(3, 400).unwrap().len(), 3);
        assert_eq!(fake.players(APP_ID_LIVE).unwrap(), 7);
        assert_eq!(*fake.calls.lock().unwrap(), 1);
    }

    #[test]
    fn the_factor_and_the_ids_are_the_agreed_ones() {
        assert_eq!((APP_ID_LIVE, APP_ID_PREVIEW), (99900, 99920));
        assert!((STANDALONE_FACTOR - 1.4).abs() < f64::EPSILON);
        assert_eq!(NEWS_COUNT, 3);
    }

    #[test]
    fn each_server_is_scaled_and_rounded_to_the_nearest_whole_player() {
        assert_eq!(estimate_total(0), 0);
        assert_eq!(estimate_total(1), 1); // 1.4
        assert_eq!(estimate_total(2), 3); // 2.8
        assert_eq!(estimate_total(10), 14);
        assert_eq!(estimate_total(229), 321); // 320.6
        assert_eq!(estimate_total(u64::MAX), u64::MAX, "saturates, never wraps");
    }

    #[test]
    fn the_total_adds_the_rounded_servers_not_the_rounded_sum() {
        // 1 -> 1 and 1 -> 1 gives 2, while rounding 2 * 1.4 = 2.8 would give 3.
        let e = PlayerEstimate::from_steam(1, 1);
        assert_eq!((e.live_total, e.preview_total, e.total), (1, 1, 2));
        let e = PlayerEstimate::from_steam(229, 12);
        assert_eq!(
            (e.live_steam, e.live_total, e.preview_steam, e.preview_total),
            (229, 321, 12, 17)
        );
        assert_eq!(e.total, 338);
    }

    #[test]
    fn both_servers_are_asked_and_a_failure_in_either_is_an_error() {
        struct Two(Result<u64, SteamError>, Result<u64, SteamError>);
        impl SteamApi for Two {
            fn news(&self, _: u32, _: u32) -> Result<Vec<NewsItem>, SteamError> {
                Ok(vec![])
            }
            fn players(&self, app_id: u32) -> Result<u64, SteamError> {
                if app_id == APP_ID_LIVE {
                    self.0.clone()
                } else {
                    assert_eq!(app_id, APP_ID_PREVIEW);
                    self.1.clone()
                }
            }
        }
        let ok = players_estimate(&Two(Ok(100), Ok(10))).unwrap();
        assert_eq!((ok.live_total, ok.preview_total, ok.total), (140, 14, 154));
        let down = || Err(SteamError::Network("down".into()));
        assert!(players_estimate(&Two(down(), Ok(1))).is_err());
        assert!(players_estimate(&Two(Ok(1), down())).is_err());
    }

    #[test]
    fn at_most_three_news_items_are_kept_and_cached() {
        let dir = tempfile::tempdir().unwrap();
        let cache = dir.path().join("steam-news.json");
        let many: Vec<_> = (0..6).map(|i| item(&format!("n{i}"))).collect();
        let feed = load_news(&Fake::new(Ok(many.clone())), &cache, 1);
        assert_eq!(feed.items.len(), 3);
        // An older cache with six items shows three.
        let old = CacheFile {
            fetched_at: 1,
            items: many,
        };
        std::fs::write(&cache, serde_json::to_vec(&old).unwrap()).unwrap();
        assert_eq!(read_cache(&cache).unwrap().0.len(), 3);
    }
}
