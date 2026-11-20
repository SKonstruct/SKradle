//! `ModsModel` (QML singleton `ModsModel`): the mods folder as the Mods page shows it, the file actions on it and
//! the modpack sync. Scanning, copying and downloading run on worker threads; the page gets JSON strings (the
//! list, the tag counts, the result of the last action, the sync report) and plain properties for the rest. The
//! logic is in `crate::mods_host`.

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
        /// JSON array of the mods (see `mods_host::ItemView`), in the order they are applied.
        #[qproperty(QString, items, READ, WRITE, NOTIFY)]
        /// JSON array of `{id, count}`: only the tags the installed mods have.
        #[qproperty(QString, tags, READ, WRITE, NOTIFY)]
        /// The mods folder as a path and as a `file://` URL.
        #[qproperty(QString, folder, READ, WRITE, NOTIFY)]
        #[qproperty(QString, folder_url, cxx_name = "folderUrl", READ, WRITE, NOTIFY)]
        /// Why the folder could not be read; empty when it could.
        #[qproperty(QString, error, READ, WRITE, NOTIFY)]
        /// A scan or a file action is running.
        #[qproperty(bool, busy, READ, WRITE, NOTIFY)]
        /// The first scan has ended.
        #[qproperty(bool, loaded, READ, WRITE, NOTIFY)]
        /// JSON of the last action's outcome (`seq`, `kind`, `ok`, `renamed`, `failed`); a new one each time.
        #[qproperty(QString, result, READ, WRITE, NOTIFY)]
        /// "unknown", "checking", "notinstalled", "update", "current", "syncing" or "error".
        #[qproperty(QString, pack_state, cxx_name = "packState", READ, WRITE, NOTIFY)]
        /// What went wrong, in "error".
        #[qproperty(QString, pack_message, cxx_name = "packMessage", READ, WRITE, NOTIFY)]
        /// While "syncing": the file number, the total and the file name.
        #[qproperty(i32, pack_current, cxx_name = "packCurrent", READ, WRITE, NOTIFY)]
        #[qproperty(i32, pack_total, cxx_name = "packTotal", READ, WRITE, NOTIFY)]
        #[qproperty(QString, pack_name, cxx_name = "packName", READ, WRITE, NOTIFY)]
        /// Files a sync would download or delete, in "update".
        #[qproperty(i32, pack_changes, cxx_name = "packChanges", READ, WRITE, NOTIFY)]
        /// JSON report of the last sync (`downloaded`, `skipped`, `deleted`, `failed`, `warnings`, `cancelled`);
        /// empty when there is none to show.
        #[qproperty(QString, pack_report, cxx_name = "packReport", READ, WRITE, NOTIFY)]
        type ModsModel = super::ModsModelRust;

        /// Scans the folder again. `game_version` marks class mods built for another version.
        #[qinvokable]
        fn refresh(self: Pin<&mut ModsModel>, game_version: &QString);

        /// Switches the mods named in the JSON array of file names on or off.
        #[qinvokable]
        #[cxx_name = "setEnabled"]
        fn set_enabled(self: Pin<&mut ModsModel>, files: &QString, on: bool);

        /// Deletes the mods named in the JSON array of file names.
        #[qinvokable]
        fn remove(self: Pin<&mut ModsModel>, files: &QString);

        /// Validates and copies the files in the JSON array of paths or `file://` URLs into the folder.
        #[qinvokable]
        fn add(self: Pin<&mut ModsModel>, sources: &QString);

        /// Creates the folder when missing; its `file://` URL, or "" when that fails.
        #[qinvokable]
        #[cxx_name = "ensureFolder"]
        fn ensure_folder(self: Pin<&mut ModsModel>) -> QString;

        /// Compares the folder with the modpack's listing (one request, nothing downloaded).
        #[qinvokable]
        #[cxx_name = "checkPack"]
        fn check_pack(self: Pin<&mut ModsModel>);

        /// Installs or updates the modpack.
        #[qinvokable]
        #[cxx_name = "syncPack"]
        fn sync_pack(self: Pin<&mut ModsModel>);

        /// Stops the sync before the next file.
        #[qinvokable]
        #[cxx_name = "cancelSync"]
        fn cancel_sync(self: Pin<&mut ModsModel>);

        /// Hides the last sync's report.
        #[qinvokable]
        #[cxx_name = "dismissReport"]
        fn dismiss_report(self: Pin<&mut ModsModel>);

        /// Screenshots only: shows a modpack state ("update", "syncing", "report", "error") whatever is true.
        #[qinvokable]
        #[cxx_name = "forcePack"]
        fn force_pack(self: Pin<&mut ModsModel>, state: &QString);
    }

    impl cxx_qt::Threading for ModsModel {}
}

