//! The server this player hosts: the interface the Servers page needs, and its implementation on top of
//! `skradle_core::serverhost`.
//!
//! [`ServerHost`] is what the QML bridge talks to. [`RealHost`] runs every lifecycle call (install, start, stop,
//! backup, update...) on a worker thread of its own and keeps the result in memory; the bridge polls
//! [`ServerHost::snapshot`], which only reads memory and the small `config/server.yml`, so it is safe on a timer.
//! (The engine jar's SHA-256 is not: it is computed on a worker at start-up and after an install, and cached.)
//!
//! Anything the player must be told is a [`Notice`] with a code; the wording is in QML, where it can be translated.
//! [`FixtureHost`] is for the self-test and the screenshot run only: it never starts Java or touches the network.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use serde::Serialize;
use skradle_core::game::Os;
use skradle_core::net::Client;
use skradle_core::runtime::{self, RuntimeEnv};
use skradle_core::serverhost::admin;
use skradle_core::serverhost::backups::{self, BackupInfo};
use skradle_core::serverhost::config::{self, HostConfig};
use skradle_core::serverhost::engine::{self, EngineError, EngineState, Stage, UpdateOutcome};
use skradle_core::serverhost::layout::ServerLayout;
use skradle_core::serverhost::ports::{self, PortUse};
use skradle_core::serverhost::process::{
    self, HostError, HostOptions, Phase, ServerHost as CoreHost, StartupFailure, WebState,
};
use skradle_core::serverhost::register::{self, RegisterError};
use skradle_core::serverhost::terms;

use crate::servers::Book;
use crate::settings;

/// A thing the player should hear about, for the page to word.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Notice {
    /// `error`, `warn` or `info`.
    pub kind: &'static str,
    pub code: &'static str,
    /// Text from the engine or the system that the message may quote; may be empty.
    pub detail: String,
    /// For busy-port notices: the port, and a free one to offer (0 when none).
    pub port: u16,
    pub suggestion: u16,
}

fn notice(kind: &'static str, code: &'static str, detail: impl Into<String>) -> Notice {
    Notice {
        kind,
        code,
        detail: detail.into(),
        port: 0,
        suggestion: 0,
    }
}

/// What the other players need, once the server has a key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Join {
    pub host: String,
    pub port: u16,
    #[serde(rename = "contentUrl")]
    pub content_url: String,
    #[serde(rename = "reachableByOthers")]
    pub reachable_by_others: bool,
}

/// Everything the control strip shows. Serialised to JSON for QML.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Snapshot {
    /// `checking`, `notinstalled`, `stopped`, `starting`, `running`, `stopping`, `failed` or `busy`.
    pub state: &'static str,
    /// While `busy`: `install`, `update`, `backup` or `restore`; otherwise empty.
    pub task: &'static str,
    /// `downloading`, `verifying`, `unpacking` or `installing` during an install; otherwise empty.
    pub step: &'static str,
    /// 0..1 while a download reports its size, -1 when there is nothing to measure.
    pub progress: f64,
    pub notice: Option<Notice>,
    #[serde(rename = "engineVersion")]
    pub engine_version: String,
    /// The engine's files were changed or damaged outside the launcher.
    #[serde(rename = "engineModified")]
    pub engine_modified: bool,
    #[serde(rename = "termsAccepted")]
    pub terms_accepted: bool,
    /// The Java that will run the engine ("25.0.1"), empty when none was found.
    pub java: String,
    pub lan: bool,
    #[serde(rename = "gamePort")]
    pub game_port: u16,
    #[serde(rename = "webPort")]
    pub web_port: u16,
    /// The administration page's address while the server runs.
    pub url: String,
    /// `ready`, `refused`, `waiting` or empty.
    pub web: &'static str,
    #[serde(rename = "adminUser")]
    pub admin_user: String,
    /// `ready`, `failed` or empty.
    pub admin: &'static str,
    pub join: Option<Join>,
    /// Counts changes to the saved server list made by registering the hosted server.
    pub revision: u64,
}

impl Snapshot {
    fn empty(state: &'static str) -> Self {
        Self {
            state,
            task: "",
            step: "",
            progress: -1.0,
            notice: None,
            engine_version: String::new(),
            engine_modified: false,
            terms_accepted: false,
            java: String::new(),
            lan: false,
            game_port: config::DEFAULT_GAME_PORT,
            web_port: config::DEFAULT_WEB_PORT,
            url: String::new(),
            web: "",
            admin_user: String::new(),
            admin: "",
            join: None,
            revision: 0,
        }
    }
}

/// One backup, for the restore list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BackupView {
    pub name: String,
    pub label: String,
    #[serde(rename = "createdAt")]
    pub created_at: u64,
    pub bytes: u64,
}

