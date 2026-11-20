//! The engine as a child process: command line, output capture, readiness, a lifecycle lock, graceful stop.
//!
//! **Command line**: `java <flags> -jar tortodrone.jar --config config/server.yml --skip-client-assets`, working
//! directory the server folder (the engine finds `data/`, `content/` and `webui/` beside its jar). The flags are
//! `-Djava.awt.headless=true --enable-native-access=ALL-UNNAMED -Dstdout.encoding=UTF-8 -Dstderr.encoding=UTF-8`
//! (UTF-8 matters: the WebUI password prompt contains an en dash). The engine needs Java 25 and, from its imports,
//! the modules `java.sql`, `jdk.httpserver`, `java.management` and `java.logging`; [`pick_server_runtime`] checks a
//! runtime's `release` file for them (a game-bundled jlink runtime probably lacks them). `--skip-client-assets` is
//! always passed unless the caller names the game folder: without it the engine may search for the game and ask
//! for its folder; with redirected stdin it never waits, but it would still run its artwork export.
//!
//! **Readiness**: the game listener is ready at the engine's line `Done! Listening on <addr>:<port> (TCP)`; the web
//! listener (which the engine opens first) is probed over loopback ([`super::ports::probe_web`]). `WebUI Ready` is
//! printed even when startup failed, so it is not used. Ports are checked before the start; the engine's own
//! failure lines are read as well: `Server startup failed: ... Address already in use` (game port; with the WebUI on
//! the *process stays alive*, idle), `Cannot bind shared HTTP listener` (web port; the process exits with code 1)
//! and `Another server is using this JAR's data directory`.
//!
//! **Stopping** (with the WebUI on, console `stop` only stops the game listener and the process waits for a
//! restart; closing stdin does not end it either): send `shutdown`, answer its prompt with `SHUT DOWN PROCESS`,
//! wait up to a timeout; then SIGTERM (`kill -TERM`, unix only, no `unsafe`), wait again; then kill. That the JVM
//! actually exits after the confirmed shutdown, and that SIGTERM ends it, is read from the engine's source
//! (`ServerControl.exit`, a shutdown hook) and has not been run: no engine jar was available.
//!
//! **Locking**: [`ServerHost`] serialises start, stop, restart, engine updates, backups, restores and resets on one
//! lifecycle lock. Status, log reads and everything the reader threads do never take it. A lifecycle call made from
//! the thread that already holds the lock (for example from a callback the lifecycle invoked) fails with
//! [`HostError::Reentrant`] instead of deadlocking. Output lines are processed on the reader threads under a
//! separate state lock; no user callback ever runs there. A `stop` issued while a start is waiting aborts that
//! start (the child is killed) before it waits for the lock.
//!
//! **Secrets**: the setup code the engine prints on first run is replaced in the log buffer; passwords are only
//! ever written to the child's stdin and the private credential file, never to the buffer or a log.

use std::collections::VecDeque;
use std::ffi::OsString;
use std::fmt;
use std::io::{self, BufRead, Read, Write};
use std::path::PathBuf;
#[cfg(unix)]
use std::process::Command;
use std::process::{Child, ChildStdin, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::{self, ThreadId};
use std::time::{Duration, Instant};

use crate::getdown::CommandSpec;
use crate::net::Client;
use crate::runtime::{JavaRelease, Runtime};

use super::admin::{self, Credentials};
use super::backups::{self, BackupInfo, Stopped};
use super::config::{self, ConfigError, HostConfig};
use super::engine::{self, EngineError, Stage, UpdateOutcome};
use super::layout::ServerLayout;
use super::ports::{self, Conflict, WebProbe};
use super::prompts::{Outcome, PromptScript};
use super::terms;

pub const MIN_JAVA_MAJOR: u32 = 25;
/// Modules the engine's code imports from the JDK, beyond `java.base`.
pub const SERVER_MODULES: [&str; 4] = [
    "java.sql",
    "jdk.httpserver",
    "java.management",
    "java.logging",
];
pub const DEFAULT_JVM_ARGS: [&str; 4] = [
    "-Djava.awt.headless=true",
    "--enable-native-access=ALL-UNNAMED",
    "-Dstdout.encoding=UTF-8",
    "-Dstderr.encoding=UTF-8",
];
const MAX_LINE_BYTES: usize = 8 * 1024;

/// Whether a runtime can run the engine: Java 25 or newer and, when its `release` file lists modules, the
/// modules in [`SERVER_MODULES`].
pub fn supports_server(release: &JavaRelease) -> bool {
    release.version.major >= MIN_JAVA_MAJOR
        && release
            .modules
            .as_ref()
            .is_none_or(|m| SERVER_MODULES.iter().all(|needed| m.contains(*needed)))
}

/// The first runtime that can run the engine, in the order given (see `runtime::find_runtimes`).
pub fn pick_server_runtime(runtimes: &[Runtime]) -> Option<&Runtime> {
    runtimes.iter().find(|r| supports_server(&r.release))
}

/// How the engine's WebUI artwork export is handled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientAssets {
    /// `--skip-client-assets`: fallback icons, no export, no prompt.
    Skip,
    /// `--client <game folder>`: the engine prepares artwork from the installed game. Needs a full JDK with
    /// `javac`; if that is missing the engine keeps fallback icons. Not verified here.
    Game(PathBuf),
}

#[derive(Debug, Clone)]
pub struct HostOptions {
    /// The `java` executable (a runtime that passes [`supports_server`]).
    pub java: PathBuf,
    /// Extra JVM arguments, after the defaults and before `-jar`.
    pub jvm_args: Vec<OsString>,
    pub client_assets: ClientAssets,
    /// How long a start may take before the child is killed.
    pub start_timeout: Duration,
    /// How long to wait for the process to end after the confirmed `shutdown`, and again after SIGTERM.
    pub shutdown_timeout: Duration,
    /// How long a console exchange (an answered prompt sequence) may take.
    pub prompt_timeout: Duration,
    /// Lines kept in the output buffer.
    pub log_capacity: usize,
    /// Create the WebUI administrator when the engine does not have it yet (see [`super::admin::needs_apply`]).
    pub manage_admin: bool,
    /// Check the ports before starting. Only tests turn this off.
    pub check_ports: bool,
}

impl HostOptions {
    pub fn new(java: impl Into<PathBuf>) -> Self {
        Self {
            java: java.into(),
            jvm_args: Vec::new(),
            client_assets: ClientAssets::Skip,
            start_timeout: Duration::from_secs(90),
            shutdown_timeout: Duration::from_secs(20),
            prompt_timeout: Duration::from_secs(15),
            log_capacity: 2000,
            manage_admin: true,
            check_ports: true,
        }
    }
}

/// The command that starts the engine.
pub fn command_spec(layout: &ServerLayout, options: &HostOptions) -> CommandSpec {
    let mut args: Vec<OsString> = DEFAULT_JVM_ARGS.iter().map(OsString::from).collect();
    args.extend(options.jvm_args.iter().cloned());
    args.extend(["-jar", "tortodrone.jar", "--config", "config/server.yml"].map(OsString::from));
    match &options.client_assets {
        ClientAssets::Skip => args.push("--skip-client-assets".into()),
        ClientAssets::Game(dir) => {
            args.push("--client".into());
            args.push(dir.clone().into_os_string());
        }
    }
    CommandSpec {
        program: options.java.clone(),
        args,
        cwd: layout.root().to_path_buf(),
        env: Vec::new(),
    }
}

/// A bounded buffer of the engine's output lines with a cursor, so a viewer can ask for what is new.
#[derive(Debug)]
pub struct LogBuffer {
    capacity: usize,
    lines: VecDeque<(u64, String)>,
    next: u64,
}

impl LogBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            lines: VecDeque::new(),
            next: 0,
        }
    }

    pub fn push(&mut self, line: String) {
        if self.lines.len() == self.capacity {
            self.lines.pop_front();
        }
        self.lines.push_back((self.next, line));
        self.next += 1;
    }

    /// The lines with a sequence number of at least `cursor`, and the cursor to pass next time. Lines that fell
    /// out of the buffer are gone.
    pub fn since(&self, cursor: u64) -> (Vec<String>, u64) {
        let lines = self
            .lines
            .iter()
            .filter(|(n, _)| *n >= cursor)
            .map(|(_, l)| l.clone())
            .collect();
        (lines, self.next)
    }

    pub fn len(&self) -> usize {
        self.lines.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }
}

/// What the engine said went wrong at startup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartupFailure {
    /// The game port is taken.
    GamePortBusy(String),
    /// The shared web/News port is taken.
    WebPortBusy(String),
    /// Another engine process uses this folder's `data/`.
    DataLocked(String),
    Other(String),
}

impl StartupFailure {
    fn classify(text: &str) -> Self {
        let text = text.trim().to_owned();
        let busy = text.contains("Address already in use") || text.contains("Bind failed");
        if text.contains("shared HTTP listener") {
            StartupFailure::WebPortBusy(text)
        } else if text.contains("Another server is using") {
            StartupFailure::DataLocked(text)
        } else if busy {
            StartupFailure::GamePortBusy(text)
        } else {
            StartupFailure::Other(text)
        }
    }
}

impl fmt::Display for StartupFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StartupFailure::GamePortBusy(t)
            | StartupFailure::WebPortBusy(t)
            | StartupFailure::DataLocked(t)
            | StartupFailure::Other(t) => f.write_str(t),
        }
    }
}

