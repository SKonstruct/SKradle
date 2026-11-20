//! `ServerBook` (QML singleton): the server list of the Servers page. The rows, the stored selection and the form
//! checks come from `crate::servers`; reachability comes from short TCP probes of each game port, one worker thread
//! per server, results posted back to the UI thread. Probe results are a property of their own, keyed by server id,
//! so a fresh result never rebuilds the list (an open menu or a focused button would be lost).

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
        /// The rows as JSON: Official, the saved servers, the hosted one (see `servers::Entry`).
        #[qproperty(QString, entries, READ, WRITE, NOTIFY)]
        /// `{id: {state: "checking" | "up" | "down", ms: n}}` as JSON.
        #[qproperty(QString, probes, READ, WRITE, NOTIFY)]
        /// The id of the server the next launch uses (`official` for Official).
        #[qproperty(QString, selected_id, cxx_name = "selectedId", READ, WRITE, NOTIFY)]
        #[qproperty(QString, selected_name, cxx_name = "selectedName", READ, WRITE, NOTIFY)]
        /// Empty, or why the last change could not be saved (`write`).
        #[qproperty(QString, error, READ, WRITE, NOTIFY)]
        type ServerBook = super::ServerBookRust;

        /// Reads the stored list again (another part of the app may have changed it).
        #[qinvokable]
        fn reload(self: Pin<&mut ServerBook>);
        /// Uses this server when the game is launched. `official` or an unknown id selects Official.
        #[qinvokable]
        fn select(self: Pin<&mut ServerBook>, id: &QString);
        /// Checks the add/edit form (JSON with `id name address port key getdown content`); the report as JSON.
        #[qinvokable]
        fn check(self: &ServerBook, draft: &QString) -> QString;
        /// Saves the form; `{"ok": true, "id": ...}` or the report as JSON.
        #[qinvokable]
        fn save(self: Pin<&mut ServerBook>, draft: &QString) -> QString;
        /// The form for editing a saved server, as JSON; empty for any other id.
        #[qinvokable]
        #[cxx_name = "draftOf"]
        fn draft_of(self: &ServerBook, id: &QString) -> QString;
        /// Removes a saved server (never Official or the hosted one). True when something was removed.
        #[qinvokable]
        fn remove(self: Pin<&mut ServerBook>, id: &QString) -> bool;
        /// The port in a typed address (`host:47624`), 0 when it has none.
        #[qinvokable]
        #[cxx_name = "portOf"]
        fn port_of(self: &ServerBook, address: &QString) -> i32;
        /// Probes every server's game port now (servers still being probed are skipped).
        #[qinvokable]
        #[cxx_name = "probeAll"]
        fn probe_all(self: Pin<&mut ServerBook>);
        /// Screenshots only: shows a fixed list with fixed probe results; "" goes back to the stored list.
        #[qinvokable]
        fn force(self: Pin<&mut ServerBook>, name: &QString);
    }

    impl cxx_qt::Threading for ServerBook {}
    impl cxx_qt::Initialize for ServerBook {}
}

use core::pin::Pin;
use std::collections::{BTreeMap, HashSet};
use std::time::Duration;

use cxx_qt::{CxxQtThread, CxxQtType, Initialize, Threading};
use cxx_qt_lib::QString;
use serde::Serialize;
use skradle_core::net;

use crate::servers::{self, Book, Draft, Entry};

/// How long a probe waits for the game port. Short: a server that does not answer in this time is shown as
/// unreachable, and nothing else waits for it.
const PROBE_TIMEOUT: Duration = Duration::from_millis(2500);

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct Probe {
    /// `checking`, `up` or `down`.
    state: &'static str,
    /// The connection time of an `up` probe, in milliseconds.
    ms: u32,
}

pub struct ServerBookRust {
    entries: QString,
    probes: QString,
    selected_id: QString,
    selected_name: QString,
    error: QString,
    book: Book,
    rows: Vec<Entry>,
    results: BTreeMap<String, Probe>,
    inflight: HashSet<String>,
    /// A fixed list for screenshots; while set nothing is read from or written to the store, and nothing is probed.
    fixture: bool,
}

