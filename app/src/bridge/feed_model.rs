//! `HomeFeed` (QML singleton `HomeFeed`): Spiral Knights news, the estimated number of players and the live events,
//! for the Home page. Requests run on worker threads (`skradle_core::steam_api`, `skradle_core::live_events`) and post
//! their result back to the UI thread; a failed request shows the last good answer from disk (marked offline) and
//! never an error.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        /// The news as a JSON array of `{title, url, date, label, teaser}` (`date` in Unix seconds); "[]" when none.
        #[qproperty(QString, news_json, cxx_name = "newsJson", READ, WRITE, NOTIFY)]
        /// "loading" until the first answer, then "ok", "offline" (the items are from the last good answer, maybe
        /// none) or "unavailable" (offline and nothing cached).
        #[qproperty(QString, news_state, cxx_name = "newsState", READ, WRITE, NOTIFY)]
        /// When the shown news were fetched, Unix seconds; 0 when unknown.
        #[qproperty(f64, news_fetched_at, cxx_name = "newsFetchedAt", READ, WRITE, NOTIFY)]
        /// "unknown" before the first answer, then "ok" or "unavailable".
        #[qproperty(QString, players_state, cxx_name = "playersState", READ, WRITE, NOTIFY)]
        /// The estimated players online, both servers (meaningful when `playersState` is "ok").
        #[qproperty(i32, players, READ, WRITE, NOTIFY)]
        /// The breakdown as JSON `{liveSteam, liveTotal, previewSteam, previewTotal, total, factor}`; "{}" before the
        /// first answer.
        #[qproperty(QString, players_json, cxx_name = "playersJson", READ, WRITE, NOTIFY)]
        /// The live events as a JSON array of `{title, description, image, link, startsAt, endsAt}` (Unix seconds, 0
        /// when none; `image` a file URL of the cached banner and `link` an https address, "" when none); "[]" when
        /// there are none.
        #[qproperty(QString, events_json, cxx_name = "eventsJson", READ, WRITE, NOTIFY)]
        /// "unknown" before the first answer, then "ok", "offline" (the events are from the last good answer) or "none"
        /// (no events: the card is hidden).
        #[qproperty(QString, events_state, cxx_name = "eventsState", READ, WRITE, NOTIFY)]
        /// When the shown events were fetched, Unix seconds; 0 when unknown.
        #[qproperty(
            f64,
            events_fetched_at,
            cxx_name = "eventsFetchedAt",
            READ,
            WRITE,
            NOTIFY
        )]
        type HomeFeed = super::HomeFeedRust;

        /// Fetches the news (not more often than every two minutes).
        #[qinvokable]
        #[cxx_name = "refreshNews"]
        fn refresh_news(self: Pin<&mut HomeFeed>);

        /// Fetches the player count (not more often than once a minute, however often this is called).
        #[qinvokable]
        #[cxx_name = "refreshPlayers"]
        fn refresh_players(self: Pin<&mut HomeFeed>);

        /// Fetches the events (not more often than every five minutes).
        #[qinvokable]
        #[cxx_name = "refreshEvents"]
        fn refresh_events(self: Pin<&mut HomeFeed>);

        /// The countdown line of an event as JSON (`{"kind":"ends","days":2,"hours":5,"minutes":0}`, kinds `starts`,
        /// `ends`, `endsToday`, `ended`), for the clock of this moment. `starts_at` 0 means none.
        #[qinvokable]
        fn countdown(self: &HomeFeed, starts_at: f64, ends_at: f64) -> QString;

        /// Screenshots only: "present", "none" or "offline" events.
        #[qinvokable]
        #[cxx_name = "forceEvents"]
        fn force_events(self: Pin<&mut HomeFeed>, state: &QString);

        /// Screenshots only: "ok", "unavailable" or "unknown" player count.
        #[qinvokable]
        #[cxx_name = "forcePlayers"]
        fn force_players(self: Pin<&mut HomeFeed>, state: &QString);

        /// Screenshots only: "offline" shows the cached-news note, "empty" the nothing-cached state.
        #[qinvokable]
        fn force(self: Pin<&mut HomeFeed>, state: &QString);
    }

    impl cxx_qt::Threading for HomeFeed {}
}

use core::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use cxx_qt::{CxxQtThread, CxxQtType, Threading};
use cxx_qt_lib::QString;
use skradle_core::live_events::{self, Event, EventsApi, EventsError, EventsFeed, HttpEvents};
use skradle_core::steam_api::{
    self, HttpSteam, MinInterval, NewsFeed, NewsItem, PlayerEstimate, SteamApi, SteamError,
};