#[derive(Debug)]
pub enum HostError {
    /// A lifecycle call from the thread that already holds the lifecycle lock.
    Reentrant,
    NotInstalled,
    NoJava,
    /// `config/server.yml` is not in the form [`ServerHost::configure`] writes (the engine's shipped file, for one).
    NotConfigured,
    PortConflict(Vec<Conflict>),
    Spawn(io::Error),
    /// A `stop` arrived while the start was waiting.
    Aborted,
    /// The process ended before it was ready.
    EarlyExit {
        code: Option<i32>,
        tail: Vec<String>,
    },
    StartupFailed(StartupFailure),
    StartTimeout,
    /// An operation that needs the server running.
    NotRunning,
    /// The console refused or did not answer.
    Console(String),
    Config(ConfigError),
    Engine(EngineError),
    Io(io::Error),
}

impl fmt::Display for HostError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HostError::Reentrant => write!(
                f,
                "a server lifecycle call was made from inside another one"
            ),
            HostError::NotInstalled => write!(f, "the server engine is not installed"),
            HostError::NoJava => write!(f, "the Java runtime for the server was not found"),
            HostError::NotConfigured => write!(f, "the server is not configured for the WebUI yet"),
            HostError::PortConflict(list) => {
                for (i, c) in list.iter().enumerate() {
                    if i > 0 {
                        write!(f, "; ")?;
                    }
                    let which = match c.which {
                        ports::PortUse::Game => "game",
                        ports::PortUse::Web => "web",
                    };
                    if c.same_as_other {
                        write!(
                            f,
                            "the {which} port {} is also the other listener's port",
                            c.port
                        )?;
                    } else {
                        write!(f, "the {which} port {} is in use", c.port)?;
                    }
                    if let Some(s) = c.suggestion {
                        write!(f, " (try {s})")?;
                    }
                }
                Ok(())
            }
            HostError::Spawn(e) => write!(f, "could not start the server process: {e}"),
            HostError::Aborted => write!(f, "the start was cancelled"),
            HostError::EarlyExit { code, .. } => match code {
                Some(c) => write!(f, "the server process ended during startup (exit code {c})"),
                None => write!(f, "the server process ended during startup"),
            },
            HostError::StartupFailed(why) => write!(f, "the server failed to start: {why}"),
            HostError::StartTimeout => write!(f, "the server did not become ready in time"),
            HostError::NotRunning => write!(f, "the server is not running"),
            HostError::Console(why) => write!(f, "the server console: {why}"),
            HostError::Config(e) => write!(f, "{e}"),
            HostError::Engine(e) => write!(f, "{e}"),
            HostError::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for HostError {}

