//! `HomeFeed` (QML singleton `HomeFeed`): Spiral Knights news and the number of players on Steam, for the Home page.
//! Requests run on worker threads through `skradle_core::steam_api` and post their result back to the UI thread; a
//! failed request shows the last good news from disk (marked offline) and never an error.

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
        /// Players on Steam now (meaningful when `playersState` is "ok").
        #[qproperty(i32, players, READ, WRITE, NOTIFY)]
        type HomeFeed = super::HomeFeedRust;

        /// Fetches the news (not more often than every two minutes).
        #[qinvokable]
        #[cxx_name = "refreshNews"]
        fn refresh_news(self: Pin<&mut HomeFeed>);

        /// Fetches the player count (not more often than once a minute, however often this is called).
        #[qinvokable]
        #[cxx_name = "refreshPlayers"]
        fn refresh_players(self: Pin<&mut HomeFeed>);

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
use skradle_core::steam_api::{
    self, HttpSteam, MinInterval, NewsFeed, NewsItem, SteamApi, SteamError,
};

use crate::settings;

const NEWS_FIXTURE: &str = include_str!("../../../core/tests/fixtures/steam_news.json");

/// Answers from compiled-in fixtures: the self-test and screenshot runs never use the network.
struct FixtureSteam;

impl SteamApi for FixtureSteam {
    fn news(&self, _: u32, _: u32) -> Result<Vec<NewsItem>, SteamError> {
        steam_api::parse_news(NEWS_FIXTURE)
    }
    fn players(&self) -> Result<u64, SteamError> {
        Ok(229)
    }
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
                let answer = api().players();
                let _ = thread.queue(move |mut model| {
                    model.as_mut().rust_mut().players_busy = false;
                    match answer {
                        Ok(count) => {
                            model
                                .as_mut()
                                .set_players(i32::try_from(count).unwrap_or(i32::MAX));
                            model.set_players_state(QString::from("ok"));
                        }
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
        assert_eq!(fixture.players().unwrap(), 229);
    }
}