impl From<BackupInfo> for BackupView {
    fn from(b: BackupInfo) -> Self {
        Self {
            name: b.name,
            label: b.label.unwrap_or_default(),
            created_at: b.created_at,
            bytes: b.bytes,
        }
    }
}

/// What the page can ask of the hosted server. Every action returns at once; its outcome shows in the snapshot.
pub trait ServerHost: Send + Sync {
    fn snapshot(&self) -> Snapshot;
    /// Looks at the engine, Java and saved list again (on a worker).
    fn refresh(&self);
    /// Records the licence acceptance (when `accepted`) and installs the engine.
    fn install(&self, accepted: bool);
    fn update_engine(&self);
    fn start(&self);
    fn stop(&self);
    fn restart(&self);
    fn backup(&self);
    fn restore(&self, name: &str);
    fn backups(&self) -> Vec<BackupView>;
    fn set_lan(&self, on: bool);
    fn set_ports(&self, game: u16, web: u16);
    fn reset_admin_password(&self);
    fn admin_password(&self) -> Option<String>;
    fn dismiss(&self);
    /// Stops the engine gracefully when the app quits.
    fn shutdown(&self);
    /// Screenshots only: shows a named state.
    fn force(&self, _name: &str) {}
}

/// The words of a busy-port or startup problem as a notice.
fn startup_notice(failure: &StartupFailure, config: &HostConfig) -> Notice {
    let (code, port, avoid) = match failure {
        StartupFailure::GamePortBusy(_) => ("port_game_busy", config.game_port, config.web_port),
        StartupFailure::WebPortBusy(_) => ("port_web_busy", config.web_port, config.game_port),
        StartupFailure::DataLocked(t) => return notice("error", "data_locked", t.clone()),
        StartupFailure::Other(t) => return notice("error", "startup_failed", t.clone()),
    };
    Notice {
        port,
        suggestion: ports::suggest(port, &[avoid]).unwrap_or(0),
        ..notice("error", code, failure.to_string())
    }
}

fn engine_notice(e: &EngineError) -> Notice {
    use skradle_core::net::NetError;
    match e {
        EngineError::TermsNotAccepted => notice("error", "terms", ""),
        EngineError::Net(NetError::Status(code)) if matches!(code, 403 | 429) => {
            notice("error", "rate_limited", code.to_string())
        }
        EngineError::Net(NetError::Status(code)) => {
            notice("error", "download_failed", format!("HTTP {code}"))
        }
        EngineError::Net(NetError::Request(why)) => notice("error", "offline", why.clone()),
        EngineError::Net(other) => notice("error", "download_failed", other.to_string()),
        EngineError::BadListing(_) => notice("error", "no_release", e.to_string()),
        EngineError::ChecksumMismatch => notice("error", "checksum", ""),
        EngineError::BadArchive(_) | EngineError::UnsafeEntry(..) => {
            notice("error", "bad_archive", e.to_string())
        }
        EngineError::NoPrevious => notice("error", "no_previous", ""),
        EngineError::Io(e) => notice("error", "io", e.to_string()),
    }
}

/// Turns a failed lifecycle call into the notice the page shows.
fn describe(e: &HostError, config: &HostConfig) -> Option<Notice> {
    Some(match e {
        HostError::NotInstalled => notice("error", "not_installed", ""),
        HostError::NoJava => notice("error", "no_java", ""),
        HostError::NotConfigured => notice("error", "not_configured", ""),
        HostError::PortConflict(list) => {
            let first = list.first()?;
            let code = match (first.which, first.same_as_other) {
                (_, true) => "port_same",
                (PortUse::Game, false) => "port_game_busy",
                (PortUse::Web, false) => "port_web_busy",
            };
            Notice {
                port: first.port,
                suggestion: first.suggestion.unwrap_or(0),
                ..notice("error", code, "")
            }
        }
        HostError::Spawn(e) => notice("error", "spawn", e.to_string()),
        // The player pressed Stop while it was starting: not a failure.
        HostError::Aborted => return None,
        HostError::EarlyExit { code, tail } => {
            let lines: Vec<String> = tail.iter().rev().take(3).rev().cloned().collect();
            notice(
                "error",
                "early_exit",
                format!(
                    "{}{}",
                    code.map(|c| format!("exit code {c}; ")).unwrap_or_default(),
                    lines.join(" | ")
                ),
            )
        }
        HostError::StartupFailed(f) => startup_notice(f, config),
        HostError::StartTimeout => notice("error", "start_timeout", ""),
        HostError::NotRunning => notice("error", "not_running", ""),
        HostError::Console(why) => notice("error", "console", why.clone()),
        HostError::Config(e) => notice("error", "config", e.to_string()),
        HostError::Engine(e) => engine_notice(e),
        HostError::Reentrant | HostError::Io(_) => notice("error", "io", e.to_string()),
    })
}

