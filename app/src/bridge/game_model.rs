//! `GameModel` (QML singleton `GameModel`): which Spiral Knights install the launcher uses. Discovery reads
//! Steam's library files, so it runs on a worker thread and posts its result back to the UI thread.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qstringlist.h");
        type QStringList = cxx_qt_lib::QStringList;
        include!("cxx-qt-lib/qurl.h");
        type QUrl = cxx_qt_lib::QUrl;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        /// "searching" until the first scan ends, then "ready" or "notfound".
        #[qproperty(QString, state, READ, WRITE, NOTIFY)]
        #[qproperty(
            QStringList,
            install_paths,
            cxx_name = "installPaths",
            READ,
            WRITE,
            NOTIFY
        )]
        /// "steam", "standalone" or "custom", one per path.
        #[qproperty(
            QStringList,
            install_kinds,
            cxx_name = "installKinds",
            READ,
            WRITE,
            NOTIFY
        )]
        #[qproperty(QString, selected_path, cxx_name = "selectedPath", READ, WRITE, NOTIFY)]
        #[qproperty(QString, selected_kind, cxx_name = "selectedKind", READ, WRITE, NOTIFY)]
        #[qproperty(QString, game_version, cxx_name = "gameVersion", READ, WRITE, NOTIFY)]
        /// The heap limit `extra.txt` sets in the selected install, in MB; 0 when it sets none.
        #[qproperty(
            i32,
            detected_memory_mb,
            cxx_name = "detectedMemoryMb",
            READ,
            WRITE,
            NOTIFY
        )]
        /// The reason the last folder choice was refused ("" when it was accepted).
        #[qproperty(QString, error, READ, WRITE, NOTIFY)]
        type GameModel = super::GameModelRust;

        /// Scans for installs again.
        #[qinvokable]
        fn refresh(self: Pin<&mut GameModel>);

        /// Uses the install at `path` (one of `installPaths`).
        #[qinvokable]
        fn select(self: Pin<&mut GameModel>, path: &QString);

        /// Checks the folder the player chose and, when it holds the game, remembers and selects it.
        #[qinvokable]
        #[cxx_name = "addCustomFolder"]
        fn add_custom_folder(self: Pin<&mut GameModel>, folder: &QUrl);

        /// Forgets the remembered folder.
        #[qinvokable]
        #[cxx_name = "forgetCustom"]
        fn forget_custom(self: Pin<&mut GameModel>);

        /// Screenshots only: shows the "notfound" or "ready" state whatever was found; "" shows the real one.
        #[qinvokable]
        fn force(self: Pin<&mut GameModel>, state: &QString);
    }

    impl cxx_qt::Threading for GameModel {}
    impl cxx_qt::Initialize for GameModel {}
}

use core::pin::Pin;

use cxx_qt::{CxxQtThread, CxxQtType, Initialize, Threading};
use cxx_qt_lib::{QString, QStringList, QUrl};
use skradle_core::game::{self, Env};

use crate::discovery::{self, Install, Resolved};
use crate::launch::memory;
use crate::settings::{self, Settings};

pub struct GameModelRust {
    state: QString,
    install_paths: QStringList,
    install_kinds: QStringList,
    selected_path: QString,
    selected_kind: QString,
    game_version: QString,
    detected_memory_mb: i32,
    error: QString,
    /// A state a screenshot forces ("" for none).
    forced: String,
    last: Resolved,
}

impl Default for GameModelRust {
    fn default() -> Self {
        Self {
            state: QString::from("searching"),
            install_paths: QStringList::default(),
            install_kinds: QStringList::default(),
            selected_path: QString::default(),
            selected_kind: QString::default(),
            game_version: QString::default(),
            detected_memory_mb: 0,
            error: QString::default(),
            forced: String::new(),
            last: Resolved::default(),
        }
    }
}

/// What a scan found, computed off the UI thread.
pub struct Snapshot {
    pub resolved: Resolved,
    pub version: Option<String>,
    pub memory_mb: Option<u32>,
}

/// Scans the machine with `env` and resolves the result against the settings.
pub fn scan(settings: &Settings, env: &Env) -> Snapshot {
    let found = game::locate(env);
    let custom = settings
        .custom_game_path
        .as_deref()
        .map(std::path::Path::new);
    let saved = settings.game_path.as_deref().map(std::path::Path::new);
    let resolved = discovery::resolve(found, custom, saved);
    let (version, memory_mb) = match resolved.selected_install() {
        Some(Install { path, .. }) => (
            discovery::game_version(path),
            std::fs::read_to_string(path.join("extra.txt"))
                .ok()
                .and_then(|t| memory::xmx_mb(&t)),
        ),
        None => (None, None),
    };
    Snapshot {
        resolved,
        version,
        memory_mb,
    }
}

/// The state to show: what was found, unless a screenshot forces one.
fn shown_state(found: bool, forced: &str) -> &'static str {
    match forced {
        "notfound" => "notfound",
        "ready" => "ready",
        _ if found => "ready",
        _ => "notfound",
    }
}