use crate::settings;

const NEWS_FIXTURE: &str = include_str!("../../../core/tests/fixtures/steam_news.json");

/// Answers from compiled-in fixtures: the self-test and screenshot runs never use the network.
struct FixtureSteam;

impl SteamApi for FixtureSteam {
    fn news(&self, _: u32, _: u32) -> Result<Vec<NewsItem>, SteamError> {
        steam_api::parse_news(NEWS_FIXTURE)
    }
    fn players(&self, app_id: u32) -> Result<u64, SteamError> {
        Ok(if app_id == steam_api::APP_ID_PREVIEW {
            12
        } else {
            229
        })
    }
}

/// A small real picture for the fixture events' banner.
const FIXTURE_BANNER: &[u8] = include_bytes!("../../../core/tests/fixtures/event_banner.png");
const FIXTURE_BANNER_URL: &str = "https://example.org/banner.png";

/// Fixture events for the self-test and screenshot runs, relative to now so that their countdowns read the same
/// every time ("Ends in 2d 5h", "Starts in 3d 2h", "Ends today").
fn fixture_events(now: i64) -> Vec<Event> {
    vec![
        Event {
            title: "Double heat day".into(),
            description: "Heat rewards are doubled until the end of the day.".into(),
            image_url: None,
            link: None,
            starts_at: None,
            ends_at: Some(now + 5 * 3600 + 12 * 60 + 30),
        },
        Event {
            title: "Winter Wonderland".into(),
            description:
                "Snow falls in the Clockworks.\nCollect snowflakes, trade them for festive gear and \
                          meet the other knights at the Haven tree."
                    .into(),
            image_url: Some(FIXTURE_BANNER_URL.into()),
            link: Some("https://example.org/events/winter".into()),
            starts_at: None,
            ends_at: Some(now + 2 * 86_400 + 5 * 3600 + 30),
        },
        Event {
            title: "Bomber Weekend".into(),
            description: "Bomb-themed missions with extra rewards.".into(),
            image_url: None,
            link: Some("https://example.org/events/bomber".into()),
            starts_at: Some(now + 3 * 86_400 + 2 * 3600 + 30),
            ends_at: Some(now + 5 * 86_400 + 30),
        },
    ]
}

/// Answers from compiled-in fixtures, like [`FixtureSteam`]: the raw feed text, as the service would send it.
struct FixtureEvents;

impl EventsApi for FixtureEvents {
    fn fetch(&self) -> Result<String, EventsError> {
        let ms = |secs: i64| (secs * 1000).to_string();
        let events: Vec<String> = fixture_events(unix_now())
            .iter()
            .map(|e| {
                // Written the way the service writes it, with the trailing comma.
                format!(
                    r#"{{"banner":"{}|0.0","content":"{}|{}","link":"{}","startsAt":"{}","endsAt":"{}",}}"#,
                    e.image_url.clone().unwrap_or_default(),
                    e.title,
                    e.description.replace('\n', "\\n"),
                    e.link.clone().unwrap_or_default(),
                    ms(e.starts_at.unwrap_or(0)),
                    ms(e.ends_at.unwrap_or(0)),
                )
            })
            .collect();
        Ok(format!(
            r#"{{"serverlist":[{{"name":"Official","events":[{}]}}]}}"#,
            events.join(",")
        ))
    }
}

fn events_api() -> Arc<dyn EventsApi> {
    if crate::options::get().is_test_run() {
        Arc::new(FixtureEvents)
    } else {
        Arc::new(HttpEvents)
    }
}

/// The banner of the fixtures, or the real download.
fn download_banner(url: &str) -> Option<Vec<u8>> {
    if crate::options::get().is_test_run() {
        (url == FIXTURE_BANNER_URL).then(|| FIXTURE_BANNER.to_vec())
    } else {
        live_events::download_image(url)
    }
}

fn events_cache_path() -> std::path::PathBuf {
    settings::config_dir().join("live-events.json")
}

fn events_image_dir() -> std::path::PathBuf {
    settings::config_dir().join("event-images")
}

/// The file URL of each event's cached banner (downloaded now when needed), "" when it has none. Banners no event
/// refers to any more are removed, unless the feed is offline (then the list may be a stale one).
fn banner_urls(feed: &EventsFeed, dir: &std::path::Path) -> Vec<String> {
    let paths: Vec<Option<std::path::PathBuf>> = feed
        .events
        .iter()
        .map(|e| {
            e.image_url
                .as_deref()
                .and_then(|url| live_events::cached_image(dir, url, &download_banner))
        })
        .collect();
    if !feed.offline {
        let keep: Vec<_> = paths.iter().flatten().cloned().collect();
        live_events::prune_images(dir, &keep);
    }
    paths
        .iter()
        .map(|p| {
            p.as_deref()
                .map(crate::mods_host::file_url)
                .unwrap_or_default()
        })
        .collect()
}