/// A long operation in progress.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Task {
    Install,
    Update,
    Backup,
    Restore,
    Start,
    Restart,
    Stop,
    Configure,
}

impl Task {
    /// Shown as "busy" with this name; the others show the server's own state.
    fn busy_name(self) -> &'static str {
        match self {
            Task::Install => "install",
            Task::Update => "update",
            Task::Backup => "backup",
            Task::Restore => "restore",
            Task::Start | Task::Restart | Task::Stop | Task::Configure => "",
        }
    }
}

/// The state word for a core phase and the task in progress. `installed` is `None` until the files were looked at.
fn state_word(phase: &Phase, task: Option<Task>, installed: Option<bool>) -> &'static str {
    if task.is_some_and(|t| !t.busy_name().is_empty()) {
        return "busy";
    }
    match (phase, task) {
        (Phase::Running, Some(Task::Stop)) | (Phase::Stopping, _) => "stopping",
        (Phase::Running, Some(Task::Restart)) => "starting",
        (Phase::Running, _) => "running",
        (Phase::Starting, _) | (Phase::Stopped, Some(Task::Start | Task::Restart)) => "starting",
        (Phase::Failed(_), None) => "failed",
        (Phase::Failed(_), Some(Task::Start | Task::Restart)) => "starting",
        (Phase::Failed(_), Some(_)) | (Phase::Stopped, _) => match installed {
            None => "checking",
            Some(false) => "notinstalled",
            Some(true) => "stopped",
        },
    }
}

#[derive(Debug, Clone, Default)]
struct EngineInfo {
    /// Whether the files have been looked at yet.
    known: bool,
    ready: bool,
    version: String,
    modified: bool,
}

#[derive(Debug, Clone)]
struct JavaInfo {
    exe: PathBuf,
    version: String,
}

#[derive(Debug, Clone, Copy)]
struct TaskState {
    task: Task,
    step: &'static str,
    progress: f64,
}

