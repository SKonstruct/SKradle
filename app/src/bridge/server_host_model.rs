//! `ServerHost` (QML singleton): the player's own server, as seen by the Servers page. A thin copy of
//! `crate::server_host`: a poller thread reads the host's snapshot three times a second and posts what changed to
//! the UI thread; every action is handed to the host, which runs it on a worker thread of its own.

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
        /// `checking`, `notinstalled`, `stopped`, `starting`, `running`, `stopping`, `failed` or `busy`.
        #[qproperty(QString, state, READ, WRITE, NOTIFY)]
        /// The server's web interface address while it runs.
        #[qproperty(QString, url, READ, WRITE, NOTIFY)]
        /// The whole snapshot as JSON (see `server_host::Snapshot`).
        #[qproperty(QString, status, READ, WRITE, NOTIFY)]
        type ServerHost = super::ServerHostRust;

        /// Looks at the engine, Java and the saved list again.
        #[qinvokable]
        fn refresh(self: &ServerHost);
        /// Installs the engine; `accepted` is the player's confirmation of its licence.
        #[qinvokable]
        fn install(self: &ServerHost, accepted: bool);
        #[qinvokable]
        #[cxx_name = "updateEngine"]
        fn update_engine(self: &ServerHost);
        #[qinvokable]
        fn start(self: &ServerHost);
        #[qinvokable]
        fn stop(self: &ServerHost);
        #[qinvokable]
        fn restart(self: &ServerHost);
        /// Backs up (stopping a running server first, starting it again afterwards).
        #[qinvokable]
        fn backup(self: &ServerHost);
        #[qinvokable]
        fn restore(self: &ServerHost, name: &QString);
        /// The backups as a JSON array, newest first.
        #[qinvokable]
        #[cxx_name = "backupsJson"]
        fn backups_json(self: &ServerHost) -> QString;
        #[qinvokable]
        #[cxx_name = "setLan"]
        fn set_lan(self: &ServerHost, on: bool);
        #[qinvokable]
        #[cxx_name = "setPorts"]
        fn set_ports(self: &ServerHost, game: i32, web: i32);
        #[qinvokable]
        #[cxx_name = "resetAdminPassword"]
        fn reset_admin_password(self: &ServerHost);
        /// The web interface administrator's password, for showing or copying on request.
        #[qinvokable]
        #[cxx_name = "adminPassword"]
        fn admin_password(self: &ServerHost) -> QString;
        /// The script that signs the embedded web view in to the page at `origin` (empty without credentials).
        /// Contains the password: run it in the view, never show or log it.
        #[qinvokable]
        #[cxx_name = "signInScript"]
        fn sign_in_script(self: &ServerHost, origin: &QString) -> QString;
        /// Console output from `cursor` on, as JSON `{lines, cursor, live}`. Never blocks.
        #[qinvokable]
        #[cxx_name = "consoleSince"]
        fn console_since(self: &ServerHost, cursor: i32) -> QString;
        /// Sends a line to the server's console; returns JSON `{ok, echo}` (`echo` is what to show back; a
        /// secret typed at a prompt is masked there and not remembered).
        #[qinvokable]
        #[cxx_name = "consoleSend"]
        fn console_send(self: &ServerHost, line: &QString) -> QString;
        /// Up (`older` true) or Down in the command history; empty string when there is nothing to show.
        #[qinvokable]
        #[cxx_name = "consoleRecall"]
        fn console_recall(self: &ServerHost, older: bool, current: &QString) -> QString;
        /// Clears the message shown in the control strip.
        #[qinvokable]
        fn dismiss(self: &ServerHost);
        /// The licence summary the install dialog shows, as JSON.
        #[qinvokable]
        #[cxx_name = "termsJson"]
        fn terms_json(self: &ServerHost) -> QString;
        /// Screenshots only: shows a named state.
        #[qinvokable]
        fn force(self: &ServerHost, name: &QString);
    }

    impl cxx_qt::Threading for ServerHost {}
    impl cxx_qt::Initialize for ServerHost {}
}

use core::pin::Pin;
use std::time::Duration;

use cxx_qt::{CxxQtThread, Initialize, Threading};
use cxx_qt_lib::QString;
use skradle_core::serverhost::terms;

use crate::server_host::{self, Snapshot};
use skradle_core::serverhost::console;
use std::sync::Mutex;

/// What the console remembers between calls: the history and the last line the engine printed (to tell a prompt).
#[derive(Default)]
struct ConsoleState {
    history: console::History,
    last_line: String,
}
static CONSOLE: Mutex<Option<ConsoleState>> = Mutex::new(None);

fn with_console<R>(f: impl FnOnce(&mut ConsoleState) -> R) -> R {
    let mut guard = CONSOLE.lock().unwrap_or_else(|e| e.into_inner());
    f(guard.get_or_insert_with(ConsoleState::default))
}