use core::pin::Pin;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use cxx_qt::{CxxQtThread, CxxQtType, Threading};
use cxx_qt_lib::QString;
use serde_json::json;
use skradle_core::mods::sync::{self, GithubHttp, PackStatus, SyncReport};

use crate::mods_host::{self, Listing, Outcome};

pub struct ModsModelRust {
    items: QString,
    tags: QString,
    folder: QString,
    folder_url: QString,
    error: QString,
    busy: bool,
    loaded: bool,
    result: QString,
    pack_state: QString,
    pack_message: QString,
    pack_current: i32,
    pack_total: i32,
    pack_name: QString,
    pack_changes: i32,
    pack_report: QString,
    dir: Option<PathBuf>,
    game_version: String,
    /// Answers from older scans are ignored.
    scan: u64,
    seq: u64,
    pack_request: u64,
    cancel: Arc<AtomicBool>,
}

impl Default for ModsModelRust {
    fn default() -> Self {
        let dir = mods_host::mods_dir();
        Self {
            items: QString::from("[]"),
            tags: QString::from("[]"),
            folder: QString::from(
                dir.as_deref()
                    .map(|d| d.to_string_lossy().into_owned())
                    .unwrap_or_default()
                    .as_str(),
            ),
            folder_url: QString::from(
                dir.as_deref()
                    .map(mods_host::file_url)
                    .unwrap_or_default()
                    .as_str(),
            ),
            error: QString::default(),
            busy: false,
            loaded: false,
            result: QString::default(),
            pack_state: QString::from("unknown"),
            pack_message: QString::default(),
            pack_current: 0,
            pack_total: 0,
            pack_name: QString::default(),
            pack_changes: 0,
            pack_report: QString::default(),
            dir,
            game_version: String::new(),
            scan: 0,
            seq: 0,
            pack_request: 0,
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }
}

type Thread = CxxQtThread<qobject::ModsModel>;

fn files_from_json(text: &QString) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(&text.to_string()).unwrap_or_default()
}

fn report_json(r: &SyncReport) -> String {
    json!({
        "downloaded": r.downloaded,
        "skipped": r.skipped.len(),
        "deleted": r.deleted,
        "failed": r
            .failed
            .iter()
            .map(|(file, why)| (file, mods_host::plain_pack_file(why)))
            .collect::<Vec<_>>(),
        "warnings": r.warnings,
        "cancelled": r.cancelled,
    })
    .to_string()
}

impl qobject::ModsModel {
    fn show_listing(mut self: Pin<&mut Self>, listing: &Listing) {
        let items = serde_json::to_string(&listing.items).unwrap_or_else(|_| "[]".into());
        let tags = serde_json::to_string(&listing.tags).unwrap_or_else(|_| "[]".into());
        self.as_mut().set_items(QString::from(items.as_str()));
        self.as_mut().set_tags(QString::from(tags.as_str()));
        self.as_mut()
            .set_error(QString::from(listing.error.as_deref().unwrap_or("")));
        self.as_mut().set_loaded(true);
    }

    /// Scans on a worker thread and shows the result, unless a newer scan was asked for meanwhile.
    fn scan_then<F>(mut self: Pin<&mut Self>, work: F)
    where
        F: FnOnce(&std::path::Path) -> Option<Outcome> + Send + 'static,
    {
        let Some(dir) = self.dir.clone() else {
            self.as_mut().set_error(QString::from(
                "There is no data folder on this computer to keep mods in.",
            ));
            self.as_mut().set_loaded(true);
            return;
        };
        self.as_mut().rust_mut().scan += 1;
        let id = self.scan;
        let version = self.game_version.clone();
        self.as_mut().set_busy(true);
        let thread: Thread = self.qt_thread();
        let spawned = std::thread::Builder::new()
            .name("skradle-mods".into())
            .spawn({
                let thread = thread.clone();
                move || {
                    let outcome = work(&dir);
                    let listing = mods_host::build_listing(
                        &dir,
                        Some(version.as_str()).filter(|v| !v.is_empty()),
                    );
                    let _ = thread.queue(move |mut m| {
                        if m.scan == id {
                            m.as_mut().show_listing(&listing);
                            m.as_mut().set_busy(false);
                        }
                        if let Some(outcome) = outcome {
                            m.publish(&outcome);
                        }
                    });
                }
            });
        if spawned.is_err() {
            self.as_mut().set_busy(false);
        }
    }