struct Inner {
    layout: ServerLayout,
    book: Book,
    core: Mutex<Option<(Arc<CoreHost>, PathBuf)>>,
    engine: Mutex<EngineInfo>,
    java: Mutex<Option<JavaInfo>>,
    task: Mutex<Option<TaskState>>,
    notice: Mutex<Option<Notice>>,
    join: Mutex<Option<Join>>,
    revision: AtomicU64,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Inner {
    fn host_config(&self) -> HostConfig {
        config::read_file(&self.layout)
            .map(|c| c.host_config())
            .unwrap_or_default()
    }

    fn set_notice(&self, n: Option<Notice>) {
        *lock(&self.notice) = n;
    }

    /// Takes the one task slot. `Stop` may join a `Start` that is waiting (it cancels it).
    fn begin(&self, task: Task) -> bool {
        let mut slot = lock(&self.task);
        if let Some(current) = *slot {
            let cancelling =
                task == Task::Stop && matches!(current.task, Task::Start | Task::Restart);
            if !cancelling {
                return false;
            }
        }
        *slot = Some(TaskState {
            task,
            step: "",
            progress: -1.0,
        });
        drop(slot);
        self.set_notice(None);
        true
    }

    fn end(&self) {
        *lock(&self.task) = None;
    }

    fn progress(&self, step: &'static str, progress: f64) {
        if let Some(t) = lock(&self.task).as_mut() {
            t.step = step;
            t.progress = progress;
        }
    }

    fn refresh_engine(&self) {
        let state = engine::installed(&self.layout);
        let mut info = EngineInfo {
            known: true,
            ready: engine::is_ready(&self.layout),
            ..EngineInfo::default()
        };
        match state {
            EngineState::Installed(e) => info.version = e.version,
            EngineState::Modified => info.modified = true,
            EngineState::NotInstalled => {}
        }
        *lock(&self.engine) = info;
    }

    fn refresh_java(&self) -> Option<JavaInfo> {
        let managed = dirs::data_dir().map(|d| d.join("SKradle").join("runtimes"));
        let runtimes = runtime::find_runtimes(&RuntimeEnv::current(), None, managed.as_deref());
        let found = process::pick_server_runtime(&runtimes).map(|r| {
            let v = &r.release.version;
            JavaInfo {
                exe: r.java_executable(Os::current()),
                version: format!("{}.{}.{}", v.major, v.minor, v.patch),
            }
        });
        *lock(&self.java) = found.clone();
        found
    }

    /// The core host for the current Java. Rebuilt only when the server is not running and another Java is now the
    /// right one (the options are fixed when a host is made). With no Java the host still serves installs, backups
    /// and updates; `start` then says `NoJava`.
    fn core(&self) -> Arc<CoreHost> {
        let java = lock(&self.java)
            .as_ref()
            .map(|j| j.exe.clone())
            .unwrap_or_default();
        let mut slot = lock(&self.core);
        if let Some((host, current)) = slot.as_ref() {
            let idle =
                matches!(host.status().phase, Phase::Stopped | Phase::Failed(_)) && !host.is_busy();
            if *current == java || !idle {
                return host.clone();
            }
        }
        let host = Arc::new(CoreHost::new(
            self.layout.clone(),
            HostOptions::new(java.clone()),
        ));
        *slot = Some((host.clone(), java));
        host
    }

    /// Puts the hosted server into the saved list (or updates its entry), once the engine has made its key.
    fn sync_registration(&self) {
        if !lock(&self.engine).ready {
            return;
        }
        let config = self.host_config();
        *lock(&self.join) = register::join_info(&self.layout, &config, None)
            .ok()
            .map(|j| Join {
                host: j.host,
                port: j.port,
                content_url: j.content_url,
                reachable_by_others: j.reachable_by_others,
            });
        let server = match register::local_server(&self.layout, &config) {
            Ok(server) => server,
            Err(RegisterError::NoKey) => return,
            Err(e) => {
                self.set_notice(Some(notice("warn", "register_failed", e.to_string())));
                return;
            }
        };
        let outcome = self.book.update(|list| {
            let before = list.servers.clone();
            let result = register::register(list, server);
            (result, before != list.servers)
        });
        match outcome {
            Ok((Ok(()), changed)) => {
                if changed {
                    self.revision.fetch_add(1, Ordering::SeqCst);
                }
            }
            Ok((Err(_), _)) => self.set_notice(Some(notice("warn", "register_duplicate", ""))),
            Err(e) => self.set_notice(Some(notice("warn", "register_failed", e.to_string()))),
        }
    }

    /// Makes `config/server.yml` the launcher's form again if it is not (the engine's shipped file, or a hand edit).
    fn ensure_configured(&self, core: &CoreHost) -> Result<(), HostError> {
        let current = config::read_file(&self.layout).map_err(HostError::Config)?;
        if current.is_launcher_configured() {
            return Ok(());
        }
        core.configure(&current.host_config())
    }

    fn fail(&self, e: &HostError) {
        let n = describe(e, &self.host_config());
        self.set_notice(n);
    }

    fn do_start(&self, restart: bool) {
        let task = if restart { Task::Restart } else { Task::Start };
        if !self.begin(task) {
            return;
        }
        self.refresh_engine();
        self.refresh_java();
        if !lock(&self.engine).ready {
            self.fail(&HostError::NotInstalled);
            self.end();
            return;
        }
        let core = self.core();
        let result = self.ensure_configured(&core).and_then(|()| {
            if restart {
                core.restart()
            } else {
                core.start()
            }
        });
        match result {
            Ok(_) => self.sync_registration(),
            Err(e) => self.fail(&e),
        }
        self.end();
    }

    fn start_again(&self, core: &CoreHost) {
        if let Err(e) = self.ensure_configured(core).and_then(|()| core.start()) {
            self.fail(&e);
        }
    }

    /// Runs `update_engine` (first install and update are the same call) and starts the server again afterwards
    /// when it was running.
    fn do_update(&self, task: Task) {
        if !self.begin(task) {
            return;
        }
        let core = self.core();
        let client = Client::new();
        let result = core.update_engine(&client, &mut |stage| match stage {
            Stage::Downloading(p) => self.progress(
                "downloading",
                p.total
                    .filter(|t| *t > 0)
                    .map_or(-1.0, |t| (p.done as f64 / t as f64).clamp(0.0, 1.0)),
            ),
            Stage::Verifying => self.progress("verifying", -1.0),
            Stage::Unpacking => self.progress("unpacking", -1.0),
            Stage::Installing | Stage::Done => self.progress("installing", -1.0),
        });
        self.refresh_engine();
        match result {
            Ok(report) => {
                let n = match &report.outcome {
                    UpdateOutcome::UpToDate(v) => notice("info", "up_to_date", v.clone()),
                    UpdateOutcome::Installed(e) => notice("info", "installed", e.version.clone()),
                };
                self.set_notice(Some(n));
                self.sync_registration();
                if report.was_running {
                    self.start_again(&core);
                }
            }
            Err(e) => self.fail(&e),
        }
        self.end();
    }
}

/// The real host: Tortodrone in the player's server folder.
pub struct RealHost {
    inner: Arc<Inner>,
}

/// Where the server lives: `SKRADLE_SERVER_DIR`, else `<data dir>/SKradle/server`. Test runs get a folder inside
/// their throw-away config folder instead, so they never touch the player's server.
pub fn server_dir(test_run: bool) -> PathBuf {
    if let Some(dir) = std::env::var_os("SKRADLE_SERVER_DIR").filter(|v| !v.is_empty()) {
        return PathBuf::from(dir);
    }
    if test_run {
        return settings::config_dir().join("server");
    }
    dirs::data_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("SKradle")
        .join("server")
}

impl RealHost {
    pub fn new(layout: ServerLayout, book: Book) -> Self {
        let host = Self {
            inner: Arc::new(Inner {
                layout,
                book,
                core: Mutex::new(None),
                engine: Mutex::new(EngineInfo::default()),
                java: Mutex::new(None),
                task: Mutex::new(None),
                notice: Mutex::new(None),
                join: Mutex::new(None),
                revision: AtomicU64::new(0),
            }),
        };
        host.refresh();
        host
    }