/// The state name for an events feed.
pub fn events_state(feed: &EventsFeed) -> &'static str {
    match (feed.events.is_empty(), feed.offline) {
        (true, _) => "none",
        (false, false) => "ok",
        (false, true) => "offline",
    }
}

/// The events as the JSON the page reads; `images` holds one file URL (or "") per event.
fn events_to_json(events: &[Event], images: &[String]) -> String {
    let list: Vec<serde_json::Value> = events
        .iter()
        .enumerate()
        .map(|(i, e)| {
            serde_json::json!({
                "title": e.title,
                "description": e.description,
                "image": images.get(i).cloned().unwrap_or_default(),
                "link": e.link.clone().unwrap_or_default(),
                "startsAt": e.starts_at.unwrap_or(0),
                "endsAt": e.ends_at.unwrap_or(0),
            })
        })
        .collect();
    serde_json::to_string(&list).unwrap_or_else(|_| "[]".to_owned())
}

fn players_to_json(e: &PlayerEstimate) -> String {
    serde_json::json!({
        "liveSteam": e.live_steam,
        "liveTotal": e.live_total,
        "previewSteam": e.preview_steam,
        "previewTotal": e.preview_total,
        "total": e.total,
        "factor": steam_api::STANDALONE_FACTOR,
    })
    .to_string()
}

fn api() -> Arc<dyn SteamApi> {
    if crate::options::get().is_test_run() {
        Arc::new(FixtureSteam)
    } else {
        Arc::new(HttpSteam::new())
    }
}

/// In the launcher's config folder (test runs have a throw-away one, so they never touch the player's cache).
fn cache_path() -> std::path::PathBuf {
    settings::config_dir().join("steam-news.json")
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(0))
}

/// The state name for a feed.
pub fn news_state(feed: &NewsFeed) -> &'static str {
    match (feed.offline, feed.items.is_empty()) {
        (false, _) => "ok",
        (true, false) => "offline",
        (true, true) => "unavailable",
    }
}

pub struct HomeFeedRust {
    news_json: QString,
    news_state: QString,
    news_fetched_at: f64,
    players_state: QString,
    players: i32,
    players_json: QString,
    events_json: QString,
    events_state: QString,
    events_fetched_at: f64,
    events_busy: bool,
    events_gate: MinInterval,
    news_busy: bool,
    news_last: Option<Instant>,
    players_busy: bool,
    players_gate: MinInterval,
}

impl Default for HomeFeedRust {
    fn default() -> Self {
        Self {
            news_json: QString::from("[]"),
            news_state: QString::from("loading"),
            news_fetched_at: 0.0,
            players_state: QString::from("unknown"),
            players: 0,
            players_json: QString::from("{}"),
            events_json: QString::from("[]"),
            events_state: QString::from("unknown"),
            events_fetched_at: 0.0,
            events_busy: false,
            events_gate: MinInterval::new(Duration::from_secs(300)),
            news_busy: false,
            news_last: None,
            players_busy: false,
            players_gate: MinInterval::new(Duration::from_secs(60)),
        }
    }
}

impl qobject::HomeFeed {
    fn show_news(mut self: Pin<&mut Self>, feed: &NewsFeed) {
        let json = serde_json::to_string(&feed.items).unwrap_or_else(|_| "[]".to_owned());
        self.as_mut().set_news_json(QString::from(json.as_str()));
        self.as_mut().set_news_fetched_at(feed.fetched_at as f64);
        self.set_news_state(QString::from(news_state(feed)));
    }

    fn show_players(mut self: Pin<&mut Self>, estimate: &PlayerEstimate) {
        let json = players_to_json(estimate);
        self.as_mut()
            .set_players(i32::try_from(estimate.total).unwrap_or(i32::MAX));
        self.as_mut().set_players_json(QString::from(json.as_str()));
        self.set_players_state(QString::from("ok"));
    }

    fn show_events(mut self: Pin<&mut Self>, feed: &EventsFeed, images: &[String]) {
        let json = events_to_json(&feed.events, images);
        self.as_mut().set_events_json(QString::from(json.as_str()));
        self.as_mut().set_events_fetched_at(feed.fetched_at as f64);
        self.set_events_state(QString::from(events_state(feed)));
    }

