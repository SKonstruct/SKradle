//! The server list as the Servers page and the launch use it: one stored list (`servers.json` in the config
//! folder), the synthesised Official entry, and the checks for the add/edit form.
//!
//! The launch reads the same file (`selected_server_from`), so the server chosen on the page is the server the game
//! connects to, and the choice survives restarts because it is just the stored `selected` id. Every write goes
//! through [`Book::update`], which serialises writers: the page and the hosted server's registration both change this
//! file and must not overwrite each other.
//!
//! Problems are reported as codes, not sentences: the wording lives in QML where it can be translated.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use skradle_core::serverhost::register::LOCAL_SERVER_ID;
use skradle_core::servers::{self, official, Server, ServerList};

use crate::settings;

pub const FILE: &str = "servers.json";
pub const OFFICIAL_ID: &str = "official";
pub const LOCAL_ID: &str = LOCAL_SERVER_ID;

static WRITERS: Mutex<()> = Mutex::new(());

/// The stored list.
pub struct Book {
    path: PathBuf,
}

impl Book {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// The player's list, in the launcher's config folder.
    pub fn global() -> Self {
        Self::new(settings::config_dir().join(FILE))
    }

    pub fn load(&self) -> ServerList {
        ServerList::load(&self.path)
    }

    /// Reads the list, lets `change` edit it and writes it back, all while holding the one writer lock.
    pub fn update<R>(&self, change: impl FnOnce(&mut ServerList) -> R) -> io::Result<R> {
        let _guard = WRITERS.lock().unwrap_or_else(|e| e.into_inner());
        let mut list = self.load();
        let result = change(&mut list);
        list.store(&self.path)?;
        Ok(result)
    }
}

/// The server a launch connects to: the selected one in `config_dir`'s list, `None` (Official) when nothing valid
/// is selected.
pub fn selected_server_from(config_dir: &Path) -> Option<Server> {
    ServerList::load(&config_dir.join(FILE))
        .selected_server()
        .cloned()
}

/// `host:port`, with an IPv6 host in brackets.
pub fn address_text(host: &str, port: u16) -> String {
    if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    }
}

/// One row of the list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Entry {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub address: String,
    /// `official`, `saved` or `local` (the hosted server, managed from its own controls).
    pub kind: &'static str,
    pub selected: bool,
    #[serde(rename = "publicKey")]
    pub public_key: String,
    #[serde(rename = "getdownUrl")]
    pub getdown_url: String,
    #[serde(rename = "contentUrl")]
    pub content_url: String,
}

/// Official first, then the saved servers, then the hosted one. Exactly one entry is selected: the stored choice,
/// or Official when nothing (or a deleted server) is stored.
pub fn entries(list: &ServerList) -> Vec<Entry> {
    let selected = list.selected_server().map(|s| s.id.clone());
    let from = |s: &Server, kind: &'static str| Entry {
        id: s.id.clone(),
        name: s.name.clone(),
        host: s.host.clone(),
        port: s.port,
        address: address_text(&s.host, s.port),
        kind,
        selected: selected.as_deref() == Some(s.id.as_str()),
        public_key: s.public_key.clone(),
        getdown_url: s.client_url.clone(),
        content_url: s.content_url.clone(),
    };
    let mut rows = vec![Entry {
        id: OFFICIAL_ID.to_owned(),
        name: "Official".to_owned(),
        host: official::HOST.to_owned(),
        port: official::PORT,
        address: address_text(official::HOST, official::PORT),
        kind: "official",
        selected: selected.is_none(),
        public_key: official::PUBLIC_KEY.to_owned(),
        getdown_url: official::CLIENT_URL.to_owned(),
        content_url: official::CONTENT_URL.to_owned(),
    }];
    rows.extend(
        list.servers
            .iter()
            .filter(|s| s.id != LOCAL_ID)
            .map(|s| from(s, "saved")),
    );
    rows.extend(
        list.servers
            .iter()
            .filter(|s| s.id == LOCAL_ID)
            .map(|s| from(s, "local")),
    );
    rows
}