    fn spawn(&self, name: &str, work: impl FnOnce(&Inner) + Send + 'static) {
        let inner = self.inner.clone();
        let spawned = std::thread::Builder::new()
            .name(format!("skradle-host-{name}"))
            .spawn(move || work(&inner));
        if spawned.is_err() {
            self.inner
                .set_notice(Some(notice("error", "io", "no thread to run this on")));
            self.inner.end();
        }
    }

    fn configure(&self, wanted: HostConfig) {
        if wanted.validate().is_err() {
            let code = if wanted.game_port == wanted.web_port && wanted.game_port != 0 {
                "port_same"
            } else {
                "port_range"
            };
            self.inner.set_notice(Some(notice("error", code, "")));
            return;
        }
        self.spawn("configure", move |i| {
            if !i.begin(Task::Configure) {
                return;
            }
            let core = i.core();
            match core.configure(&wanted) {
                Ok(()) => {
                    i.sync_registration();
                    // A port that is taken now is worth a word before the player presses Start.
                    if let Some(c) = ports::check(&wanted).first() {
                        let code = match (c.which, c.same_as_other) {
                            (_, true) => "port_same",
                            (PortUse::Game, _) => "port_game_busy",
                            (PortUse::Web, _) => "port_web_busy",
                        };
                        i.set_notice(Some(Notice {
                            port: c.port,
                            suggestion: c.suggestion.unwrap_or(0),
                            ..notice("warn", code, "")
                        }));
                    }
                }
                Err(HostError::Config(_))
                    if matches!(core.status().phase, Phase::Running | Phase::Starting) =>
                {
                    i.set_notice(Some(notice("error", "stop_first", "")));
                }
                Err(e) => i.fail(&e),
            }
            i.end();
        });
    }
}

impl ServerHost for RealHost {
    fn snapshot(&self) -> Snapshot {
        let i = &self.inner;
        let engine = lock(&i.engine).clone();
        let task = *lock(&i.task);
        let status = lock(&i.core).as_ref().map(|(h, _)| h.status());
        let phase = status.as_ref().map_or(Phase::Stopped, |s| s.phase.clone());
        let state = state_word(
            &phase,
            task.map(|t| t.task),
            engine.known.then_some(engine.ready),
        );
        let config = i.host_config();
        let mut shown = lock(&i.notice).clone();
        if shown.is_none() {
            if let Phase::Failed(why) = &phase {
                shown = Some(notice("error", "ended", why.clone()));
            }
        }
        let admin_state = status.as_ref().map_or("", |s| match s.admin {
            process::AdminState::Ready => "ready",
            process::AdminState::Failed(_) => "failed",
            process::AdminState::Unknown => "",
        });
        Snapshot {
            task: task.map_or("", |t| t.task.busy_name()),
            step: task.map_or("", |t| t.step),
            progress: task.map_or(-1.0, |t| t.progress),
            notice: shown,
            engine_version: engine.version,
            engine_modified: engine.modified,
            terms_accepted: terms::is_accepted(&i.layout),
            java: lock(&i.java)
                .as_ref()
                .map(|j| j.version.clone())
                .unwrap_or_default(),
            lan: config.lan,
            game_port: config.game_port,
            web_port: config.web_port,
            url: status
                .as_ref()
                .filter(|_| state == "running")
                .and_then(|s| s.web_ui_url.clone())
                .unwrap_or_default(),
            web: match status.as_ref().map(|s| s.web) {
                Some(WebState::Ready) => "ready",
                Some(WebState::Refused(_)) => "refused",
                Some(WebState::NotAnswering) => "waiting",
                _ => "",
            },
            admin_user: if admin::load(&i.layout).is_some() {
                admin::ADMIN_USER.to_owned()
            } else {
                String::new()
            },
            admin: admin_state,
            join: lock(&i.join).clone(),
            revision: i.revision.load(Ordering::SeqCst),
            ..Snapshot::empty(state)
        }
    }

    fn refresh(&self) {
        self.spawn("refresh", |i| {
            i.refresh_engine();
            i.refresh_java();
            i.sync_registration();
        });
    }

    fn install(&self, accepted: bool) {
        if !accepted {
            self.inner.set_notice(Some(notice("error", "terms", "")));
            return;
        }
        self.spawn("install", |i| {
            let recorded = i
                .layout
                .ensure()
                .and_then(|()| terms::accept(&i.layout, terms::now_secs()));
            if let Err(e) = recorded {
                i.set_notice(Some(notice("error", "io", e.to_string())));
                return;
            }
            i.do_update(Task::Install);
        });
    }