impl Default for ServerBookRust {
    fn default() -> Self {
        Self {
            entries: QString::default(),
            probes: QString::from("{}"),
            selected_id: QString::from(servers::OFFICIAL_ID),
            selected_name: QString::default(),
            error: QString::default(),
            book: Book::global(),
            rows: Vec::new(),
            results: BTreeMap::new(),
            inflight: HashSet::new(),
            fixture: false,
        }
    }
}

/// The milliseconds a probe took, at least 1 (a loopback connect can round to zero).
fn millis(d: Duration) -> u32 {
    u32::try_from(d.as_millis()).unwrap_or(u32::MAX).max(1)
}

impl qobject::ServerBook {
    fn show_rows(mut self: Pin<&mut Self>, rows: Vec<Entry>) {
        let json = serde_json::to_string(&rows).unwrap_or_else(|_| "[]".into());
        let selected = rows.iter().find(|r| r.selected);
        let id = selected.map_or(servers::OFFICIAL_ID, |r| r.id.as_str());
        let name = selected.map_or("", |r| r.name.as_str());
        let (id, name) = (QString::from(id), QString::from(name));
        // A probe result for a server that is gone, or moved to another address, is stale.
        let live: HashSet<&str> = rows.iter().map(|r| r.id.as_str()).collect();
        self.as_mut()
            .rust_mut()
            .results
            .retain(|k, _| live.contains(k.as_str()));
        self.as_mut().rust_mut().rows = rows;
        self.as_mut().set_entries(QString::from(json.as_str()));
        self.as_mut().set_selected_id(id);
        self.as_mut().set_selected_name(name);
        self.show_probes();
    }

    fn show_probes(mut self: Pin<&mut Self>) {
        let json = serde_json::to_string(&self.results).unwrap_or_else(|_| "{}".into());
        self.as_mut().set_probes(QString::from(json.as_str()));
    }

    fn reload_rows(self: Pin<&mut Self>) {
        let rows = servers::entries(&self.book.load());
        self.show_rows(rows);
    }

    fn remember_error(mut self: Pin<&mut Self>, failed: bool) {
        self.as_mut()
            .set_error(QString::from(if failed { "write" } else { "" }));
    }

    pub fn reload(self: Pin<&mut Self>) {
        if !self.fixture {
            self.reload_rows();
        }
    }

    pub fn select(mut self: Pin<&mut Self>, id: &QString) {
        if self.fixture {
            return;
        }
        let failed = servers::select(&self.book, &id.to_string()).is_err();
        self.as_mut().remember_error(failed);
        self.reload_rows();
    }

    pub fn check(&self, draft: &QString) -> QString {
        let draft: Draft = serde_json::from_str(&draft.to_string()).unwrap_or_default();
        let (report, _) = servers::check(&draft, &self.book.load());
        QString::from(serde_json::to_string(&report).unwrap_or_default().as_str())
    }

    pub fn save(mut self: Pin<&mut Self>, draft: &QString) -> QString {
        let draft: Draft = serde_json::from_str(&draft.to_string()).unwrap_or_default();
        let json = if self.fixture {
            serde_json::json!({ "ok": false, "errors": { "form": { "code": "write", "detail": "" } }, "changes": [] })
        } else {
            match servers::save(&self.book, &draft) {
                Ok(id) => {
                    // An edited server may have moved: its old probe result says nothing about the new address.
                    self.as_mut().rust_mut().results.remove(&id);
                    self.as_mut().reload_rows();
                    serde_json::json!({ "ok": true, "id": id })
                }
                Err(report) => serde_json::to_value(report).unwrap_or_default(),
            }
        };
        QString::from(json.to_string().as_str())
    }

    pub fn draft_of(&self, id: &QString) -> QString {
        let draft = servers::draft_of(&self.book.load(), &id.to_string());
        QString::from(draft.map(|d| d.to_string()).unwrap_or_default().as_str())
    }

    pub fn remove(mut self: Pin<&mut Self>, id: &QString) -> bool {
        if self.fixture {
            return false;
        }
        let id = id.to_string();
        let outcome = servers::remove(&self.book, &id);
        self.as_mut().remember_error(outcome.is_err());
        self.as_mut().rust_mut().results.remove(&id);
        self.reload_rows();
        outcome.unwrap_or(false)
    }