/// The add/edit form as typed.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Draft {
    /// Empty when adding.
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub address: String,
    #[serde(default)]
    pub port: String,
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub getdown: String,
    #[serde(default)]
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FieldError {
    pub code: &'static str,
    /// A value the message mentions (the other server's name, the address's port); may be empty.
    pub detail: String,
}

fn err(code: &'static str) -> FieldError {
    FieldError {
        code,
        detail: String::new(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Change {
    pub key: &'static str,
    pub value: String,
}

/// What the form says: the problems per field (`name`, `address`, `port`, `key`, `getdown`, `content`, `form`) and,
/// when there are none, the connection settings the game will get.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Report {
    pub ok: bool,
    pub errors: BTreeMap<&'static str, FieldError>,
    pub changes: Vec<Change>,
}

/// A server that passes every check, used to test one field at a time.
fn blank(id: &str) -> Server {
    Server {
        id: id.to_owned(),
        name: "x".to_owned(),
        host: "x".to_owned(),
        port: 1,
        public_key: String::new(),
        client_url: String::new(),
        content_url: String::new(),
    }
}

/// Checks the form against the stored list. Returns the report and, when it is fine, the server it describes (with
/// the draft's id, or an empty id for a new one).
pub fn check(draft: &Draft, list: &ServerList) -> (Report, Option<Server>) {
    use servers::Problem;
    let mut errors: BTreeMap<&'static str, FieldError> = BTreeMap::new();
    let id = draft.id.trim();
    if id == LOCAL_ID || id == OFFICIAL_ID {
        errors.insert("form", err("managed"));
    }
    let probe_id = if id.is_empty() { "new" } else { id };
    let with = |edit: &dyn Fn(&mut Server)| {
        let mut s = blank(probe_id);
        edit(&mut s);
        servers::validate(&s, &list.servers)
    };

    let name = draft.name.trim().to_owned();
    if name.is_empty() {
        errors.insert("name", err("name_required"));
    } else if name.chars().count() > 40 {
        errors.insert("name", err("name_long"));
    } else if with(&|s| s.name = name.clone()) == Some(Problem::Duplicate) {
        errors.insert("name", err("name_duplicate"));
    }

    let mut host = String::new();
    let mut address_port = None;
    if draft.address.trim().is_empty() {
        errors.insert("address", err("address_required"));
    } else {
        match servers::split_address(&draft.address) {
            Err(port) => {
                errors.insert(
                    "address",
                    FieldError {
                        code: "address_port",
                        detail: port,
                    },
                );
            }
            Ok((h, p)) => {
                host = h.trim_matches(['[', ']']).to_owned();
                address_port = p;
                if host.is_empty() || with(&|s| s.host = host.clone()).is_some() {
                    errors.insert("address", err("address_invalid"));
                }
            }
        }
    }

    let typed = draft.port.trim();
    let mut port = None;
    if typed.is_empty() {
        match address_port {
            Some(p) => port = Some(p),
            None => {
                errors.insert("port", err("port_required"));
            }
        }
    } else {
        match typed.parse::<u16>() {
            Ok(p) if p > 0 => match address_port {
                Some(a) if a != p => {
                    errors.insert(
                        "port",
                        FieldError {
                            code: "port_mismatch",
                            detail: a.to_string(),
                        },
                    );
                }
                _ => port = Some(p),
            },
            _ => {
                errors.insert("port", err("port_range"));
            }
        }
    }

    let key = draft.key.trim().to_owned();
    if with(&|s| s.public_key = key.clone()) == Some(Problem::Key) {
        errors.insert("key", err("key_invalid"));
    }
    let getdown = draft.getdown.trim().to_owned();
    if with(&|s| s.client_url = getdown.clone()) == Some(Problem::Url) {
        errors.insert("getdown", err("url_invalid"));
    }
    let content = draft.content.trim().to_owned();
    if with(&|s| s.content_url = content.clone()) == Some(Problem::Url) {
        errors.insert("content", err("url_invalid"));
    }

    // The same game server twice is a duplicate too, whatever it is called (Official's address included).
    if let (false, Some(port), false) = (host.is_empty(), port, errors.contains_key("address")) {
        let same = |h: &str, p: u16| h.eq_ignore_ascii_case(&host) && p == port;
        if same(official::HOST, official::PORT) {
            errors.insert("address", err("address_official"));
        } else if let Some(other) = list
            .servers
            .iter()
            .find(|s| s.id != probe_id && same(&s.host, s.port))
        {
            errors.insert(
                "address",
                FieldError {
                    code: "address_duplicate",
                    detail: other.name.clone(),
                },
            );
        }
    }

    let server = Server {
        id: id.to_owned(),
        name,
        host,
        port: port.unwrap_or(0),
        public_key: key,
        client_url: getdown,
        content_url: content,
    };
    // Whatever the per-field checks missed still may not reach the stored list.
    if errors.is_empty() {
        let probe = Server {
            id: probe_id.to_owned(),
            ..server.clone()
        };
        if servers::validate(&probe, &list.servers).is_some() {
            errors.insert("form", err("invalid"));
        }
    }
    let ok = errors.is_empty();
    let changes = if ok {
        servers::connection_summary(Some(&server))
            .into_iter()
            .map(|(key, value)| Change { key, value })
            .collect()
    } else {
        Vec::new()
    };
    (
        Report {
            ok,
            errors,
            changes,
        },
        ok.then_some(server),
    )
}

/// Validates and stores the form as a new server (empty id) or as a change to an existing one. Returns the id.
pub fn save(book: &Book, draft: &Draft) -> Result<String, Report> {
    let outcome = book.update(|list| {
        let (report, server) = check(draft, list);
        let Some(mut server) = server else {
            return Err(report);
        };
        if server.id.is_empty() {
            server.id = servers::new_id();
        } else if !list.servers.iter().any(|s| s.id == server.id) {
            return Err(Report {
                ok: false,
                errors: BTreeMap::from([("form", err("missing"))]),
                changes: Vec::new(),
            });
        }
        let id = server.id.clone();
        list.save(server).map(|()| id).map_err(|_| Report {
            ok: false,
            errors: BTreeMap::from([("form", err("invalid"))]),
            changes: Vec::new(),
        })
    });
    match outcome {
        Ok(result) => result,
        Err(_) => Err(Report {
            ok: false,
            errors: BTreeMap::from([("form", err("write"))]),
            changes: Vec::new(),
        }),
    }
}

/// The draft for editing the saved server `id`.
pub fn draft_of(list: &ServerList, id: &str) -> Option<serde_json::Value> {
    let s = list.servers.iter().find(|s| s.id == id)?;
    Some(serde_json::json!({
        "id": s.id, "name": s.name, "address": s.host, "port": s.port.to_string(),
        "key": s.public_key, "getdown": s.client_url, "content": s.content_url,
    }))
}

/// Makes `id` the server the next launch uses (`official` or an unknown id selects Official).
pub fn select(book: &Book, id: &str) -> io::Result<()> {
    book.update(|list| list.select((id != OFFICIAL_ID).then_some(id)))
}

/// Removes a saved server. Official and the hosted server are not removable here.
pub fn remove(book: &Book, id: &str) -> io::Result<bool> {
    if id == OFFICIAL_ID || id == LOCAL_ID {
        return Ok(false);
    }
    book.update(|list| {
        let before = list.servers.len();
        list.delete(id);
        list.servers.len() != before
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft(name: &str, address: &str, port: &str) -> Draft {
        Draft {
            name: name.into(),
            address: address.into(),
            port: port.into(),
            ..Draft::default()
        }
    }

    fn code(report: &Report, field: &str) -> Option<&'static str> {
        report.errors.get(field).map(|e| e.code)
    }

    fn book() -> (tempfile::TempDir, Book) {
        let dir = tempfile::tempdir().unwrap();
        let book = Book::new(dir.path().join(FILE));
        (dir, book)
    }

    #[test]
    fn a_name_and_an_address_are_required() {
        let (r, s) = check(&draft("", "", "47624"), &ServerList::default());
        assert!(s.is_none() && !r.ok);
        assert_eq!(code(&r, "name"), Some("name_required"));
        assert_eq!(code(&r, "address"), Some("address_required"));
        assert!(r.changes.is_empty());
    }

    #[test]
    fn every_field_is_checked_on_its_own_and_all_problems_show_at_once() {
        let mut d = draft(&"n".repeat(41), "bad host!", "70000");
        d.key = "nothex".into();
        d.getdown = "ftp://x".into();
        d.content = "not a url".into();
        let (r, _) = check(&d, &ServerList::default());
        assert_eq!(code(&r, "name"), Some("name_long"));
        assert_eq!(code(&r, "address"), Some("address_invalid"));
        assert_eq!(code(&r, "port"), Some("port_range"));
        assert_eq!(code(&r, "key"), Some("key_invalid"));
        assert_eq!(code(&r, "getdown"), Some("url_invalid"));
        assert_eq!(code(&r, "content"), Some("url_invalid"));
        // Only the URL that is wrong is blamed.
        d.getdown = "https://dl.example".into();
        let (r, _) = check(&d, &ServerList::default());
        assert_eq!(code(&r, "getdown"), None);
        assert_eq!(code(&r, "content"), Some("url_invalid"));
    }

    #[test]
    fn ports_are_one_to_65535_and_an_address_port_fills_an_empty_field() {
        for bad in ["0", "65536", "-1", "x", "1.5"] {
            let (r, _) = check(&draft("A", "play.example", bad), &ServerList::default());
            assert_eq!(code(&r, "port"), Some("port_range"), "{bad}");
        }
        let (r, _) = check(&draft("A", "play.example", ""), &ServerList::default());
        assert_eq!(code(&r, "port"), Some("port_required"));
        let (r, s) = check(&draft("A", "play.example:9000", ""), &ServerList::default());
        assert!(r.ok, "{r:?}");
        assert_eq!(s.unwrap().port, 9000);
        let (r, _) = check(
            &draft("A", "play.example:9000", "9001"),
            &ServerList::default(),
        );
        let e = &r.errors["port"];
        assert_eq!((e.code, e.detail.as_str()), ("port_mismatch", "9000"));
        let (r, _) = check(
            &draft("A", "play.example:99999", "9000"),
            &ServerList::default(),
        );
        assert_eq!(code(&r, "address"), Some("address_port"));
    }

    #[test]
    fn the_host_is_split_from_the_port_before_it_is_stored() {
        let (_r, s) = check(&draft("A", "[::1]:5000", ""), &ServerList::default());
        let s = s.unwrap();
        assert_eq!((s.host.as_str(), s.port), ("::1", 5000));
        assert_eq!(address_text(&s.host, s.port), "[::1]:5000");
        let (_r, s) = check(
            &draft("A", "Play.Example:47624", "47624"),
            &ServerList::default(),
        );
        assert_eq!(s.unwrap().host, "Play.Example");
    }

    #[test]
    fn duplicates_by_name_and_by_address_are_refused_but_editing_yourself_is_not() {
        let (_d, book) = book();
        let id = save(&book, &draft("Haven", "haven.example", "47624")).unwrap();
        let list = book.load();
        let (r, _) = check(&draft("haven", "other.example", "1"), &list);
        assert_eq!(code(&r, "name"), Some("name_duplicate"));
        let (r, _) = check(&draft("Other", "HAVEN.example", "47624"), &list);
        let e = &r.errors["address"];
        assert_eq!((e.code, e.detail.as_str()), ("address_duplicate", "Haven"));
        let (r, _) = check(&draft("Other", "haven.example", "47625"), &list);
        assert!(r.ok, "another port is another server");
        let mut edit = draft("Haven", "haven.example", "47624");
        edit.id = id;
        assert!(check(&edit, &list).0.ok);
        let (r, _) = check(&draft("Official", "x.example", "1"), &list);
        assert_eq!(code(&r, "name"), Some("name_duplicate"));
        let (r, _) = check(
            &draft(
                "Mine",
                "Game.SpiralKnights.com",
                &official::PORT.to_string(),
            ),
            &list,
        );
        assert_eq!(code(&r, "address"), Some("address_official"));
    }

    #[test]
    fn the_hosted_server_cannot_be_edited_here() {
        let mut d = draft("Local Server", "127.0.0.1", "47624");
        d.id = LOCAL_ID.into();
        assert_eq!(
            code(&check(&d, &ServerList::default()).0, "form"),
            Some("managed")
        );
    }

    #[test]
    fn the_report_shows_what_the_game_configuration_gets() {
        let mut d = draft("A", "play.example", "9000");
        d.getdown = "https://dl.example/client/x".into();
        let (r, _) = check(&d, &ServerList::default());
        let get = |k: &str| r.changes.iter().find(|c| c.key == k).unwrap().value.clone();
        assert_eq!(get("server_host"), "play.example");
        assert_eq!(get("server_ports"), "9000");
        assert_eq!(get("client_root_url"), "https://dl.example");
        assert_eq!(get("key.public"), official::PUBLIC_KEY);
    }

    #[test]
    fn saving_edits_and_deleting_persist_in_the_store() {
        let (_d, book) = book();
        let id = save(&book, &draft("A", "a.example", "1000")).unwrap();
        let mut d = draft("A2", "a.example", "1001");
        d.id = id.clone();
        assert_eq!(save(&book, &d).unwrap(), id);
        let list = book.load();
        assert_eq!(list.servers.len(), 1);
        assert_eq!(list.servers[0].name, "A2");
        assert_eq!(list.servers[0].port, 1001);
        d.id = "nope".into();
        d.name = "A3".into();
        d.port = "1002".into();
        assert_eq!(code(&save(&book, &d).unwrap_err(), "form"), Some("missing"));
        assert!(save(&book, &draft("", "", "")).is_err());
        assert!(remove(&book, &id).unwrap());
        assert!(book.load().servers.is_empty());
        assert!(!remove(&book, OFFICIAL_ID).unwrap());
    }

    #[test]
    fn the_list_has_official_first_the_hosted_server_last_and_one_selection() {
        let (_d, book) = book();
        let a = save(&book, &draft("A", "a.example", "1")).unwrap();
        book.update(|l| {
            l.save(Server {
                id: LOCAL_ID.into(),
                name: "Local Server".into(),
                host: "127.0.0.1".into(),
                port: 47624,
                public_key: String::new(),
                client_url: String::new(),
                content_url: String::new(),
            })
            .unwrap()
        })
        .unwrap();
        let rows = entries(&book.load());
        let kinds: Vec<_> = rows.iter().map(|e| (e.id.as_str(), e.kind)).collect();
        assert_eq!(
            kinds,
            [
                (OFFICIAL_ID, "official"),
                (a.as_str(), "saved"),
                (LOCAL_ID, "local")
            ]
        );
        assert_eq!(rows.iter().filter(|e| e.selected).count(), 1);
        assert!(rows[0].selected, "nothing stored means Official");
        select(&book, &a).unwrap();
        let rows = entries(&book.load());
        assert!(rows[1].selected && !rows[0].selected);
        select(&book, OFFICIAL_ID).unwrap();
        assert!(entries(&book.load())[0].selected);
        // A selected server that is deleted leaves Official selected.
        select(&book, &a).unwrap();
        remove(&book, &a).unwrap();
        assert!(entries(&book.load())[0].selected);
    }

    #[test]
    fn the_hosted_server_can_be_chosen_like_any_other() {
        let (_d, book) = book();
        book.update(|l| {
            l.save(Server {
                id: LOCAL_ID.into(),
                name: "Local Server".into(),
                host: "127.0.0.1".into(),
                port: 47624,
                public_key: String::new(),
                client_url: String::new(),
                content_url: String::new(),
            })
        })
        .unwrap()
        .unwrap();
        select(&book, LOCAL_ID).unwrap();
        let rows = entries(&book.load());
        assert!(rows.iter().find(|e| e.id == LOCAL_ID).unwrap().selected);
        assert!(!rows[0].selected);
    }

    #[test]
    fn the_selection_survives_a_restart_and_is_what_the_launch_reads() {
        let (dir, book) = book();
        assert_eq!(selected_server_from(dir.path()), None);
        let id = save(&book, &draft("A", "a.example", "1234")).unwrap();
        select(&book, &id).unwrap();
        // A new `Book` over the same file is a new run of the program.
        let after = selected_server_from(dir.path()).expect("still selected");
        assert_eq!((after.host.as_str(), after.port), ("a.example", 1234));
        select(&Book::new(dir.path().join(FILE)), OFFICIAL_ID).unwrap();
        assert_eq!(selected_server_from(dir.path()), None);
    }
}