    fn update_engine(&self) {
        self.spawn("update", |i| i.do_update(Task::Update));
    }

    fn start(&self) {
        self.spawn("start", |i| i.do_start(false));
    }

    fn restart(&self) {
        self.spawn("restart", |i| i.do_start(true));
    }

    fn stop(&self) {
        self.spawn("stop", |i| {
            if !i.begin(Task::Stop) {
                return;
            }
            if let Err(e) = i.core().stop() {
                i.fail(&e);
            }
            i.end();
        });
    }

    fn backup(&self) {
        self.spawn("backup", |i| {
            if !i.begin(Task::Backup) {
                return;
            }
            let core = i.core();
            match core.backup(Some("manual")) {
                Ok(done) => {
                    i.set_notice(Some(notice("info", "backup_done", done.value.name)));
                    if done.was_running {
                        i.start_again(&core);
                    }
                }
                Err(e) => i.fail(&e),
            }
            i.end();
        });
    }

    fn restore(&self, name: &str) {
        let name = name.to_owned();
        self.spawn("restore", move |i| {
            if !i.begin(Task::Restore) {
                return;
            }
            let core = i.core();
            match core.restore(&name) {
                Ok(done) => {
                    i.set_notice(Some(notice("info", "restored", name.clone())));
                    i.refresh_engine();
                    i.sync_registration();
                    if done.was_running {
                        i.start_again(&core);
                    }
                }
                Err(e) => i.fail(&e),
            }
            i.end();
        });
    }

    fn backups(&self) -> Vec<BackupView> {
        backups::list(&self.inner.layout)
            .unwrap_or_default()
            .into_iter()
            .map(BackupView::from)
            .collect()
    }

    fn set_lan(&self, on: bool) {
        let current = self.inner.host_config();
        self.configure(HostConfig { lan: on, ..current });
    }

    fn set_ports(&self, game: u16, web: u16) {
        let current = self.inner.host_config();
        self.configure(HostConfig {
            game_port: game,
            web_port: web,
            ..current
        });
    }

    fn reset_admin_password(&self) {
        self.spawn("admin", |i| match i.core().reset_admin_password() {
            Ok(_) => i.set_notice(Some(notice("info", "password_reset", ""))),
            Err(e) => i.fail(&e),
        });
    }

    fn admin_password(&self) -> Option<String> {
        admin::load(&self.inner.layout).map(|c| c.reveal_password().to_owned())
    }

    fn dismiss(&self) {
        self.inner.set_notice(None);
    }

    fn shutdown(&self) {
        let host = lock(&self.inner.core).as_ref().map(|(h, _)| h.clone());
        // Only a live engine needs stopping; an install or backup in progress holds the lifecycle lock with the
        // server already stopped, and must not make quitting wait for it.
        if let Some(host) = host {
            if matches!(
                host.status().phase,
                Phase::Starting | Phase::Running | Phase::Stopping
            ) {
                let _ = host.stop();
            }
        }
    }
}

/// A `data:` URL for an HTML page (percent-encoded, so it survives every URL parser).
pub fn data_url(html: &str) -> String {
    let mut url = String::from("data:text/html;charset=utf-8,");
    for byte in html.bytes() {
        if byte.is_ascii_alphanumeric() || b"-_.~".contains(&byte) {
            url.push(byte as char);
        } else {
            url.push_str(&format!("%{byte:02X}"));
        }
    }
    url
}

/// The page the fixture "running" server serves: it sets its own title, so the self-test can tell that an embedded
/// web view really loaded and ran a script.
const FIXTURE_PAGE: &str = "<!doctype html><meta charset=utf-8><title>web view</title>\
<style>body{font:15px system-ui,sans-serif;margin:40px;color:#222}code{background:#eee;padding:2px 6px;border-radius:4px}</style>\
<h1>Test page</h1><p>Stands in for the server's web interface in the self-test. Script: <code id=js>not run</code></p>\
<script>document.getElementById('js').textContent='script ran';document.title='script ran'</script>";

/// The host for the self-test and the screenshot run: fixed states, no Java, no network, no files.
pub struct FixtureHost {
    state: Mutex<Snapshot>,
}

impl FixtureHost {
    pub fn new() -> Self {
        let host = Self {
            state: Mutex::new(Snapshot::empty("stopped")),
        };
        host.force("running");
        host
    }
}

impl ServerHost for FixtureHost {
    fn snapshot(&self) -> Snapshot {
        lock(&self.state).clone()
    }
    fn refresh(&self) {}
    fn install(&self, _: bool) {}
    fn update_engine(&self) {}
    fn start(&self) {}
    fn stop(&self) {}
    fn restart(&self) {}
    fn backup(&self) {}
    fn restore(&self, _: &str) {}
    fn backups(&self) -> Vec<BackupView> {
        vec![
            BackupView {
                name: "20260301_120000-before-update".into(),
                label: "before-update".into(),
                created_at: 1_772_366_400,
                bytes: 48_000_000,
            },
            BackupView {
                name: "20260228_090000-manual".into(),
                label: "manual".into(),
                created_at: 1_772_269_200,
                bytes: 47_500_000,
            },
        ]
    }
    fn set_lan(&self, _: bool) {}
    fn set_ports(&self, _: u16, _: u16) {}
    fn reset_admin_password(&self) {}
    fn admin_password(&self) -> Option<String> {
        Some("Fixture-password-0000".into())
    }
    fn dismiss(&self) {
        lock(&self.state).notice = None;
    }
    fn shutdown(&self) {}