impl From<io::Error> for HostError {
    fn from(e: io::Error) -> Self {
        HostError::Io(e)
    }
}
impl From<ConfigError> for HostError {
    fn from(e: ConfigError) -> Self {
        HostError::Config(e)
    }
}
impl From<EngineError> for HostError {
    fn from(e: EngineError) -> Self {
        HostError::Engine(e)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Phase {
    Stopped,
    Starting,
    Running,
    Stopping,
    /// The last run ended badly: it never became ready, or it ended while it should have been running.
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdminState {
    /// Not attempted yet (or not managed).
    Unknown,
    /// The `skradle` administrator exists with the stored password.
    Ready,
    Failed(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebState {
    Unknown,
    /// The probe got `200`.
    Ready,
    /// The listener answers but refused the probe (this status): the configured `webui-public-url` differs.
    Refused(u16),
    NotAnswering,
}

/// How a stop ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopKind {
    NotRunning,
    /// The confirmed `shutdown` ended the process.
    Graceful,
    /// SIGTERM was needed.
    Terminated,
    /// The process had to be killed.
    Killed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    pub phase: Phase,
    pub game_ready: bool,
    /// The `address:port` the engine reported for the game listener.
    pub listening: Option<String>,
    pub web: WebState,
    pub admin: AdminState,
    pub pid: Option<u32>,
    pub exit_code: Option<i32>,
    /// The URL to open for administration while the server runs.
    pub web_ui_url: Option<String>,
}

#[derive(Debug)]
struct State {
    phase: Phase,
    log: LogBuffer,
    game_ready: bool,
    listening: Option<String>,
    failure: Option<StartupFailure>,
    exit: Option<Option<i32>>,
    web: WebState,
    admin: AdminState,
    script: Option<ActiveScript>,
    readers_open: u8,
    pid: Option<u32>,
}

#[derive(Debug)]
struct ActiveScript {
    script: PromptScript,
    outcome: Option<Outcome>,
}

/// One run of the child process and everything attached to it.
struct Run {
    state: Mutex<State>,
    changed: Condvar,
    child: Mutex<Option<Child>>,
    stdin: Mutex<Option<ChildStdin>>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

impl Run {
    fn new(capacity: usize) -> Self {
        Run {
            state: Mutex::new(State {
                phase: Phase::Starting,
                log: LogBuffer::new(capacity),
                game_ready: false,
                listening: None,
                failure: None,
                exit: None,
                web: WebState::Unknown,
                admin: AdminState::Unknown,
                script: None,
                readers_open: 0,
                pid: None,
            }),
            changed: Condvar::new(),
            child: Mutex::new(None),
            stdin: Mutex::new(None),
        }
    }

    fn send_line(&self, text: &str) -> io::Result<()> {
        let mut guard = lock(&self.stdin);
        let stdin = guard
            .as_mut()
            .ok_or_else(|| io::Error::new(io::ErrorKind::BrokenPipe, "no console"))?;
        stdin.write_all(text.as_bytes())?;
        stdin.write_all(b"\n")?;
        stdin.flush()
    }

    /// Notes the child's exit if it has happened.
    ///
    /// Lock order: the `child` and `state` locks are never held together (this reads the child, lets go, then
    /// takes the state), so no other thread can wait on them in the opposite order.
    fn refresh_exit(&self) {
        let exited = {
            let mut child = lock(&self.child);
            child
                .as_mut()
                .and_then(|c| c.try_wait().ok().flatten())
                .map(|status| status.code())
        };
        if let Some(code) = exited {
            let mut state = lock(&self.state);
            if state.exit.is_none() {
                state.exit = Some(code);
            }
            self.changed.notify_all();
        }
    }

    /// Whether a child is held that has not been seen to exit.
    fn has_live_child(&self) -> bool {
        self.refresh_exit();
        let has_child = lock(&self.child).is_some();
        let exited = lock(&self.state).exit.is_some();
        has_child && !exited
    }

    /// Waits until `done` is true for the state or the timeout passes, checking for the child's exit every 25 ms.
    fn wait_for(&self, timeout: Duration, mut done: impl FnMut(&mut State) -> bool) -> bool {
        let deadline = Instant::now() + timeout;
        loop {
            self.refresh_exit();
            let mut state = lock(&self.state);
            if done(&mut state) {
                return true;
            }
            let now = Instant::now();
            if now >= deadline {
                return false;
            }
            let wait = (deadline - now).min(Duration::from_millis(25));
            let _ = self.changed.wait_timeout(state, wait);
        }
    }

    fn handle_line(&self, raw: String) {
        let line = redact(raw);
        let mut reply = None;
        {
            let mut state = lock(&self.state);
            state.log.push(line.clone());
            if line.contains("Done! Listening on ") && line.contains("(TCP)") {
                state.game_ready = true;
                state.listening = line
                    .split("Listening on ")
                    .nth(1)
                    .and_then(|r| r.split(" (").next())
                    .map(str::to_owned);
            }
            if let Some((_, rest)) = line.split_once("Server startup failed:") {
                state.failure = Some(StartupFailure::classify(rest));
            } else if line.contains("Cannot bind shared HTTP listener")
                || line.contains("Another server is using this JAR's data directory")
            {
                state.failure = Some(StartupFailure::classify(&line));
            }
            if let Some(active) = state.script.as_mut() {
                let step = active.script.on_line(&line);
                reply = step.reply;
                if let Some(outcome) = step.outcome {
                    active.outcome = Some(outcome);
                }
            }
            self.changed.notify_all();
        }
        if let Some(secret) = reply {
            let _ = self.send_line(secret.expose());
        }
    }

    fn reader_done(&self) {
        let mut state = lock(&self.state);
        state.readers_open = state.readers_open.saturating_sub(1);
        self.changed.notify_all();
    }

    /// Runs a console exchange: sends the command and answers its prompts as they come. Needs a running process.
    fn run_script(
        &self,
        script: PromptScript,
        timeout: Duration,
        abort: &AtomicBool,
    ) -> Result<Outcome, HostError> {
        let command = script.command().to_owned();
        {
            let mut state = lock(&self.state);
            if state.exit.is_some() {
                return Err(HostError::NotRunning);
            }
            state.script = Some(ActiveScript {
                script,
                outcome: None,
            });
        }
        let result = match self.send_line(&command) {
            Err(_) => Err(HostError::NotRunning),
            Ok(()) => {
                let mut outcome = None;
                let finished = self.wait_for(timeout, |s| {
                    outcome = s.script.as_ref().and_then(|a| a.outcome.clone());
                    outcome.is_some() || s.exit.is_some() || abort.load(Ordering::SeqCst)
                });
                match outcome {
                    Some(outcome) => Ok(outcome),
                    None if abort.load(Ordering::SeqCst) => Err(HostError::Aborted),
                    None if finished => Err(HostError::NotRunning),
                    None => Err(HostError::Console("no answer in time".into())),
                }
            }
        };
        lock(&self.state).script = None;
        result
    }
}

/// An abort flag that is never set, for steps that must finish even while a stop is under way.
static NEVER: AtomicBool = AtomicBool::new(false);

/// Hides what must not be kept: the first-run WebUI setup code.
fn redact(line: String) -> String {
    const MARK: &str = "WebUI first-run setup code:";
    match line.find(MARK) {
        Some(at) => format!("{}{MARK} [hidden]", &line[..at]),
        None => line,
    }
}

/// Reads lines (lossy UTF-8, long lines cut) until the stream ends.
fn read_lines(stream: impl Read, mut on_line: impl FnMut(String)) {
    let mut reader = io::BufReader::new(stream);
    let mut buf = Vec::new();
    loop {
        buf.clear();
        let mut limited = reader.by_ref().take(MAX_LINE_BYTES as u64);
        match limited.read_until(b'\n', &mut buf) {
            Ok(0) | Err(_) => return,
            Ok(_) => {}
        }
        if buf.last() != Some(&b'\n') && buf.len() >= MAX_LINE_BYTES {
            // Too long: keep the start, drop the rest of the line without holding it.
            skip_line(&mut reader);
        }
        let text = String::from_utf8_lossy(&buf);
        on_line(text.trim_end_matches(['\r', '\n']).to_owned());
    }
}

/// Consumes input up to and including the next newline (or the end) without keeping any of it.
fn skip_line(reader: &mut impl BufRead) {
    loop {
        let (done, used) = match reader.fill_buf() {
            Ok([]) | Err(_) => return,
            Ok(chunk) => match chunk.iter().position(|&b| b == b'\n') {
                Some(at) => (true, at + 1),
                None => (false, chunk.len()),
            },
        };
        reader.consume(used);
        if done {
            return;
        }
    }
}

/// Ends the lifecycle lock's ownership record when dropped.
struct LifecycleGuard<'a> {
    host: &'a ServerHost,
    _lock: MutexGuard<'a, ()>,
}

impl Drop for LifecycleGuard<'_> {
    fn drop(&mut self) {
        *lock(&self.host.owner) = None;
    }
}

/// The result of an operation that needed the server stopped.
#[derive(Debug)]
pub struct WhileStopped<T> {
    /// The server was running and was stopped for this; the caller decides whether to start it again.
    pub was_running: bool,
    pub value: T,
}

/// What an engine update did.
#[derive(Debug)]
pub struct UpdateReport {
    pub outcome: UpdateOutcome,
    pub was_running: bool,
    pub backup: Option<BackupInfo>,
}

/// Manages one server folder. Cheap to share between threads (wrap it in an `Arc`).
pub struct ServerHost {
    layout: ServerLayout,
    options: HostOptions,
    lifecycle: Mutex<()>,
    owner: Mutex<Option<ThreadId>>,
    abort: AtomicBool,
    run: Mutex<Option<Arc<Run>>>,
}

impl ServerHost {
    pub fn new(layout: ServerLayout, options: HostOptions) -> Self {
        Self {
            layout,
            options,
            lifecycle: Mutex::new(()),
            owner: Mutex::new(None),
            abort: AtomicBool::new(false),
            run: Mutex::new(None),
        }
    }

    pub fn layout(&self) -> &ServerLayout {
        &self.layout
    }

    fn lifecycle(&self) -> Result<LifecycleGuard<'_>, HostError> {
        let me = thread::current().id();
        if *lock(&self.owner) == Some(me) {
            return Err(HostError::Reentrant);
        }
        let guard = lock(&self.lifecycle);
        *lock(&self.owner) = Some(me);
        Ok(LifecycleGuard {
            host: self,
            _lock: guard,
        })
    }

    /// True while some lifecycle call is in progress.
    pub fn is_busy(&self) -> bool {
        // Only a held lock means busy; a lock poisoned by a panicked call is free again.
        matches!(
            self.lifecycle.try_lock(),
            Err(std::sync::TryLockError::WouldBlock)
        )
    }

    fn current(&self) -> Option<Arc<Run>> {
        lock(&self.run).clone()
    }

    /// The live state. Never waits for the lifecycle lock.
    pub fn status(&self) -> Status {
        let Some(run) = self.current() else {
            return Status {
                phase: Phase::Stopped,
                game_ready: false,
                listening: None,
                web: WebState::Unknown,
                admin: AdminState::Unknown,
                pid: None,
                exit_code: None,
                web_ui_url: None,
            };
        };
        run.refresh_exit();
        // Read the settings before taking the state lock: the output readers wait on it.
        let web_ui_url = config::read_file(&self.layout)
            .ok()
            .map(|c| c.host_config().web_ui_url());
        let mut state = lock(&run.state);
        if let Some(code) = state.exit {
            match state.phase {
                Phase::Running => {
                    state.phase = Phase::Failed(match code {
                        Some(c) => format!("the server ended unexpectedly (exit code {c})"),
                        None => "the server ended unexpectedly".to_owned(),
                    })
                }
                Phase::Stopping => state.phase = Phase::Stopped,
                _ => {}
            }
        }
        let web_ui_url = web_ui_url.filter(|_| state.phase == Phase::Running);
        Status {
            phase: state.phase.clone(),
            game_ready: state.game_ready && state.exit.is_none(),
            listening: state.listening.clone(),
            web: state.web,
            admin: state.admin.clone(),
            pid: if state.exit.is_none() {
                state.pid
            } else {
                None
            },
            exit_code: state.exit.flatten(),
            web_ui_url,
        }
    }

    /// The output lines from `cursor` on (0 for everything buffered) and the next cursor.
    pub fn log_since(&self, cursor: u64) -> (Vec<String>, u64) {
        match self.current() {
            Some(run) => lock(&run.state).log.since(cursor),
            None => (Vec::new(), 0),
        }
    }

    /// Writes the launcher's web and port settings into `config/server.yml` (see [`super::config`]). Refused while
    /// the server runs, because the engine only reads the file at startup.
    pub fn configure(&self, config: &HostConfig) -> Result<(), HostError> {
        let _guard = self.lifecycle()?;
        if self.is_alive() {
            return Err(HostError::Config(ConfigError::BadPort(
                "stop the server before changing its settings",
            )));
        }
        self.layout.ensure()?;
        config.write(&self.layout)?;
        Ok(())
    }

    fn is_alive(&self) -> bool {
        self.current().is_some_and(|run| run.has_live_child())
    }

    /// Starts the server and returns when the game listener is ready, the web listener has been probed and (unless
    /// disabled) the administrator account has been applied. A failed start leaves no process running.
    pub fn start(&self) -> Result<Status, HostError> {
        let _guard = self.lifecycle()?;
        self.start_locked()?;
        Ok(self.status())
    }

    pub fn stop(&self) -> Result<StopKind, HostError> {
        // A call from inside a lifecycle call is refused before it can touch the flag: setting and clearing it
        // here would wipe an abort that another thread's stop has requested.
        if *lock(&self.owner) == Some(thread::current().id()) {
            return Err(HostError::Reentrant);
        }
        // A start that is waiting must not make this wait for its whole timeout.
        self.abort.store(true, Ordering::SeqCst);
        let guard = self.lifecycle();
        let result = guard.and_then(|_g| self.stop_locked());
        self.abort.store(false, Ordering::SeqCst);
        result
    }

    pub fn restart(&self) -> Result<Status, HostError> {
        let _guard = self.lifecycle()?;
        self.stop_locked()?;
        self.start_locked()?;
        Ok(self.status())
    }

    fn start_locked(&self) -> Result<(), HostError> {
        self.abort.store(false, Ordering::SeqCst);
        if self.is_alive() {
            return Ok(());
        }
        if !engine::is_ready(&self.layout) {
            return Err(HostError::NotInstalled);
        }
        if !self.options.java.is_file() {
            return Err(HostError::NoJava);
        }
        let current = config::read_file(&self.layout)?;
        let host_config = current.host_config();
        // The engine's shipped file has the WebUI on but binds every interface in "automatic" Host mode; only a file
        // written by `configure` (the web origin pinned to loopback, plain HTTP) is started.
        if !current.is_launcher_configured() {
            return Err(HostError::NotConfigured);
        }
        if self.options.check_ports {
            let conflicts = ports::check(&host_config);
            if !conflicts.is_empty() {
                return Err(HostError::PortConflict(conflicts));
            }
        }

        // Asked before the engine starts, because the engine creates its account database.
        let engine_had_accounts = admin::engine_has_accounts(&self.layout);
        let spec = command_spec(&self.layout, &self.options);
        let mut command = spec.to_command();
        command
            .env_remove("_JAVA_OPTIONS")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        // A background server must not open a console window of its own.
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
        let mut child = command.spawn().map_err(HostError::Spawn)?;
        let run = Arc::new(Run::new(self.options.log_capacity));
        {
            let mut state = lock(&run.state);
            state.pid = Some(child.id());
            state.readers_open = 2;
        }
        *lock(&run.stdin) = child.stdin.take();
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        *lock(&run.child) = Some(child);
        *lock(&self.run) = Some(run.clone());
        for stream in [
            stdout.map(|s| Box::new(s) as Box<dyn Read + Send>),
            stderr.map(|s| Box::new(s) as Box<dyn Read + Send>),
        ] {
            let reader_run = run.clone();
            let spawned = thread::Builder::new()
                .name("server-output".into())
                .spawn(move || {
                    if let Some(stream) = stream {
                        read_lines(stream, |line| reader_run.handle_line(line));
                    }
                    reader_run.reader_done();
                });
            if let Err(e) = spawned {
                // Without its readers the engine would run unobserved (and block on a full pipe): end it and
                // forget the run, so nothing mistakes it for a started server.
                if let Some(child) = lock(&run.child).as_mut() {
                    let _ = child.kill();
                    let _ = child.wait();
                }
                *lock(&self.run) = None;
                return Err(HostError::Spawn(e));
            }
        }

        let failed = |this: &Self, run: &Arc<Run>, error: HostError| {
            match &error {
                // The engine's console is alive and idle after a startup failure: ask it to end.
                HostError::StartupFailed(_) => {
                    let _ = this.end_process(run);
                }
                // Cancelled or never ready: no point waiting politely.
                _ => {
                    if let Some(child) = lock(&run.child).as_mut() {
                        let _ = child.kill();
                    }
                    run.wait_for(Duration::from_secs(5), |s| s.exit.is_some());
                    let _ = this.reap(run);
                }
            }
            lock(&run.state).phase = match &error {
                HostError::Aborted => Phase::Stopped,
                other => Phase::Failed(other.to_string()),
            };
            Err(error)
        };

        // Wait for the game listener, a startup failure, an exit, an abort or the timeout.
        let abort = &self.abort;
        let mut exit_seen: Option<Instant> = None;
        let outcome = run.wait_for(self.options.start_timeout, |s| {
            if abort.load(Ordering::SeqCst) || s.game_ready {
                return true;
            }
            if s.exit.is_some() {
                // Let the output reader catch up so the engine's own explanation is not missed.
                let since = *exit_seen.get_or_insert_with(Instant::now);
                return s.readers_open == 0 || since.elapsed() > Duration::from_secs(1);
            }
            s.failure.is_some()
        });
        let (ready, failure, exit, tail) = {
            let s = lock(&run.state);
            let (tail, _) = s.log.since(s.log.since(0).1.saturating_sub(12));
            (s.game_ready, s.failure.clone(), s.exit, tail)
        };
        if self.abort.load(Ordering::SeqCst) {
            return failed(self, &run, HostError::Aborted);
        }
        if let Some(failure) = failure {
            return failed(self, &run, HostError::StartupFailed(failure));
        }
        if let Some(code) = exit {
            return failed(self, &run, HostError::EarlyExit { code, tail });
        }
        if !outcome || !ready {
            return failed(self, &run, HostError::StartTimeout);
        }

        // The web listener opens before the game listener; give it a moment anyway.
        let web = self.wait_for_web(host_config.web_port, &run);
        lock(&run.state).web = web;
        if let Some(error) = self.interrupted(&run) {
            return failed(self, &run, error);
        }

        if self.options.manage_admin {
            // Only when nothing proves the engine has the account: `webui user` would replace a password the
            // player changed in the WebUI and sign them out.
            let admin_state = if admin::needs_apply(&self.layout, engine_had_accounts) {
                match self.apply_admin(&run, false) {
                    Ok(()) => AdminState::Ready,
                    Err(e) => AdminState::Failed(e.to_string()),
                }
            } else {
                AdminState::Ready
            };
            lock(&run.state).admin = admin_state;
            if let Some(error) = self.interrupted(&run) {
                return failed(self, &run, error);
            }
        }
        lock(&run.state).phase = Phase::Running;
        Ok(())
    }

    /// Whether a stop was requested or the child exited while a start was still working: the start has failed even
    /// if the earlier steps went well.
    fn interrupted(&self, run: &Arc<Run>) -> Option<HostError> {
        run.refresh_exit();
        if self.abort.load(Ordering::SeqCst) {
            return Some(HostError::Aborted);
        }
        let s = lock(&run.state);
        let code = s.exit?;
        let (tail, _) = s.log.since(s.log.since(0).1.saturating_sub(12));
        Some(HostError::EarlyExit { code, tail })
    }

    fn wait_for_web(&self, port: u16, run: &Arc<Run>) -> WebState {
        let deadline = Instant::now() + Duration::from_secs(10).min(self.options.start_timeout);
        loop {
            match ports::probe_web(port, Duration::from_millis(500)) {
                WebProbe::Answers(200) => return WebState::Ready,
                WebProbe::Answers(other) => return WebState::Refused(other),
                WebProbe::NotListening => {}
            }
            run.refresh_exit();
            // An abort ends the wait at once; the caller then sees it and fails the start as aborted.
            if Instant::now() >= deadline
                || self.abort.load(Ordering::SeqCst)
                || lock(&run.state).exit.is_some()
            {
                return WebState::NotAnswering;
            }
            thread::sleep(Duration::from_millis(100));
        }
    }

    /// Creates the administrator with `webui user`; with `reset` a new password goes through `webui password` first.
    fn apply_admin(&self, run: &Arc<Run>, reset: bool) -> Result<(), HostError> {
        let credentials: Credentials = if reset {
            let fresh = Credentials::new(admin::ADMIN_USER, admin::generate_password()?);
            admin::store(&self.layout, &fresh)?;
            fresh
        } else {
            admin::ensure(&self.layout)?
        };
        let timeout = self.options.prompt_timeout;
        let secret = credentials.secret();
        if reset {
            match run.run_script(
                PromptScript::webui_password(&credentials.username, &secret),
                timeout,
                &self.abort,
            )? {
                Outcome::Done => return Ok(admin::mark_applied(&self.layout)?),
                // The account is missing (a reset of the database): create it instead.
                Outcome::Refused(_) => {}
            }
        }
        match run.run_script(
            PromptScript::webui_user(&credentials.username, &secret),
            timeout,
            &self.abort,
        )? {
            Outcome::Done => Ok(admin::mark_applied(&self.layout)?),
            Outcome::Refused(why) => Err(HostError::Console(why)),
        }
    }

    /// Replaces the administrator's password with a new generated one (`webui password`, falling back to
    /// `webui user` when the account is gone) and returns the new credentials. The server must be running.
    pub fn reset_admin_password(&self) -> Result<Credentials, HostError> {
        let _guard = self.lifecycle()?;
        let run = self
            .current()
            .filter(|_| self.is_alive())
            .ok_or(HostError::NotRunning)?;
        self.apply_admin(&run, true)?;
        lock(&run.state).admin = AdminState::Ready;
        admin::load(&self.layout)
            .ok_or_else(|| HostError::Io(io::Error::other("credentials missing")))
    }

    /// The stored administrator credentials, to show the player.
    pub fn admin_credentials(&self) -> Option<Credentials> {
        admin::load(&self.layout)
    }

    fn stop_locked(&self) -> Result<StopKind, HostError> {
        let Some(run) = self.current() else {
            return Ok(StopKind::NotRunning);
        };
        if !run.has_live_child() {
            let mut state = lock(&run.state);
            if state.phase != Phase::Stopped && !matches!(state.phase, Phase::Failed(_)) {
                state.phase = Phase::Stopped;
            }
            drop(state);
            let _ = self.reap(&run);
            return Ok(StopKind::NotRunning);
        }
        lock(&run.state).phase = Phase::Stopping;
        let kind = self.end_process(&run)?;
        lock(&run.state).phase = Phase::Stopped;
        Ok(kind)
    }

    /// The graceful sequence: confirmed shutdown, SIGTERM, kill.
    fn end_process(&self, run: &Arc<Run>) -> Result<StopKind, HostError> {
        let timeout = self.options.shutdown_timeout;
        let exited = |run: &Arc<Run>, wait: Duration| run.wait_for(wait, |s| s.exit.is_some());
        if lock(&run.state).exit.is_some() {
            self.reap(run)?;
            return Ok(StopKind::NotRunning);
        }
        // 1. The confirmed shutdown. A refusal or a missing answer just moves on to the next step.
        // Not abortable: a stop sets the abort flag, and this is the stop itself.
        let _ = run.run_script(
            PromptScript::shutdown(),
            self.options.prompt_timeout.min(timeout),
            &NEVER,
        );
        if exited(run, timeout) {
            self.reap(run)?;
            return Ok(StopKind::Graceful);
        }
        // 2. SIGTERM (unix): runs the engine's shutdown hook.
        if self.terminate(run) && exited(run, timeout) {
            self.reap(run)?;
            return Ok(StopKind::Terminated);
        }
        // 3. Kill.
        if let Some(child) = lock(&run.child).as_mut() {
            let _ = child.kill();
        }
        exited(run, Duration::from_secs(5));
        self.reap(run)?;
        Ok(StopKind::Killed)
    }

    #[cfg(unix)]
    fn terminate(&self, run: &Arc<Run>) -> bool {
        // The child stays locked while it is signalled: once it has been collected its number may belong to another
        // process, and only a child that has not exited yet may be signalled.
        let mut child = lock(&run.child);
        let Some(child) = child.as_mut() else {
            return false;
        };
        if !matches!(child.try_wait(), Ok(None)) {
            return false;
        }
        let pid = child.id();
        Command::new("kill")
            .args(["-TERM", &pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    }

    #[cfg(not(unix))]
    fn terminate(&self, _run: &Arc<Run>) -> bool {
        false
    }

    /// Collects the exited child and closes its stdin.
    fn reap(&self, run: &Arc<Run>) -> io::Result<()> {
        *lock(&run.stdin) = None;
        // Take the child out first: waiting must not hold the lock the status readers use.
        let taken = lock(&run.child).take();
        if let Some(mut child) = taken {
            let status = child.wait()?;
            let mut state = lock(&run.state);
            if state.exit.is_none() {
                state.exit = Some(status.code());
            }
            run.changed.notify_all();
        }
        // The output readers end on their own when the pipes close; give them a moment so the log is complete.
        run.wait_for(Duration::from_secs(1), |s| s.readers_open == 0);
        Ok(())
    }

    /// Runs `operation` with the server stopped and the lifecycle lock held, stopping a running server first. The
    /// operation gets the proof the backup functions require. It must not call this host's lifecycle methods (they
    /// fail with [`HostError::Reentrant`]).
    pub fn run_stopped<T>(
        &self,
        operation: impl FnOnce(&ServerLayout, &Stopped<'_>) -> Result<T, HostError>,
    ) -> Result<WhileStopped<T>, HostError> {
        let _guard = self.lifecycle()?;
        let was_running = self.is_alive();
        self.stop_locked()?;
        let proof = Stopped::assume_stopped();
        let value = operation(&self.layout, &proof)?;
        Ok(WhileStopped { was_running, value })
    }

    pub fn backup(&self, label: Option<&str>) -> Result<WhileStopped<BackupInfo>, HostError> {
        self.run_stopped(|layout, proof| {
            Ok(backups::create(layout, proof, label, terms::now_secs())?)
        })
    }

    pub fn restore(&self, name: &str) -> Result<WhileStopped<()>, HostError> {
        self.run_stopped(|layout, proof| {
            Ok(backups::restore(layout, proof, name, terms::now_secs())?)
        })
    }

    pub fn reset(&self, keep_keys: bool) -> Result<WhileStopped<BackupInfo>, HostError> {
        self.run_stopped(|layout, proof| {
            Ok(backups::reset(layout, proof, keep_keys, terms::now_secs())?)
        })
    }

    /// Installs the newest engine release (with the licence accepted), after backing up `data/` and `config/`: no
    /// backup, no update. The server is stopped for it; restart it afterwards.
    pub fn update_engine(
        &self,
        client: &Client,
        progress: &mut dyn FnMut(Stage),
    ) -> Result<UpdateReport, HostError> {
        // Looking is free: only an update that will really be installed stops the server and takes a backup (which
        // also prunes the oldest). The licence and the version are checked first.
        let release = match engine::check_latest(client, &self.layout)? {
            engine::Latest::UpToDate(version) => {
                return Ok(UpdateReport {
                    outcome: engine::UpdateOutcome::UpToDate(version),
                    backup: None,
                    was_running: self.is_alive(),
                });
            }
            engine::Latest::Newer(release) => release,
        };
        self.run_stopped(|layout, proof| {
            let first_install = !layout.server_yml().exists();
            let backup =
                if layout.data_dir().exists() || layout.config_dir().join("server.yml").exists() {
                    Some(backups::create(
                        layout,
                        proof,
                        Some("before-update"),
                        terms::now_secs(),
                    )?)
                } else {
                    None
                };
            let installed = engine::install_release(client, layout, &release, progress)?;
            finish_install(layout, first_install)?;
            Ok((engine::UpdateOutcome::Installed(installed), backup))
        })
        .map(|done| UpdateReport {
            outcome: done.value.0,
            backup: done.value.1,
            was_running: done.was_running,
        })
    }

    /// Puts the previous engine back (server stopped for it).
    pub fn rollback_engine(&self) -> Result<WhileStopped<engine::InstalledEngine>, HostError> {
        self.run_stopped(|layout, _| Ok(engine::rollback_to_previous(layout)?))
    }
}

/// After an install: a server folder that had no `server.yml` before gets the launcher's own (loopback-only
/// defaults) instead of the engine's shipped file, which binds every interface in "automatic" Host mode.
fn finish_install(layout: &ServerLayout, first_install: bool) -> Result<(), HostError> {
    if first_install {
        HostConfig::default().write(layout)?;
    }
    Ok(())
}

impl Drop for ServerHost {
    /// Dropping the host does not leave the engine running: it is killed (no graceful wait; call `stop` first).
    fn drop(&mut self) {
        if let Some(run) = self.current() {
            if let Some(child) = lock(&run.child).as_mut() {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::JavaRelease;

    fn release(text: &str) -> JavaRelease {
        JavaRelease::parse(text).unwrap()
    }

    #[test]
    fn the_command_line_is_the_one_the_engine_documents() {
        let layout = ServerLayout::new("/srv/sk");
        let spec = command_spec(&layout, &HostOptions::new("/jre/bin/java"));
        assert_eq!(spec.program, PathBuf::from("/jre/bin/java"));
        assert_eq!(spec.cwd, PathBuf::from("/srv/sk"));
        let args: Vec<_> = spec.args.iter().map(|a| a.to_str().unwrap()).collect();
        assert_eq!(
            args,
            [
                "-Djava.awt.headless=true",
                "--enable-native-access=ALL-UNNAMED",
                "-Dstdout.encoding=UTF-8",
                "-Dstderr.encoding=UTF-8",
                "-jar",
                "tortodrone.jar",
                "--config",
                "config/server.yml",
                "--skip-client-assets",
            ]
        );
    }

    #[test]
    fn extra_jvm_arguments_come_before_the_jar_and_the_game_folder_replaces_the_skip() {
        let mut options = HostOptions::new("java");
        options.jvm_args = vec!["-Xmx1g".into()];
        options.client_assets = ClientAssets::Game(PathBuf::from("/games/Spiral Knights"));
        let spec = command_spec(&ServerLayout::new("/s"), &options);
        let args: Vec<_> = spec.args.iter().map(|a| a.to_str().unwrap()).collect();
        let jar = args.iter().position(|a| *a == "-jar").unwrap();
        assert_eq!(args[jar - 1], "-Xmx1g");
        assert_eq!(
            &args[args.len() - 2..],
            ["--client", "/games/Spiral Knights"]
        );
        assert!(!args.contains(&"--skip-client-assets"));
    }

    #[test]
    fn a_runtime_needs_java_25_and_the_modules_the_engine_imports() {
        let full = "JAVA_VERSION=\"25.0.3\"\nMODULES=\"java.base java.sql jdk.httpserver java.management java.logging java.desktop\"\n";
        assert!(supports_server(&release(full)));
        // No module list: a full runtime.
        assert!(supports_server(&release("JAVA_VERSION=\"25.0.1\"\n")));
        let game = "JAVA_VERSION=\"25.0.1\"\nMODULES=\"java.base java.desktop java.logging jdk.unsupported\"\n";
        assert!(
            !supports_server(&release(game)),
            "the game's trimmed runtime has no java.sql"
        );
        assert!(!supports_server(&release(
            "JAVA_VERSION=\"21.0.5\"\nMODULES=\"java.base java.sql jdk.httpserver java.management java.logging\"\n"
        )));
    }

    #[test]
    fn the_log_buffer_is_bounded_and_cursors_return_only_new_lines() {
        let mut log = LogBuffer::new(3);
        for n in 0..5 {
            log.push(format!("line {n}"));
        }
        assert_eq!(log.len(), 3);
        let (lines, cursor) = log.since(0);
        assert_eq!(lines, ["line 2", "line 3", "line 4"]);
        assert_eq!(cursor, 5);
        log.push("line 5".into());
        assert_eq!(log.since(cursor), (vec!["line 5".to_owned()], 6));
        assert_eq!(log.since(6).0, Vec::<String>::new());
    }

    #[test]
    fn the_setup_code_never_reaches_the_buffer() {
        assert_eq!(
            redact("[10:00:01] [Main/INFO]: WebUI first-run setup code: 12345678".into()),
            "[10:00:01] [Main/INFO]: WebUI first-run setup code: [hidden]"
        );
        assert_eq!(redact("nothing here".into()), "nothing here");
    }

    #[test]
    fn startup_failures_are_told_apart() {
        use StartupFailure::*;
        assert!(matches!(
            StartupFailure::classify(" Address already in use (Bind failed)"),
            GamePortBusy(_)
        ));
        assert!(matches!(
            StartupFailure::classify(
                " Cannot bind shared HTTP listener on 0.0.0.0:8080: Address already in use"
            ),
            WebPortBusy(_)
        ));
        assert!(matches!(
            StartupFailure::classify(" Another server is using this JAR's data directory"),
            DataLocked(_)
        ));
        assert!(matches!(
            StartupFailure::classify(" Unsupported database schema 99"),
            Other(_)
        ));
    }

    #[test]
    fn long_lines_are_cut_and_invalid_utf8_is_replaced() {
        let mut data = vec![b'a'; MAX_LINE_BYTES * 2 + 5];
        data.push(b'\n');
        data.extend_from_slice(b"ok \xff end\r\n");
        let mut lines = Vec::new();
        read_lines(&data[..], |l| lines.push(l));
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].len(), MAX_LINE_BYTES);
        assert_eq!(lines[1], "ok \u{fffd} end");
    }

    #[test]
    fn the_rest_of_a_huge_line_is_skipped_in_chunks_and_the_next_line_survives() {
        // Several buffer fills of one endless line, then an ordinary line, one ending exactly at the cut, and a
        // final line without a newline.
        let mut data = vec![b'x'; 3 * 1024 * 1024];
        data.extend_from_slice(b"\nafter\n");
        data.extend(std::iter::repeat_n(b'y', MAX_LINE_BYTES - 1));
        data.extend_from_slice(b"\nnext\ntail");
        let mut lines = Vec::new();
        read_lines(&data[..], |l| lines.push(l));
        assert_eq!(
            lines.len(),
            5,
            "{:?}",
            lines.iter().map(String::len).collect::<Vec<_>>()
        );
        assert_eq!(lines[0].len(), MAX_LINE_BYTES);
        assert_eq!(lines[1], "after");
        assert_eq!(lines[2].len(), MAX_LINE_BYTES - 1);
        assert_eq!(lines[3], "next");
        assert_eq!(lines[4], "tail");
        let mut more = Vec::new();
        read_lines(&b"a\nb"[..], |l| more.push(l));
        assert_eq!(more, ["a", "b"]);
    }

    #[test]
    fn a_first_install_replaces_the_shipped_config_and_a_later_one_leaves_the_players_alone() {
        use super::super::engine::testkit;
        let dir = tempfile::tempdir().unwrap();
        let layout = testkit::accepted_layout(dir.path());
        testkit::install(&layout, "0.40.17", &[]);
        // As installed, the engine's shipped file is not startable by the launcher.
        assert!(!config::read_file(&layout).unwrap().is_launcher_configured());
        finish_install(&layout, true).unwrap();
        let now = config::read_file(&layout).unwrap();
        assert!(now.is_launcher_configured());
        assert_eq!(now.host_config(), HostConfig::default());
        // A LAN choice made later survives an update.
        let lan = HostConfig {
            lan: true,
            ..HostConfig::default()
        };
        lan.write(&layout).unwrap();
        finish_install(&layout, false).unwrap();
        assert_eq!(config::read_file(&layout).unwrap().host_config(), lan);
    }

    #[test]
    fn conflicts_read_clearly() {
        let error = HostError::PortConflict(vec![Conflict {
            which: ports::PortUse::Web,
            port: 8080,
            same_as_other: false,
            suggestion: Some(8081),
        }]);
        assert_eq!(error.to_string(), "the web port 8080 is in use (try 8081)");
    }
}

#[cfg(all(test, unix))]
mod fake_java_tests {
    //! The process manager against shell scripts standing in for `java`. The scripts print what the engine's
    //! source says it prints; they are not recordings of a real engine.
    use super::*;
    use std::net::TcpListener;
    use std::os::unix::fs::PermissionsExt;

    const BANNER: &str = r#"
echo "[10:00:00] [Main/INFO]: Starting Tortodrone server version 0.40.17"
echo "[10:00:00] [Main/INFO]: WebUI first-run setup code: 12345678"
echo "[10:00:01] [Main/INFO]: Done! Listening on 127.0.0.1:47624 (TCP)"
echo "[10:00:01] [Main/INFO]: Server ready (0.5s)"
"#;

    const SHELL_LOOP: &str = r#"
while IFS= read -r line; do
  printf '%s\n' "$line" >> received.txt
  case "$line" in
    "webui user "*|"webui password "*)
      case "$line" in "webui user "*) ok="WebUI account saved";; *) ok="WebUI password changed";; esac
      echo "[10:00:02] [Client-1/INFO]: noise: with a colon:"
      echo "[10:00:02] [ServerConsole/INFO]: WebUI password (8–128 characters):"
      IFS= read -r p1; printf '%s\n' "$p1" >> received.txt
      echo "[10:00:02] [Client-2/INFO]: more noise:"
      echo "[10:00:02] [ServerConsole/INFO]: Confirm WebUI password:"
      IFS= read -r p2; printf '%s\n' "$p2" >> received.txt
      if [ "$p1" = "$p2" ]; then
        echo "[10:00:03] [ServerConsole/INFO]: $ok: skradle; existing sessions revoked."
      else
        echo "[10:00:03] [ServerConsole/WARN]: Passwords do not match"
      fi;;
    shutdown)
      echo "[10:00:04] [ServerConsole/INFO]: Type SHUT DOWN PROCESS to confirm full process shutdown:"
      IFS= read -r c; printf '%s\n' "$c" >> received.txt
      if [ "$c" = "SHUT DOWN PROCESS" ]; then echo "[10:00:04] [Main/INFO]: Server stopped."; exit 0; fi;;
  esac
done
"#;

    struct Fixture {
        _dir: tempfile::TempDir,
        host: Arc<ServerHost>,
        layout: ServerLayout,
        config: HostConfig,
    }

    /// An HTTP listener that answers every request with `status`, standing in for the engine's web listener.
    fn web_listener(status: u16) -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let mut stream = stream;
                let mut buf = [0u8; 512];
                let _ = stream.read(&mut buf);
                let _ = stream.write_all(
                    format!(
                        "HTTP/1.1 {status} X\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{{}}"
                    )
                    .as_bytes(),
                );
            }
        });
        port
    }

    fn free_port() -> u16 {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    }

    fn fixture_with(
        script: &str,
        tweak: impl FnOnce(&mut HostOptions),
        web_status: u16,
    ) -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let layout = engine::testkit::accepted_layout(dir.path());
        engine::testkit::install(&layout, "0.40.17", &[]);
        let java = dir.path().join("java");
        std::fs::write(&java, format!("#!/bin/sh\n{script}")).unwrap();
        std::fs::set_permissions(&java, std::fs::Permissions::from_mode(0o755)).unwrap();
        let config = HostConfig {
            lan: false,
            game_port: free_port(),
            web_port: web_listener(web_status),
        };
        config.write(&layout).unwrap();
        let mut options = HostOptions::new(java);
        options.check_ports = false;
        options.start_timeout = Duration::from_secs(10);
        options.shutdown_timeout = Duration::from_millis(600);
        options.prompt_timeout = Duration::from_secs(3);
        tweak(&mut options);
        Fixture {
            host: Arc::new(ServerHost::new(layout.clone(), options)),
            layout,
            config,
            _dir: dir,
        }
    }

    fn fixture(script: &str) -> Fixture {
        fixture_with(&format!("{BANNER}{SHELL_LOOP}"), |_| {}, 200).with_script(script)
    }

    impl Fixture {
        fn with_script(self, _unused: &str) -> Self {
            self
        }
        fn received(&self) -> Vec<String> {
            std::fs::read_to_string(self.layout.root().join("received.txt"))
                .unwrap_or_default()
                .lines()
                .map(str::to_owned)
                .collect()
        }
    }

    #[test]
    fn starts_applies_the_admin_account_and_stops_gracefully() {
        let f = fixture("");
        let status = f.host.start().unwrap();
        assert_eq!(status.phase, Phase::Running);
        assert!(status.game_ready);
        assert_eq!(status.listening.as_deref(), Some("127.0.0.1:47624"));
        assert_eq!(status.web, WebState::Ready);
        assert_eq!(status.admin, AdminState::Ready);
        assert_eq!(
            status.web_ui_url,
            Some(format!("http://127.0.0.1:{}/admin/", f.config.web_port))
        );
        assert!(status.pid.is_some());

        // The engine saw the command and the generated password twice, and the buffer never did.
        let credentials = f.host.admin_credentials().unwrap();
        let received = f.received();
        assert_eq!(received[0], "webui user skradle ADMINISTRATOR");
        assert_eq!(received[1], credentials.reveal_password());
        assert_eq!(received[2], credentials.reveal_password());
        let (log, _) = f.host.log_since(0);
        assert!(log
            .iter()
            .all(|l| !l.contains(credentials.reveal_password())));
        assert!(log.iter().any(|l| l.ends_with("setup code: [hidden]")));
        assert!(log.iter().all(|l| !l.contains("12345678")));

        assert_eq!(f.host.stop().unwrap(), StopKind::Graceful);
        assert_eq!(f.host.status().phase, Phase::Stopped);
        assert_eq!(f.host.status().pid, None);
        assert!(f
            .received()
            .ends_with(&["shutdown".to_owned(), "SHUT DOWN PROCESS".to_owned()]));
        assert_eq!(f.host.stop().unwrap(), StopKind::NotRunning);
    }

    #[test]
    fn a_restart_does_not_overwrite_a_password_the_player_may_have_changed() {
        let f = fixture("");
        f.host.start().unwrap();
        let first = f.host.admin_credentials().unwrap();
        // The real engine creates its account database; the fake one does not, so stand in for it.
        std::fs::create_dir_all(f.layout.data_dir()).unwrap();
        std::fs::write(f.layout.data_dir().join("webui.sqlite"), b"").unwrap();
        f.host.restart().unwrap();
        assert_eq!(f.host.admin_credentials().unwrap(), first);
        assert_eq!(f.host.status().admin, AdminState::Ready);
        assert_eq!(count_applied(&f), 1, "applied once, at the first start");

        // A server reset removes the database: the account is created again, with the same stored password.
        f.host.stop().unwrap();
        std::fs::remove_file(f.layout.data_dir().join("webui.sqlite")).unwrap();
        f.host.start().unwrap();
        assert_eq!(count_applied(&f), 2);
        assert_eq!(f.host.admin_credentials().unwrap(), first);
        f.host.stop().unwrap();
    }

    #[test]
    fn credentials_that_never_reached_the_engine_are_applied_at_the_next_start() {
        let f = fixture("");
        f.host.start().unwrap();
        f.host.stop().unwrap();
        // A crash after the file was written and before the engine accepted it leaves no marker.
        std::fs::create_dir_all(f.layout.data_dir()).unwrap();
        std::fs::write(f.layout.data_dir().join("webui.sqlite"), b"").unwrap();
        std::fs::remove_file(f.layout.root().join("admin-applied")).unwrap();
        f.host.start().unwrap();
        assert_eq!(count_applied(&f), 2);
        f.host.stop().unwrap();
    }

    fn count_applied(f: &Fixture) -> usize {
        f.received()
            .into_iter()
            .filter(|l| l.starts_with("webui user"))
            .count()
    }

    #[test]
    fn the_admin_password_can_be_reset_while_running() {
        let f = fixture("");
        f.host.start().unwrap();
        let before = f.host.admin_credentials().unwrap();
        let after = f.host.reset_admin_password().unwrap();
        assert_ne!(before.reveal_password(), after.reveal_password());
        assert!(f.received().contains(&"webui password skradle".to_owned()));
        assert_eq!(f.host.admin_credentials().unwrap(), after);
        f.host.stop().unwrap();
        assert!(matches!(
            f.host.reset_admin_password(),
            Err(HostError::NotRunning)
        ));
    }

    #[test]
    fn a_reset_falls_back_to_creating_the_account_when_it_is_gone() {
        let script = SHELL_LOOP.replace(
            r#""webui user "*|"webui password "*)"#,
            r#""webui password "*) echo "[10:00:02] [ServerConsole/WARN]: WebUI account unavailable";;
    "webui user "*)"#,
        );
        let f = fixture_with(&format!("{BANNER}{script}"), |_| {}, 200);
        f.host.start().unwrap();
        let fresh = f.host.reset_admin_password().unwrap();
        let received = f.received();
        assert!(received.contains(&"webui password skradle".to_owned()));
        assert_eq!(
            received
                .iter()
                .filter(|l| l.starts_with("webui user"))
                .count(),
            2,
            "once at start, once as the fallback"
        );
        assert_eq!(f.host.admin_credentials().unwrap(), fresh);
        f.host.stop().unwrap();
    }

    #[test]
    fn a_refused_admin_command_does_not_fail_the_start() {
        let script = r#"
while IFS= read -r line; do
  case "$line" in
    "webui user "*) echo "[10:00:02] [ServerConsole/WARN]: WebUI username needs 3–64 letters";;
    shutdown) echo "[10:00:04] [ServerConsole/INFO]: Type SHUT DOWN PROCESS to confirm:"; IFS= read -r c; exit 0;;
  esac