impl qobject::GameModel {
    fn apply(mut self: Pin<&mut Self>, snapshot: Snapshot) {
        let mut paths = QStringList::default();
        let mut kinds = QStringList::default();
        for install in &snapshot.resolved.installs {
            paths.append(QString::from(install.path.to_string_lossy().as_ref()));
            kinds.append(QString::from(install.kind.as_str()));
        }
        let selected = snapshot.resolved.selected_install().cloned();
        let found = selected.is_some();
        self.as_mut().rust_mut().last = snapshot.resolved;
        self.as_mut().set_install_paths(paths);
        self.as_mut().set_install_kinds(kinds);
        self.as_mut().set_selected_path(QString::from(
            selected
                .as_ref()
                .map_or_else(String::new, |i| i.path.to_string_lossy().into_owned())
                .as_str(),
        ));
        self.as_mut().set_selected_kind(QString::from(
            selected.as_ref().map_or("", |i| i.kind.as_str()),
        ));
        self.as_mut()
            .set_game_version(QString::from(snapshot.version.unwrap_or_default().as_str()));
        self.as_mut()
            .set_detected_memory_mb(snapshot.memory_mb.map_or(0, |m| m as i32));
        let state = shown_state(found, &self.forced);
        self.set_state(QString::from(state));
    }

    fn rescan(self: Pin<&mut Self>) {
        let thread: CxxQtThread<qobject::GameModel> = self.qt_thread();
        let spawned = std::thread::Builder::new()
            .name("skradle-scan".into())
            .spawn(move || {
                let snapshot = scan(&settings::global().get(), &Env::current());
                let _ = thread.queue(move |model| model.apply(snapshot));
            });
        if spawned.is_err() {
            // No thread, no scan: say "not found" rather than searching forever.
            self.set_state(QString::from("notfound"));
        }
    }

    pub fn refresh(self: Pin<&mut Self>) {
        self.rescan();
    }

    pub fn select(self: Pin<&mut Self>, path: &QString) {
        let path = path.to_string();
        settings::global().update(|s| s.game_path = Some(path));
        self.rescan();
    }

    pub fn add_custom_folder(mut self: Pin<&mut Self>, folder: &QUrl) {
        self.as_mut().set_error(QString::default());
        let chosen = discovery::path_from_url_text(&folder.to_qstring().to_string());
        let thread = self.qt_thread();
        let _ = std::thread::Builder::new()
            .name("skradle-folder".into())
            .spawn(move || match discovery::validate_folder(&chosen) {
                Ok(dir) => {
                    let text = dir.to_string_lossy().into_owned();
                    settings::global().update(|s| {
                        s.custom_game_path = Some(text.clone());
                        s.game_path = Some(text);
                    });
                    let snapshot = scan(&settings::global().get(), &Env::current());
                    let _ = thread.queue(move |model| model.apply(snapshot));
                }
                Err(e) => {
                    let message = QString::from(e.message());
                    let _ = thread.queue(move |model| model.set_error(message));
                }
            });
    }

    pub fn forget_custom(self: Pin<&mut Self>) {
        settings::global().update(|s| {
            if s.game_path == s.custom_game_path {
                s.game_path = None;
            }
            s.custom_game_path = None;
        });
        self.rescan();
    }

    pub fn force(mut self: Pin<&mut Self>, state: &QString) {
        self.as_mut().rust_mut().forced = state.to_string();
        let found = self.last.selected_install().is_some();
        let state = shown_state(found, &self.forced);
        self.set_state(QString::from(state));
    }
}

impl Initialize for qobject::GameModel {
    fn initialize(self: Pin<&mut Self>) {
        self.rescan();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use skradle_core::game::Os;
    use std::fs;

    fn env(home: &std::path::Path) -> Env {
        Env {
            os: Os::MacOs,
            home: Some(home.to_path_buf()),
            program_files_x86: None,
            local_app_data: None,
        }
    }

    fn make_game(dir: &std::path::Path, extra: Option<&str>) {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join("getdown.txt"), "version = 20260101000000\n").unwrap();
        fs::write(dir.join("getdown-pro.jar"), b"jar").unwrap();
        if let Some(extra) = extra {
            fs::write(dir.join("extra.txt"), extra).unwrap();
        }
    }

    #[test]
    fn forced_state_wins_over_what_was_found() {
        assert_eq!(shown_state(true, ""), "ready");
        assert_eq!(shown_state(false, ""), "notfound");
        assert_eq!(shown_state(true, "notfound"), "notfound");
        assert_eq!(shown_state(false, "ready"), "ready");
    }

    #[test]
    fn scan_with_no_game_is_empty() {
        let home = tempfile::tempdir().unwrap();
        let snap = scan(&Settings::default(), &env(home.path()));
        assert!(snap.resolved.installs.is_empty());
        assert!(snap.version.is_none() && snap.memory_mb.is_none());
    }

    #[test]
    fn scan_finds_steam_reads_version_and_memory() {
        let home = tempfile::tempdir().unwrap();
        let game = home
            .path()
            .join("Library/Application Support/Steam/steamapps/common/Spiral Knights");
        make_game(&game, Some("-Xmx4096M\n"));
        let snap = scan(&Settings::default(), &env(home.path()));
        assert_eq!(snap.resolved.selected, Some(0));
        assert_eq!(snap.version.as_deref(), Some("20260101000000"));
        assert_eq!(snap.memory_mb, Some(4096));
    }

    #[test]
    fn scan_honours_the_custom_folder_and_the_saved_choice() {
        let home = tempfile::tempdir().unwrap();
        let steam = home
            .path()
            .join("Library/Application Support/Steam/steamapps/common/Spiral Knights");
        make_game(&steam, None);
        let custom = tempfile::tempdir().unwrap();
        make_game(custom.path(), Some("-Xmx1g\n"));
        let mut settings = Settings::default();
        settings.custom_game_path = Some(custom.path().display().to_string());
        settings.game_path = settings.custom_game_path.clone();
        let snap = scan(&settings, &env(home.path()));
        assert_eq!(snap.resolved.installs.len(), 2);
        assert_eq!(snap.resolved.selected, Some(1));
        assert_eq!(snap.memory_mb, Some(1024));
    }
}