    fn publish(mut self: Pin<&mut Self>, outcome: &Outcome) {
        self.as_mut().rust_mut().seq += 1;
        let seq = self.seq;
        let mut value = serde_json::to_value(outcome).unwrap_or_default();
        value["seq"] = json!(seq);
        self.as_mut()
            .set_result(QString::from(value.to_string().as_str()));
    }

    pub fn refresh(mut self: Pin<&mut Self>, game_version: &QString) {
        self.as_mut().rust_mut().game_version = game_version.to_string();
        self.scan_then(|_| None);
    }

    pub fn set_enabled(mut self: Pin<&mut Self>, files: &QString, on: bool) {
        if self.busy || self.pack_state.to_string() == "syncing" {
            return;
        }
        let files = files_from_json(files);
        self.as_mut()
            .scan_then(move |dir| Some(mods_host::set_enabled_all(dir, &files, on)));
    }

    pub fn remove(mut self: Pin<&mut Self>, files: &QString) {
        if self.busy || self.pack_state.to_string() == "syncing" {
            return;
        }
        let files = files_from_json(files);
        self.as_mut()
            .scan_then(move |dir| Some(mods_host::remove_all(dir, &files)));
    }

    pub fn add(mut self: Pin<&mut Self>, sources: &QString) {
        if self.busy || self.pack_state.to_string() == "syncing" {
            return;
        }
        let sources = files_from_json(sources);
        self.as_mut()
            .scan_then(move |dir| Some(mods_host::add_all(dir, &sources)));
    }

    pub fn ensure_folder(self: Pin<&mut Self>) -> QString {
        match &self.dir {
            Some(dir) if std::fs::create_dir_all(dir).is_ok() => {
                QString::from(mods_host::file_url(dir).as_str())
            }
            _ => QString::default(),
        }
    }

    fn fail_pack(mut self: Pin<&mut Self>, why: &str) {
        eprintln!("skradle: modpack: {why}");
        self.as_mut()
            .set_pack_message(QString::from(mods_host::plain_pack_error(why).as_str()));
        self.as_mut().set_pack_state(QString::from("error"));
    }

    fn show_status(mut self: Pin<&mut Self>, status: &PackStatus) {
        let changes = status.to_download.len() + status.to_delete.len();
        self.as_mut().set_pack_changes(changes as i32);
        self.as_mut().set_pack_total(status.total as i32);
        let state = if status.installed && status.has_changes() {
            "update"
        } else if !status.installed && !status.to_download.is_empty() {
            "notinstalled"
        } else {
            "current"
        };
        self.as_mut().set_pack_state(QString::from(state));
    }

    pub fn check_pack(mut self: Pin<&mut Self>) {
        if matches!(self.pack_state.to_string().as_str(), "checking" | "syncing") {
            return;
        }
        let Some(dir) = self.dir.clone() else {
            return;
        };
        self.as_mut().rust_mut().pack_request += 1;
        let id = self.pack_request;
        self.as_mut().set_pack_message(QString::default());
        self.as_mut().set_pack_state(QString::from("checking"));
        let thread: Thread = self.qt_thread();
        let spawned = std::thread::Builder::new()
            .name("skradle-pack-check".into())
            .spawn({
                let thread = thread.clone();
                move || {
                    let result = sync::status(&dir, &GithubHttp::new());
                    let _ = thread.queue(move |mut m| {
                        if m.pack_request != id {
                            return;
                        }
                        match result {
                            Ok(status) => m.as_mut().show_status(&status),
                            Err(e) => m.as_mut().fail_pack(&e.to_string()),
                        }
                    });
                }
            });
        if let Err(e) = spawned {
            self.fail_pack(&e.to_string());
        }
    }