    pub fn refresh_events(mut self: Pin<&mut Self>) {
        if self.events_busy || !self.as_mut().rust_mut().events_gate.ready(Instant::now()) {
            return;
        }
        self.as_mut().rust_mut().events_busy = true;
        let thread: CxxQtThread<qobject::HomeFeed> = self.qt_thread();
        let spawned = std::thread::Builder::new()
            .name("skradle-events".into())
            .spawn(move || {
                let feed = live_events::load_events(
                    events_api().as_ref(),
                    &events_cache_path(),
                    unix_now(),
                );
                let images = banner_urls(&feed, &events_image_dir());
                let _ = thread.queue(move |mut model| {
                    model.as_mut().rust_mut().events_busy = false;
                    model.show_events(&feed, &images);
                });
            });
        if spawned.is_err() {
            self.as_mut().rust_mut().events_busy = false;
            self.set_events_state(QString::from("none"));
        }
    }

    pub fn countdown(&self, starts_at: f64, ends_at: f64) -> QString {
        let starts = (starts_at > 0.0).then_some(starts_at as i64);
        let ends = (ends_at > 0.0).then_some(ends_at as i64);
        let c = live_events::countdown(starts, ends, unix_now());
        QString::from(serde_json::to_string(&c).unwrap_or_default().as_str())
    }

    pub fn force_events(mut self: Pin<&mut Self>, state: &QString) {
        let now = unix_now();
        let feed = match state.to_string().as_str() {
            "none" => EventsFeed {
                events: vec![],
                offline: false,
                fetched_at: now,
            },
            "offline" => EventsFeed {
                events: fixture_events(now),
                offline: true,
                fetched_at: now - 5 * 3600,
            },
            _ => EventsFeed {
                events: fixture_events(now),
                offline: false,
                fetched_at: now,
            },
        };
        let images = banner_urls(&feed, &events_image_dir());
        self.as_mut().show_events(&feed, &images);
    }

    pub fn force_players(self: Pin<&mut Self>, state: &QString) {
        match state.to_string().as_str() {
            "unavailable" => self.set_players_state(QString::from("unavailable")),
            "unknown" => self.set_players_state(QString::from("unknown")),
            _ => self.show_players(&PlayerEstimate::from_steam(229, 12)),
        }
    }

    pub fn refresh_news(mut self: Pin<&mut Self>) {
        let recent = self
            .news_last
            .is_some_and(|t| t.elapsed() < Duration::from_secs(120));
        if self.news_busy || recent {
            return;
        }
        self.as_mut().rust_mut().news_busy = true;
        self.as_mut().rust_mut().news_last = Some(Instant::now());
        let thread: CxxQtThread<qobject::HomeFeed> = self.qt_thread();
        let spawned = std::thread::Builder::new()
            .name("skradle-news".into())
            .spawn(move || {
                let feed = steam_api::load_news(api().as_ref(), &cache_path(), unix_now());
                let _ = thread.queue(move |mut model| {
                    model.as_mut().rust_mut().news_busy = false;
                    model.show_news(&feed);
                });
            });
        if spawned.is_err() {
            self.as_mut().rust_mut().news_busy = false;
            self.set_news_state(QString::from("unavailable"));
        }
    }

    pub fn refresh_players(mut self: Pin<&mut Self>) {
        if self.players_busy || !self.as_mut().rust_mut().players_gate.ready(Instant::now()) {
            return;
        }
        self.as_mut().rust_mut().players_busy = true;
        let thread: CxxQtThread<qobject::HomeFeed> = self.qt_thread();
        let spawned = std::thread::Builder::new()
            .name("skradle-players".into())
            .spawn(move || {
                let answer = steam_api::players_estimate(api().as_ref());
                let _ = thread.queue(move |mut model| {
                    model.as_mut().rust_mut().players_busy = false;
                    match answer {
                        Ok(estimate) => model.show_players(&estimate),
                        // Keep the last good number visible but say it is not current.
                        Err(_) => model.set_players_state(QString::from("unavailable")),
                    }
                });
            });
        if spawned.is_err() {
            self.as_mut().rust_mut().players_busy = false;
            self.set_players_state(QString::from("unavailable"));
        }
    }