#[derive(Default)]
pub struct ServerHostRust {
    state: QString,
    url: QString,
    status: QString,
}

/// A port from QML, clamped into `u16` (0 is rejected later by the host as out of range).
fn port(value: i32) -> u16 {
    u16::try_from(value).unwrap_or(0)
}

impl qobject::ServerHost {
    fn apply(mut self: Pin<&mut Self>, snapshot: &Snapshot) {
        let json = serde_json::to_string(snapshot).unwrap_or_default();
        self.as_mut().set_state(QString::from(snapshot.state));
        self.as_mut().set_url(QString::from(snapshot.url.as_str()));
        self.set_status(QString::from(json.as_str()));
    }

    pub fn refresh(&self) {
        server_host::global().refresh();
    }
    pub fn install(&self, accepted: bool) {
        server_host::global().install(accepted);
    }
    pub fn update_engine(&self) {
        server_host::global().update_engine();
    }
    pub fn start(&self) {
        server_host::global().start();
    }
    pub fn stop(&self) {
        server_host::global().stop();
    }
    pub fn restart(&self) {
        server_host::global().restart();
    }
    pub fn backup(&self) {
        server_host::global().backup();
    }
    pub fn restore(&self, name: &QString) {
        server_host::global().restore(&name.to_string());
    }
    pub fn backups_json(&self) -> QString {
        let list = server_host::global().backups();
        QString::from(serde_json::to_string(&list).unwrap_or_default().as_str())
    }
    pub fn set_lan(&self, on: bool) {
        server_host::global().set_lan(on);
    }
    pub fn set_ports(&self, game: i32, web: i32) {
        server_host::global().set_ports(port(game), port(web));
    }
    pub fn reset_admin_password(&self) {
        server_host::global().reset_admin_password();
    }
    pub fn admin_password(&self) -> QString {
        QString::from(
            server_host::global()
                .admin_password()
                .unwrap_or_default()
                .as_str(),
        )
    }
    pub fn sign_in_script(&self, origin: &QString) -> QString {
        QString::from(
            server_host::global()
                .sign_in_script(&origin.to_string())
                .unwrap_or_default()
                .as_str(),
        )
    }
    pub fn console_since(&self, cursor: i32) -> QString {
        let (lines, next, live) = server_host::global().console_since(cursor.max(0) as u64);
        if let Some(last) = lines.last() {
            with_console(|c| c.last_line = last.clone());
        }
        let json = serde_json::json!({ "lines": lines, "cursor": next, "live": live });
        QString::from(json.to_string().as_str())
    }
    pub fn console_send(&self, line: &QString) -> QString {
        let text = line.to_string();
        let Some(command) = console::clean_command(&text) else {
            return QString::from(r#"{"ok":false,"echo":""}"#);
        };
        let secret = with_console(|c| console::looks_like_prompt(&c.last_line));
        let ok = server_host::global().console_send(&command);
        if ok && !secret {
            with_console(|c| c.history.push(&command));
        }
        let echo = if ok {
            console::echo(&command, secret)
        } else {
            String::new()
        };
        QString::from(
            serde_json::json!({ "ok": ok, "echo": echo })
                .to_string()
                .as_str(),
        )
    }
    pub fn console_recall(&self, older: bool, current: &QString) -> QString {
        let shown = with_console(|c| {
            if older {
                c.history.older(&current.to_string())
            } else {
                c.history.newer()
            }
        });
        QString::from(shown.unwrap_or_else(|| current.to_string()).as_str())
    }
    pub fn dismiss(&self) {
        server_host::global().dismiss();
    }
    pub fn terms_json(&self) -> QString {
        let json = serde_json::json!({
            "summary": terms::SUMMARY,
            "licence": terms::LICENCE_NAME,
            "licenceUrl": terms::LICENCE_URL,
            "engineUrl": terms::ENGINE_URL,
        });
        QString::from(json.to_string().as_str())
    }
    pub fn force(&self, name: &QString) {
        server_host::global().force(&name.to_string());
    }
}

impl Initialize for qobject::ServerHost {
    fn initialize(mut self: Pin<&mut Self>) {
        let first = server_host::global().snapshot();
        self.as_mut().apply(&first);
        let thread: CxxQtThread<qobject::ServerHost> = self.qt_thread();
        let mut last = first;
        // Ends when the QObject is gone (the queue call then fails).
        let _ = std::thread::Builder::new()
            .name("skradle-host-poll".into())
            .spawn(move || loop {
                std::thread::sleep(Duration::from_millis(300));
                let now = server_host::global().snapshot();
                if now == last {
                    continue;
                }
                last = now.clone();
                if thread.queue(move |model| model.apply(&now)).is_err() {
                    break;
                }
            });
    }
}