    fn force(&self, name: &str) {
        let mut s = Snapshot::empty("stopped");
        s.engine_version = "0.40.17".into();
        s.terms_accepted = true;
        s.java = "25.0.1".into();
        match name {
            "notinstalled" => {
                s.state = "notinstalled";
                s.engine_version.clear();
                s.terms_accepted = false;
            }
            "nojava" => s.java.clear(),
            "stopped" => {}
            "starting" => s.state = "starting",
            "busy" => {
                s.state = "busy";
                s.task = "install";
                s.step = "downloading";
                s.progress = 0.42;
            }
            "failed" => {
                s.state = "failed";
                s.notice = Some(notice(
                    "error",
                    "ended",
                    "the server ended unexpectedly (exit code 1)",
                ));
            }
            "portbusy" => {
                s.notice = Some(Notice {
                    port: 8080,
                    suggestion: 8081,
                    ..notice("error", "port_web_busy", "")
                });
            }
            "offline" => {
                s.engine_version.clear();
                s.state = "notinstalled";
                s.notice = Some(notice("error", "offline", "dns error"));
            }
            _ => {
                s.state = "running";
                s.web = "ready";
                s.url = data_url(FIXTURE_PAGE);
                s.admin_user = admin::ADMIN_USER.into();
                s.admin = "ready";
                s.lan = name == "running-lan";
                s.join = Some(Join {
                    host: "192.168.1.20".into(),
                    port: 47624,
                    content_url: "http://192.168.1.20:8080/".into(),
                    reachable_by_others: s.lan,
                });
            }
        }
        *lock(&self.state) = s;
    }
}

static HOST: std::sync::OnceLock<Box<dyn ServerHost>> = std::sync::OnceLock::new();

/// The process-wide host, made on first use: the fixture host in a test run, the real one otherwise.
pub fn global() -> &'static dyn ServerHost {
    HOST.get_or_init(|| {
        if crate::options::get().is_test_run() {
            Box::new(FixtureHost::new())
        } else {
            Box::new(RealHost::new(
                ServerLayout::new(server_dir(false)),
                Book::global(),
            ))
        }
    })
    .as_ref()
}