done
"#;
        let f = fixture_with(&format!("{BANNER}{script}"), |_| {}, 200);
        let status = f.host.start().unwrap();
        assert_eq!(status.phase, Phase::Running);
        assert!(matches!(status.admin, AdminState::Failed(ref why) if why.contains("username")));
        f.host.stop().unwrap();
    }

    #[test]
    fn a_process_that_ignores_shutdown_gets_sigterm() {
        let script = r#"
trap 'echo "[10:00:05] [Main/INFO]: Server stopped."; exit 0' TERM
while true; do sleep 0.05; done
"#;
        let f = fixture_with(
            &format!("{BANNER}{script}"),
            |o| {
                o.manage_admin = false;
                o.prompt_timeout = Duration::from_millis(200);
            },
            200,
        );
        f.host.start().unwrap();
        let began = Instant::now();
        assert_eq!(f.host.stop().unwrap(), StopKind::Terminated);
        assert!(
            began.elapsed() < Duration::from_secs(5),
            "{:?}",
            began.elapsed()
        );
        assert_eq!(f.host.status().phase, Phase::Stopped);
    }

    #[test]
    fn a_process_that_ignores_everything_is_killed() {
        let script = r#"
trap '' TERM
while true; do sleep 0.05; done
"#;
        let f = fixture_with(
            &format!("{BANNER}{script}"),
            |o| {
                o.manage_admin = false;
                o.prompt_timeout = Duration::from_millis(200);
                o.shutdown_timeout = Duration::from_millis(300);
            },
            200,
        );
        let pid = f.host.start().unwrap().pid.unwrap();
        assert_eq!(f.host.stop().unwrap(), StopKind::Killed);
        // The process is really gone.
        let alive = Command::new("kill")
            .args(["-0", &pid.to_string()])
            .output()
            .unwrap();
        assert!(!alive.status.success());
    }

    #[test]
    fn a_game_port_failure_with_the_child_still_alive_is_reported_and_the_child_ended() {
        let script = r#"
echo "[10:00:01] [Main/ERROR]: Server startup failed: Address already in use (Bind failed)"
while IFS= read -r line; do
  case "$line" in
    shutdown) echo "[10:00:04] [ServerConsole/INFO]: Type SHUT DOWN PROCESS to confirm full process shutdown:"; IFS= read -r c; exit 0;;
  esac