    pub fn sync_pack(mut self: Pin<&mut Self>) {
        if self.busy || self.pack_state.to_string() == "syncing" {
            return;
        }
        let Some(dir) = self.dir.clone() else {
            return;
        };
        self.as_mut().rust_mut().pack_request += 1;
        let id = self.pack_request;
        let cancel = Arc::new(AtomicBool::new(false));
        self.as_mut().rust_mut().cancel = cancel.clone();
        self.as_mut().set_pack_report(QString::default());
        self.as_mut().set_pack_message(QString::default());
        self.as_mut().set_pack_current(0);
        self.as_mut().set_pack_name(QString::default());
        self.as_mut().set_pack_state(QString::from("syncing"));
        let thread: Thread = self.qt_thread();
        let version = self.game_version.clone();
        let spawned = std::thread::Builder::new()
            .name("skradle-pack-sync".into())
            .spawn({
                let thread = thread.clone();
                move || {
                    let progress_thread = thread.clone();
                    let flag = cancel.clone();
                    let result = sync::sync_cancellable(
                        &dir,
                        &GithubHttp::new(),
                        &mut |p| {
                            let (current, total, name) =
                                (p.current as i32, p.total as i32, p.name.clone());
                            let _ = progress_thread.queue(move |mut m| {
                                if m.pack_request == id {
                                    m.as_mut().set_pack_current(current);
                                    m.as_mut().set_pack_total(total);
                                    m.as_mut().set_pack_name(QString::from(name.as_str()));
                                }
                            });
                        },
                        &|| flag.load(Ordering::Relaxed),
                    );
                    let listing = mods_host::build_listing(
                        &dir,
                        Some(version.as_str()).filter(|v| !v.is_empty()),
                    );
                    let _ = thread.queue(move |mut m| {
                        if m.pack_request != id {
                            return;
                        }
                        m.as_mut().show_listing(&listing);
                        match result {
                            Ok(report) => {
                                m.as_mut()
                                    .set_pack_report(QString::from(report_json(&report).as_str()));
                                if report.cancelled || !report.is_complete() {
                                    let left = report.failed.len().max(1);
                                    m.as_mut().set_pack_changes(left as i32);
                                    m.as_mut().set_pack_state(QString::from("update"));
                                } else {
                                    m.as_mut().set_pack_changes(0);
                                    m.as_mut().set_pack_state(QString::from("current"));
                                }
                            }
                            Err(e) => m.as_mut().fail_pack(&e.to_string()),
                        }
                    });
                }
            });
        if let Err(e) = spawned {
            self.fail_pack(&e.to_string());
        }
    }

    pub fn cancel_sync(self: Pin<&mut Self>) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    pub fn dismiss_report(self: Pin<&mut Self>) {
        self.set_pack_report(QString::default());
    }

    pub fn force_pack(mut self: Pin<&mut Self>, state: &QString) {
        let state = state.to_string();
        self.as_mut().set_pack_report(QString::default());
        self.as_mut().set_pack_message(QString::default());
        match state.as_str() {
            "update" => {
                self.as_mut().set_pack_changes(3);
                self.as_mut().set_pack_state(QString::from("update"));
            }
            "syncing" => {
                self.as_mut().set_pack_current(4);
                self.as_mut().set_pack_total(12);
                self.as_mut()
                    .set_pack_name(QString::from("Example Texture Pack.zip"));
                self.as_mut().set_pack_state(QString::from("syncing"));
            }
            "report" => {
                let report = SyncReport {
                    downloaded: vec!["Example A.zip".into(), "Example B.zip".into()],
                    skipped: vec!["Example C.zip".into()],
                    deleted: vec!["Old Example.zip".into()],
                    failed: vec![(
                        "Example D.zip".into(),
                        "the download did not match its published hash".into(),
                    )],
                    ..SyncReport::default()
                };
                self.as_mut()
                    .set_pack_report(QString::from(report_json(&report).as_str()));
                self.as_mut().set_pack_changes(1);
                self.as_mut().set_pack_state(QString::from("update"));
            }
            "error" => {
                self.as_mut().set_pack_message(QString::from(
                    "cannot read the modpack listing: connection failed",
                ));
                self.as_mut().set_pack_state(QString::from("error"));
            }
            "notinstalled" | "current" => {
                self.as_mut().set_pack_state(QString::from(state.as_str()));
            }
            _ => self.as_mut().set_pack_state(QString::from("unknown")),
        }
    }
}