/// Called when the app quits: stops the engine gracefully if the host was ever used.
pub fn shutdown() {
    if let Some(host) = HOST.get() {
        host.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use skradle_core::serverhost::ports::Conflict;

    fn cfg() -> HostConfig {
        HostConfig::default()
    }

    #[test]
    fn data_urls_are_fully_encoded() {
        assert_eq!(
            data_url("<a b>"),
            "data:text/html;charset=utf-8,%3Ca%20b%3E"
        );
        assert!(!data_url("é#?&").contains(['#', '?', '&', 'é']));
    }

    #[test]
    fn states_follow_the_phase_and_the_task_in_progress() {
        let known = Some(true);
        let failed = Phase::Failed("x".into());
        assert_eq!(state_word(&Phase::Stopped, None, None), "checking");
        assert_eq!(
            state_word(&Phase::Stopped, None, Some(false)),
            "notinstalled"
        );
        assert_eq!(state_word(&Phase::Stopped, None, known), "stopped");
        assert_eq!(state_word(&Phase::Running, None, known), "running");
        assert_eq!(
            state_word(&Phase::Starting, Some(Task::Start), known),
            "starting"
        );
        // The task is set before the core has a run to report a phase for.
        assert_eq!(
            state_word(&Phase::Stopped, Some(Task::Start), known),
            "starting"
        );
        assert_eq!(
            state_word(&Phase::Running, Some(Task::Stop), known),
            "stopping"
        );
        assert_eq!(
            state_word(&Phase::Running, Some(Task::Restart), known),
            "starting"
        );
        assert_eq!(state_word(&failed, None, known), "failed");
        // A failed run being started again is starting, not failed.
        assert_eq!(state_word(&failed, Some(Task::Start), known), "starting");
        for task in [Task::Install, Task::Update, Task::Backup, Task::Restore] {
            assert_eq!(state_word(&Phase::Stopped, Some(task), known), "busy");
            assert_eq!(state_word(&Phase::Running, Some(task), known), "busy");
        }
    }

    #[test]
    fn a_busy_port_is_named_with_a_free_one_to_offer() {
        let e = HostError::PortConflict(vec![Conflict {
            which: PortUse::Web,
            port: 8080,
            same_as_other: false,
            suggestion: Some(8081),
        }]);
        let n = describe(&e, &cfg()).unwrap();
        assert_eq!(
            (n.code, n.port, n.suggestion),
            ("port_web_busy", 8080, 8081)
        );
        let e = HostError::PortConflict(vec![Conflict {
            which: PortUse::Game,
            port: 47624,
            same_as_other: true,
            suggestion: None,
        }]);
        assert_eq!(describe(&e, &cfg()).unwrap().code, "port_same");
    }

    #[test]
    fn what_the_engine_says_at_startup_becomes_a_specific_notice() {
        let game = StartupFailure::GamePortBusy("Address already in use".into());
        let n = startup_notice(&game, &cfg());
        assert_eq!((n.code, n.port), ("port_game_busy", 47624));
        let web = StartupFailure::WebPortBusy("Cannot bind shared HTTP listener".into());
        assert_eq!(startup_notice(&web, &cfg()).port, 8080);
        let other = HostError::StartupFailed(StartupFailure::Other("boom".into()));
        assert_eq!(describe(&other, &cfg()).unwrap().detail, "boom");
        assert!(
            describe(&HostError::Aborted, &cfg()).is_none(),
            "a cancelled start is not an error"
        );
        assert_eq!(
            describe(&HostError::NoJava, &cfg()).unwrap().code,
            "no_java"
        );
        assert_eq!(
            describe(&HostError::NotInstalled, &cfg()).unwrap().code,
            "not_installed"
        );
    }

    #[test]
    fn offline_and_rate_limits_are_told_apart_from_other_download_failures() {
        use skradle_core::net::NetError;
        let code = |e| engine_notice(&e).code;
        assert_eq!(
            code(EngineError::Net(NetError::Request("dns".into()))),
            "offline"
        );
        assert_eq!(
            code(EngineError::Net(NetError::Status(403))),
            "rate_limited"
        );
        assert_eq!(
            code(EngineError::Net(NetError::Status(500))),
            "download_failed"
        );
        assert_eq!(code(EngineError::ChecksumMismatch), "checksum");
        assert_eq!(code(EngineError::TermsNotAccepted), "terms");
    }

    fn real() -> (tempfile::TempDir, RealHost) {
        let dir = tempfile::tempdir().unwrap();
        let host = RealHost::new(
            ServerLayout::new(dir.path().join("server")),
            Book::new(dir.path().join("servers.json")),
        );
        (dir, host)
    }

    fn wait_until(host: &RealHost, done: impl Fn(&Snapshot) -> bool) -> Snapshot {
        for _ in 0..200 {
            let s = host.snapshot();
            if done(&s) {
                return s;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        panic!("timed out: {:?}", host.snapshot());
    }

    #[test]
    fn an_empty_folder_is_not_installed_and_starting_says_so() {
        let (_d, host) = real();
        let s = wait_until(&host, |s| s.state != "checking");
        assert_eq!(s.state, "notinstalled");
        assert!(!s.terms_accepted);
        assert_eq!(s.revision, 0);
        host.start();
        let s = wait_until(&host, |s| s.notice.is_some());
        assert_eq!(s.notice.unwrap().code, "not_installed");
        assert_eq!(host.snapshot().state, "notinstalled");
    }

    #[test]
    fn installing_without_accepting_the_licence_downloads_nothing() {
        let (dir, host) = real();
        host.install(false);
        let s = host.snapshot();
        assert_eq!(s.notice.unwrap().code, "terms");
        assert!(!dir
            .path()
            .join("server")
            .join("terms-accepted.json")
            .exists());
    }

    #[test]
    fn invalid_ports_are_refused_before_anything_is_written() {
        let (dir, host) = real();
        host.set_ports(8080, 8080);
        assert_eq!(host.snapshot().notice.unwrap().code, "port_same");
        host.set_ports(0, 8080);
        assert_eq!(host.snapshot().notice.unwrap().code, "port_range");
        assert!(!dir.path().join("server").join("config").exists());
    }

    #[test]
    fn the_fixture_host_runs_nothing_and_shows_named_states() {
        let host = FixtureHost::new();
        assert_eq!(host.snapshot().state, "running");
        assert!(host.snapshot().url.starts_with("data:text/html"));
        for state in ["notinstalled", "failed", "busy", "stopped"] {
            host.force(state);
            assert_eq!(host.snapshot().state, state);
        }
    }
}