done
"#;
        let f = fixture_with(script, |_| {}, 200);
        match f.host.start() {
            Err(HostError::StartupFailed(StartupFailure::GamePortBusy(text))) => {
                assert!(text.contains("Address already in use"))
            }
            other => panic!("{other:?}"),
        }
        let status = f.host.status();
        assert!(matches!(status.phase, Phase::Failed(_)), "{status:?}");
        assert_eq!(status.pid, None, "no process left");
        assert!(!f.host.is_alive());
    }

    #[test]
    fn a_web_port_failure_that_exits_with_code_one_names_the_web_port() {
        let script = r#"
echo "[10:00:01] [Main/ERROR]: Server startup failed: Cannot bind shared HTTP listener on 0.0.0.0:8080: Address already in use"
exit 1
"#;
        let f = fixture_with(script, |_| {}, 200);
        match f.host.start() {
            Err(HostError::StartupFailed(StartupFailure::WebPortBusy(_))) => {}
            other => panic!("{other:?}"),
        }
        assert_eq!(f.host.status().exit_code, Some(1));
    }

    #[test]
    fn an_early_crash_reports_the_exit_code_and_the_last_lines() {
        let script =
            "echo 'Exception in thread \"main\" java.lang.UnsupportedClassVersionError'\nexit 3\n";
        let f = fixture_with(script, |_| {}, 200);
        match f.host.start() {
            Err(HostError::EarlyExit { code, tail }) => {
                assert_eq!(code, Some(3));
                assert!(tail
                    .iter()
                    .any(|l| l.contains("UnsupportedClassVersionError")));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn stopping_while_a_start_waits_cancels_it() {
        let script = "echo 'loading...'\nsleep 30\n";
        let f = fixture_with(script, |o| o.start_timeout = Duration::from_secs(30), 200);
        let host = f.host.clone();
        let starter = thread::spawn(move || host.start());
        thread::sleep(Duration::from_millis(300));
        let began = Instant::now();
        f.host.stop().unwrap();
        assert!(matches!(starter.join().unwrap(), Err(HostError::Aborted)));
        assert!(began.elapsed() < Duration::from_secs(8));
        assert_eq!(f.host.status().phase, Phase::Stopped);
        assert!(!f.host.is_alive());
    }

    #[test]
    fn a_start_that_never_gets_ready_times_out_and_leaves_no_process() {
        let f = fixture_with(
            "sleep 30\n",
            |o| o.start_timeout = Duration::from_millis(400),
            200,
        );
        assert!(matches!(f.host.start(), Err(HostError::StartTimeout)));
        assert!(!f.host.is_alive());
    }

    #[test]
    fn a_process_that_dies_while_running_shows_as_failed() {
        let script = format!("{BANNER}sleep 0.5\nexit 7\n");
        let f = fixture_with(&script, |o| o.manage_admin = false, 200);
        assert_eq!(f.host.start().unwrap().phase, Phase::Running);
        thread::sleep(Duration::from_millis(1200));
        let status = f.host.status();
        assert!(
            matches!(status.phase, Phase::Failed(ref why) if why.contains("7")),
            "{status:?}"
        );
        assert_eq!(status.exit_code, Some(7));
        assert!(!status.game_ready);
    }

    #[test]
    fn an_engine_that_dies_while_the_admin_account_is_applied_fails_the_start() {
        // Listening, then gone as soon as the first console command arrives.
        let script = format!("{BANNER}IFS= read -r line\nexit 3\n");
        let f = fixture_with(&script, |_| {}, 200);
        let err = f.host.start().unwrap_err();
        assert!(
            matches!(err, HostError::EarlyExit { code: Some(3), .. }),
            "{err}"
        );
        let status = f.host.status();
        assert!(matches!(status.phase, Phase::Failed(_)), "{status:?}");
        assert_eq!(status.pid, None);
    }

    #[test]
    fn a_stop_that_arrives_while_the_admin_account_is_applied_fails_the_start() {
        // The engine never answers the console, so the start sits in the admin step until the prompt timeout.
        let script = format!("{BANNER}exec sleep 30\n");
        let f = fixture_with(&script, |o| o.prompt_timeout = Duration::from_secs(2), 200);
        let host = f.host.clone();
        let starter = thread::spawn(move || host.start());
        thread::sleep(Duration::from_millis(800));
        let _ = f.host.stop();
        let result = starter.join().unwrap();
        assert!(matches!(result, Err(HostError::Aborted)), "{result:?}");
        assert!(!f.host.is_alive());
    }

    #[test]
    fn an_update_that_cannot_happen_leaves_the_running_server_and_the_backups_alone() {
        let f = fixture("");
        f.host.start().unwrap();
        // The licence is not accepted: the check fails before anything is stopped or backed up.
        std::fs::remove_file(f.layout.terms_record()).unwrap();
        let client = Client::new();
        let err = f.host.update_engine(&client, &mut |_| {}).unwrap_err();
        assert!(
            matches!(
                err,
                HostError::Engine(engine::EngineError::TermsNotAccepted)
            ),
            "{err}"
        );
        assert!(f.host.status().pid.is_some(), "the server is still running");
        assert!(backups::list(&f.layout).unwrap().is_empty());
        f.host.stop().unwrap();
    }

    #[test]
    fn a_web_listener_that_refuses_the_probe_is_reported_not_hidden() {
        let f = fixture_with(&format!("{BANNER}{SHELL_LOOP}"), |_| {}, 403);
        let status = f.host.start().unwrap();
        assert_eq!(status.web, WebState::Refused(403));
        f.host.stop().unwrap();
    }

    #[test]
    fn ports_are_checked_before_the_start_and_both_are_named() {
        let f = fixture_with(
            &format!("{BANNER}{SHELL_LOOP}"),
            |o| o.check_ports = true,
            200,
        );
        // The web port is held by the test's own listener already.
        match f.host.start() {
            Err(HostError::PortConflict(list)) => {
                assert_eq!(list.len(), 1);
                assert_eq!(list[0].which, ports::PortUse::Web);
                assert_eq!(list[0].port, f.config.web_port);
                assert!(list[0].suggestion.is_some());
            }
            other => panic!("{other:?}"),
        }
        assert!(!f.host.is_alive());
        // Hold the game port as well.
        let _game = TcpListener::bind(("127.0.0.1", f.config.game_port)).unwrap();
        match f.host.start() {
            Err(HostError::PortConflict(list)) => assert_eq!(list.len(), 2),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn status_polling_never_deadlocks_with_lifecycle_calls() {
        let f = fixture_with(
            &format!("{BANNER}{SHELL_LOOP}"),
            |o| o.manage_admin = false,
            200,
        );
        let stop = Arc::new(AtomicBool::new(false));
        let pollers: Vec<_> = (0..3)
            .map(|_| {
                let host = f.host.clone();
                let stop = stop.clone();
                thread::spawn(move || {
                    let mut polls = 0u32;
                    while !stop.load(Ordering::SeqCst) {
                        let _ = host.status();
                        let _ = host.log_since(0);
                        polls += 1;
                    }
                    polls
                })
            })
            .collect();
        let host = f.host.clone();
        let cycles = thread::spawn(move || {
            for _ in 0..3 {
                host.start().unwrap();
                host.stop().unwrap();
            }
        });
        let began = Instant::now();
        while !cycles.is_finished() {
            assert!(
                began.elapsed() < Duration::from_secs(60),
                "start/stop did not finish: deadlock"
            );
            thread::sleep(Duration::from_millis(20));
        }
        cycles.join().unwrap();
        stop.store(true, Ordering::SeqCst);
        for poller in pollers {
            assert!(poller.join().unwrap() > 0);
        }
    }

    #[test]
    fn the_engines_shipped_config_is_refused_until_the_launcher_has_written_its_own() {
        let f = fixture("");
        let shipped = "server-ip: 0.0.0.0\nserver-port: 47624\nwebui-enabled: true\nwebui-public-url: 0.0.0.0\nwebui/content_url-port: 8080\nwebui-tls-mode: http\n";
        std::fs::write(f.layout.server_yml(), shipped).unwrap();
        assert!(matches!(f.host.start(), Err(HostError::NotConfigured)));
        assert!(!f.host.is_alive());
    }

    #[test]
    fn a_missing_engine_or_java_or_webui_config_is_refused_before_anything_runs() {
        let f = fixture("");
        std::fs::remove_file(f.layout.jar()).unwrap();
        assert!(matches!(f.host.start(), Err(HostError::NotInstalled)));

        let g = fixture("");
        std::fs::remove_file(g.host.options.java.clone()).unwrap();
        assert!(matches!(g.host.start(), Err(HostError::NoJava)));

        let h = fixture("");
        std::fs::write(h.layout.server_yml(), "server-port: 47624\n").unwrap();
        assert!(matches!(h.host.start(), Err(HostError::NotConfigured)));
    }

    #[test]
    fn a_lifecycle_lock_poisoned_by_a_panic_does_not_make_the_host_busy_for_good() {
        let f = fixture("");
        let host = f.host.clone();
        assert!(!host.is_busy());
        let poisoned = std::thread::spawn({
            let host = host.clone();
            move || {
                let _guard = host.lifecycle.lock().unwrap();
                panic!("a lifecycle call failed");
            }
        })
        .join();
        assert!(poisoned.is_err());
        assert!(!host.is_busy());
        let held = host.lifecycle.lock();
        let _held = held.unwrap_or_else(|e| e.into_inner());
        assert!(host.is_busy(), "a held lock still counts as busy");
    }

    #[cfg(unix)]
    #[test]
    fn a_stopped_run_is_never_signalled_again() {
        let f = fixture("");
        f.host.start().unwrap();
        let run = f.host.current().unwrap();
        f.host.stop().unwrap();
        assert!(
            !f.host.terminate(&run),
            "its number may belong to someone else now"
        );
    }

    #[test]
    fn lifecycle_calls_from_inside_a_lifecycle_call_fail_instead_of_deadlocking() {
        let f = fixture("");
        let host = f.host.clone();
        let result = f.host.run_stopped(|_, _| {
            assert!(matches!(host.start(), Err(HostError::Reentrant)));
            assert!(matches!(host.stop(), Err(HostError::Reentrant)));
            assert!(matches!(host.backup(None), Err(HostError::Reentrant)));
            // Status and the log never take the lock.
            assert_eq!(host.status().phase, Phase::Stopped);
            let _ = host.log_since(0);
            assert!(host.is_busy());
            Ok(7)
        });
        assert_eq!(result.unwrap().value, 7);
        assert!(!f.host.is_busy());
    }

    #[test]
    fn a_reentrant_stop_does_not_wipe_an_abort_another_thread_requested() {
        let f = fixture("");
        let host = f.host.clone();
        f.host
            .run_stopped(|_, _| {
                // Another thread's stop has asked a start to give up; this nested stop is refused outright.
                host.abort.store(true, Ordering::SeqCst);
                assert!(matches!(host.stop(), Err(HostError::Reentrant)));
                assert!(host.abort.load(Ordering::SeqCst));
                host.abort.store(false, Ordering::SeqCst);
                Ok(())
            })
            .unwrap();
    }

    #[test]
    fn waiting_for_the_web_ui_ends_at_once_when_a_stop_is_requested() {
        let f = fixture("");
        let run = Arc::new(Run::new(16));
        // A port nothing listens on, so the probe would otherwise be retried for the whole ten seconds.
        let port = {
            let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
            listener.local_addr().unwrap().port()
        };
        f.host.abort.store(true, Ordering::SeqCst);
        let started = Instant::now();
        assert!(matches!(
            f.host.wait_for_web(port, &run),
            WebState::NotAnswering
        ));
        assert!(started.elapsed() < Duration::from_secs(3));
    }

    #[test]
    fn a_backup_in_progress_keeps_a_start_waiting() {
        let f = fixture("");
        let host = f.host.clone();
        let (holding_tx, holding_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
        let finished = Arc::new(AtomicBool::new(false));
        let finished_in = finished.clone();
        let backup = thread::spawn(move || {
            host.run_stopped(|layout, proof| {
                holding_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                finished_in.store(true, Ordering::SeqCst);
                Ok(backups::create(layout, proof, Some("held"), 5)?)
            })
        });
        holding_rx.recv().unwrap();
        let host = f.host.clone();
        let finished_seen = finished.clone();
        let starter = thread::spawn(move || {
            let result = host.start();
            (result, finished_seen.load(Ordering::SeqCst))
        });
        thread::sleep(Duration::from_millis(300));
        assert!(!starter.is_finished(), "the start must wait for the backup");
        release_tx.send(()).unwrap();
        backup.join().unwrap().unwrap();
        let (result, backup_was_done) = starter.join().unwrap();
        assert!(
            backup_was_done,
            "the start only began after the backup ended"
        );
        assert_eq!(result.unwrap().phase, Phase::Running);
        f.host.stop().unwrap();
    }

    #[test]
    fn backup_restore_and_reset_stop_a_running_server_first() {
        let f = fixture("");
        std::fs::create_dir_all(f.layout.data_dir()).unwrap();
        std::fs::write(f.layout.database(), b"db").unwrap();
        f.host.start().unwrap();
        let made = f.host.backup(Some("manual")).unwrap();
        assert!(made.was_running);
        assert_eq!(f.host.status().phase, Phase::Stopped);
        std::fs::write(f.layout.database(), b"changed").unwrap();
        let restored = f.host.restore(&made.value.name).unwrap();
        assert!(!restored.was_running);
        assert_eq!(std::fs::read(f.layout.database()).unwrap(), b"db");
        f.host.reset(true).unwrap();
        assert!(!f.layout.database().exists());
    }

    #[test]
    fn configure_is_refused_while_the_server_runs() {
        let f = fixture("");
        f.host.start().unwrap();
        assert!(f.host.configure(&f.config).is_err());
        f.host.stop().unwrap();
        f.host.configure(&f.config).unwrap();
    }
}