    pub fn force(mut self: Pin<&mut Self>, state: &QString) {
        let feed = match state.to_string().as_str() {
            "offline" => {
                let items = steam_api::parse_news(NEWS_FIXTURE).unwrap_or_default();
                NewsFeed {
                    items,
                    offline: true,
                    fetched_at: unix_now() - 3 * 3600,
                }
            }
            "empty" => NewsFeed {
                items: vec![],
                offline: true,
                fetched_at: 0,
            },
            _ => NewsFeed {
                items: steam_api::parse_news(NEWS_FIXTURE).unwrap_or_default(),
                offline: false,
                fetched_at: unix_now(),
            },
        };
        self.as_mut().show_news(&feed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_news_state_says_where_the_items_came_from() {
        let item = steam_api::parse_news(NEWS_FIXTURE).unwrap();
        let feed = |offline, items: Vec<NewsItem>| NewsFeed {
            items,
            offline,
            fetched_at: 0,
        };
        assert_eq!(news_state(&feed(false, item.clone())), "ok");
        assert_eq!(news_state(&feed(true, item)), "offline");
        assert_eq!(news_state(&feed(true, vec![])), "unavailable");
    }

    #[test]
    fn the_fixture_serves_both_shapes_without_a_network() {
        let fixture = FixtureSteam;
        assert_eq!(fixture.news(6, 800).unwrap().len(), 3);
        assert_eq!(fixture.players(steam_api::APP_ID_LIVE).unwrap(), 229);
        assert_eq!(fixture.players(steam_api::APP_ID_PREVIEW).unwrap(), 12);
        let feed = FixtureEvents.fetch().unwrap();
        assert_eq!(live_events::parse_feed(&feed, unix_now()).unwrap().len(), 3);
    }

    #[test]
    fn the_fixture_feed_parses_back_into_the_fixture_events() {
        let now = unix_now();
        let parsed = live_events::parse_feed(&FixtureEvents.fetch().unwrap(), now).unwrap();
        let expected = fixture_events(now);
        let times = |events: &[Event]| -> Vec<(Option<i64>, Option<i64>)> {
            events.iter().map(|e| (e.starts_at, e.ends_at)).collect()
        };
        // The fixture clock moves between the two calls: compare the text, and the times to the second.
        for (a, b) in parsed.iter().zip(&expected) {
            assert_eq!((&a.title, &a.description), (&b.title, &b.description));
            assert_eq!((&a.image_url, &a.link), (&b.image_url, &b.link));
        }
        for (a, b) in times(&parsed).iter().zip(times(&expected)) {
            assert!((a.1.unwrap() - b.1.unwrap()).abs() <= 2);
        }
    }

    #[test]
    fn the_fixture_events_read_as_planned_and_serialise_for_the_page() {
        use skradle_core::live_events::{countdown, Countdown};
        let now = 1_800_000_000;
        let events = fixture_events(now);
        assert_eq!(
            countdown(events[1].starts_at, events[1].ends_at, now),
            Countdown::Ends {
                days: 2,
                hours: 5,
                minutes: 0
            }
        );
        assert!(matches!(
            countdown(events[2].starts_at, events[2].ends_at, now),
            Countdown::Starts {
                days: 3,
                hours: 2,
                ..
            }
        ));
        assert!(matches!(
            countdown(events[0].starts_at, events[0].ends_at, now),
            Countdown::EndsToday {
                hours: 5,
                minutes: 12,
                ..
            }
        ));
        let images = vec![String::new(), "file:///x.img".to_owned(), String::new()];
        let json: serde_json::Value =
            serde_json::from_str(&events_to_json(&events, &images)).unwrap();
        assert_eq!(json[0]["link"], "");
        assert_eq!(json[0]["startsAt"], 0);
        assert_eq!(json[1]["image"], "file:///x.img");
        assert_eq!(json[2]["link"], "https://example.org/events/bomber");
    }

    #[test]
    fn the_fixture_banner_is_a_picture_the_cache_accepts() {
        assert!(live_events::is_acceptable_image(FIXTURE_BANNER));
    }

    #[test]
    fn the_events_state_hides_an_empty_card_and_marks_cached_ones() {
        let feed = |events, offline| EventsFeed {
            events,
            offline,
            fetched_at: 0,
        };
        let some = fixture_events(0);
        assert_eq!(events_state(&feed(some.clone(), false)), "ok");
        assert_eq!(events_state(&feed(some, true)), "offline");
        assert_eq!(events_state(&feed(vec![], true)), "none");
        assert_eq!(events_state(&feed(vec![], false)), "none");
    }

    #[test]
    fn the_players_breakdown_carries_both_servers_and_the_factor() {
        let json: serde_json::Value =
            serde_json::from_str(&players_to_json(&PlayerEstimate::from_steam(229, 12))).unwrap();
        assert_eq!(json["liveSteam"], 229);
        assert_eq!(json["liveTotal"], 321);
        assert_eq!(json["previewSteam"], 12);
        assert_eq!(json["previewTotal"], 17);
        assert_eq!(json["total"], 338);
        assert_eq!(json["factor"], 1.4);
    }
}