    pub fn port_of(&self, address: &QString) -> i32 {
        match skradle_core::servers::split_address(&address.to_string()) {
            Ok((_, Some(port))) => i32::from(port),
            _ => 0,
        }
    }

    pub fn probe_all(mut self: Pin<&mut Self>) {
        // Self-tests and screenshots never touch the network.
        if self.fixture || crate::options::get().is_test_run() {
            return;
        }
        let rows = self.rows.clone();
        for row in rows {
            if self.inflight.contains(&row.id) {
                continue;
            }
            self.as_mut().rust_mut().inflight.insert(row.id.clone());
            // Until the first answer there is nothing to show but "checking"; later rounds keep the last result.
            if !self.results.contains_key(&row.id) {
                self.as_mut().rust_mut().results.insert(
                    row.id.clone(),
                    Probe {
                        state: "checking",
                        ms: 0,
                    },
                );
            }
            let thread: CxxQtThread<qobject::ServerBook> = self.qt_thread();
            let (id, host, port) = (row.id.clone(), row.host.clone(), row.port);
            let spawned = std::thread::Builder::new()
                .name("skradle-probe".into())
                .spawn(move || {
                    let took = net::probe_tcp(&host, port, PROBE_TIMEOUT);
                    let _ = thread.queue(move |mut model| {
                        model.as_mut().rust_mut().inflight.remove(&id);
                        // Only a server still in the list at the same address gets the answer.
                        let current = model
                            .rows
                            .iter()
                            .any(|r| r.id == id && r.host == host && r.port == port);
                        if current {
                            let probe = match took {
                                Some(d) => Probe {
                                    state: "up",
                                    ms: millis(d),
                                },
                                None => Probe {
                                    state: "down",
                                    ms: 0,
                                },
                            };
                            model.as_mut().rust_mut().results.insert(id, probe);
                        }
                        model.show_probes();
                    });
                });
            if spawned.is_err() {
                self.as_mut().rust_mut().inflight.remove(&row.id);
                self.as_mut().rust_mut().results.insert(
                    row.id,
                    Probe {
                        state: "down",
                        ms: 0,
                    },
                );
            }
        }
        self.show_probes();
    }

    pub fn force(mut self: Pin<&mut Self>, name: &QString) {
        if name.to_string().is_empty() {
            self.as_mut().rust_mut().fixture = false;
            self.as_mut().rust_mut().results.clear();
            self.reload_rows();
            return;
        }
        let row =
            |id: &str, name: &str, host: &str, port: u16, kind: &'static str, selected: bool| {
                Entry {
                    id: id.into(),
                    name: name.into(),
                    host: host.into(),
                    port,
                    address: servers::address_text(host, port),
                    kind,
                    selected,
                    public_key: String::new(),
                    getdown_url: String::new(),
                    content_url: String::new(),
                }
            };
        let rows = vec![
            row(
                "official",
                "Official",
                "game.spiralknights.com",
                47624,
                "official",
                false,
            ),
            row(
                "a",
                "Haven Community",
                "haven.example.org",
                47624,
                "saved",
                true,
            ),
            row("b", "Friends only", "192.168.1.40", 47700, "saved", false),
            row(
                "c",
                "Old test realm",
                "test.example.org",
                47624,
                "saved",
                false,
            ),
            row(
                "local-server",
                "Local Server",
                "127.0.0.1",
                47624,
                "local",
                false,
            ),
        ];
        let result = |state, ms| Probe { state, ms };
        let results = BTreeMap::from([
            ("official".to_owned(), result("up", 38)),
            ("a".to_owned(), result("up", 124)),
            ("b".to_owned(), result("up", 2)),
            ("c".to_owned(), result("down", 0)),
            ("local-server".to_owned(), result("down", 0)),
        ]);
        self.as_mut().rust_mut().fixture = true;
        self.as_mut().rust_mut().results = results;
        self.show_rows(rows);
    }
}

impl Initialize for qobject::ServerBook {
    fn initialize(self: Pin<&mut Self>) {
        self.reload_rows();
    }
}
