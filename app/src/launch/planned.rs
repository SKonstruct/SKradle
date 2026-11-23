//! The launcher that follows the core's launch plan (`skradle_core::launch::plan`).
//!
//! The planner owns the order that must not be broken: an update first (through the game's own updater), otherwise
//! mods, then the server settings, then the game started directly. This module gathers what the planner needs,
//! runs the steps before the process through the real effects, and hands the final command to the supervisor,
//! which starts it and follows it. All of this happens on the supervisor's thread, so the window never waits.
//!
//! Where things live (none of it is created by the launch itself):
//! - the selected server: `servers.json` in the launcher's config folder, written by the Servers page (no file
//!   means Official),
//! - the player's mods: `<data dir>/SKradle/mods` (no folder means nothing to apply),
//! - the graphics agent: `<data dir>/SKradle/skgfx/skgfx-agent.jar` (no file means graphics enhancements are off).
//!   TODO(graphics): downloading the agent is not built yet (it needs the release pipeline of the `skgfx` repository).

use std::path::{Path, PathBuf};

use skradle_core::game::{self, Env};
use skradle_core::getdown::CommandSpec;
use skradle_core::graphics::{agent_args, GraphicsSettings};
use skradle_core::launch::direct::{DirectOptions, GarbageCollector, Inputs, Memory};
use skradle_core::launch::plan::{self as core_plan, Effects, LaunchKind, PlanInputs, RealEffects};
use skradle_core::launch::steam::{self, ClientState, Waited, Waiter};
use skradle_core::log;
use skradle_core::mods::ApplyOptions;
use skradle_core::runtime::{self, Runtime};
#[cfg(test)]
use skradle_core::servers;
use skradle_core::servers::Server;

use super::plan::{self, Machine};
use super::supervisor::{self, Context, Prepare, Timing};
use super::{memory, LaunchError, Launcher, Request, Running, Sink};
use super::{pending, Event, Phase, Step};
use crate::discovery::{self, Kind};
use crate::settings::{self, Collector, Settings};

/// What a launch depends on besides the game folder and the settings.
#[derive(Debug, Clone, Default)]
pub struct Surroundings {
    pub arch: String,
    pub server: Option<Server>,
    pub mods_dir: Option<PathBuf>,
    pub agent_jar: Option<PathBuf>,
    /// The newest game version an earlier background check recorded (`launch::pending`). A launch never asks the
    /// network; this is only what is already known locally.
    pub known_remote_version: Option<String>,
    /// The Steam client is running and signed in (the launcher waits for it before planning). Only a Steam install
    /// on Official needs it.
    pub steam_running: bool,
    /// SKradle's data folder, where the pending-update marker lives.
    pub data_dir: Option<PathBuf>,
}

impl Surroundings {
    /// Looks around on this computer. Reads local files only: no network, no process probes.
    pub fn detect() -> Self {
        let data = dirs::data_dir().map(|d| d.join("SKradle"));
        Self {
            arch: std::env::consts::ARCH.to_owned(),
            server: crate::servers::selected_server_from(&settings::config_dir()),
            // The same folder the Mods page manages (it honours SKRADLE_MODS_DIR); no folder means nothing to apply.
            mods_dir: crate::mods_host::mods_dir().filter(|p| p.is_dir()),
            agent_jar: data
                .as_ref()
                .map(|d| d.join("skgfx").join("skgfx-agent.jar"))
                .filter(|p| p.is_file()),
            known_remote_version: data.as_deref().and_then(pending::read),
            steam_running: false,
            data_dir: data,
        }
    }
}

/// The planner's effects, except that "start the game" only hands the command back: the supervisor starts it in
/// its own way (output captured, its own process group). Each effect first says which step it is, so the window can
/// show what is going on instead of one generic line.
struct Deferred<'a> {
    inner: RealEffects,
    report: &'a dyn Fn(Step),
}

impl Effects for Deferred<'_> {
    type Handle = CommandSpec;

    fn clean_extra_txt(&mut self, game_root: &Path) -> Result<(), String> {
        self.inner.clean_extra_txt(game_root)
    }

    fn apply_mods(
        &mut self,
        game_root: &Path,
        mods_dir: &Path,
        options: ApplyOptions,
    ) -> Result<Vec<String>, String> {
        (self.report)(Step::Mods);
        self.inner.apply_mods(game_root, mods_dir, options)
    }

    fn apply_connection(
        &mut self,
        game_root: &Path,
        server: Option<&Server>,
    ) -> Result<(), String> {
        (self.report)(Step::Server);
        self.inner.apply_connection(game_root, server)
    }

    fn spawn(&mut self, command: &CommandSpec) -> Result<CommandSpec, String> {
        (self.report)(Step::Starting);
        Ok(command.clone())
    }
}

fn graphics_settings(settings: &Settings) -> GraphicsSettings {
    GraphicsSettings {
        render_scale_percent: settings.render_scale,
        sharpening_percent: settings.sharpening,
        sharp_hud: settings.sharp_hud,
        show_fps: settings.show_fps,
        frame_generation: settings.frame_gen_mode == 1,
        target_fps: settings.target_fps,
    }
}

/// The JVM-related settings as the direct launch takes them: memory, collector, explicit GC and the player's own
/// extra arguments (which come last and win).
pub fn direct_options(settings: &Settings) -> DirectOptions {
    DirectOptions {
        memory: settings.memory_mb.map(|mb| Memory {
            initial_mb: None,
            max_mb: Some(mb),
        }),
        collector: match settings.collector {
            Collector::Default => GarbageCollector::Default,
            Collector::G1 => GarbageCollector::G1,
            Collector::Zgc => GarbageCollector::Zgc,
            Collector::Parallel => GarbageCollector::Parallel,
            Collector::Serial => GarbageCollector::Serial,
        },
        disable_explicit_gc: settings.disable_explicit_gc,
        jvm_args: split_args(&settings.extra_jvm_args),
        ..DirectOptions::default()
    }
}

/// The file cleaning and purging switches as the mods engine takes them (its flags say what to keep, so they invert).
pub fn mod_options(settings: &Settings) -> ApplyOptions {
    ApplyOptions {
        keep_modified_originals: !settings.file_cleaning,
        keep_added_files: !settings.file_purging,
        ..ApplyOptions::default()
    }
}

/// Plans the launch and runs every step before the process. Returns the command to run and how to follow it.
pub fn prepare(
    request: &Request,
    machine: &Machine,
    around: &Surroundings,
    effects: &mut impl Effects<Handle = CommandSpec>,
) -> Result<(CommandSpec, LaunchKind), LaunchError> {
    let install = &request.install;
    let settings = &request.settings;
    let minimum = plan::java_minimum(&install.path);
    // Checks the player's own Java first, so its problems keep their precise messages.
    let chosen = plan::pick_java(machine, install, &settings.java_path, minimum)?;
    let runtimes: Vec<Runtime> = if settings.java_path.trim().is_empty() {
        runtime::find_runtimes(
            &machine.runtime_env,
            Some(&install.path),
            machine.managed_dir.as_deref(),
        )
    } else {
        vec![chosen]
    };

    let core_install = install.to_core();
    let game_files = Inputs::read(&core_install)
        .map_err(|e| LaunchError::Prepare(format!("could not read the game's files: {e}")))?;
    let options = direct_options(settings);
    // The master switch: off means no agent, so the planner keeps the game's own Java.
    let graphics = around
        .agent_jar
        .as_deref()
        .filter(|_| settings.graphics_enabled)
        .map(|jar| agent_args(jar, &graphics_settings(settings)));
    // Only what is known locally: Steam keeps its own install up to date, so it is never "pending".
    let update_pending = install.kind != Kind::Steam
        && pending::is_pending(
            &game_files.getdown_txt,
            around.known_remote_version.as_deref(),
        );
    log::info(
        "launch",
        format!(
            "plan inputs: install kind {}, server {}, update known pending {update_pending}, mods folder {}, graphics agent {}",
            install.kind.as_str(),
            around.server.as_ref().map_or("official", |s| s.host.as_str()),
            around.mods_dir.is_some(),
            around.agent_jar.is_some(),
        ),
    );

    let plan = core_plan::plan(&PlanInputs {
        install: &core_install,
        os: machine.os,
        arch: &around.arch,
        runtimes: &runtimes,
        game_files: &game_files,
        server: around.server.as_ref(),
        mods_dir: around.mods_dir.as_deref(),
        graphics_args: graphics.as_deref(),
        update_pending,
        options: &options,
        mod_options: mod_options(settings),
        steam_running: around.steam_running,
    })
    .map_err(|refusal| {
        log::warn("launch", format!("the launch was refused: {refusal}"));
        LaunchError::Refused(refusal.to_string())
    })?;
    // Steam installs on Windows and Linux also get `steam_appid.txt` in the game folder (best effort).
    if steam::wants_appid_file(machine.os, core_install.kind, around.server.is_some()) {
        match steam::ensure_appid_file(&install.path) {
            Ok(true) => log::info("steam", "wrote steam_appid.txt into the game folder"),
            Ok(false) => log::debug("steam", "steam_appid.txt is already in place"),
            Err(e) => log::warn("steam", format!("could not write steam_appid.txt: {e}")),
        }
    }
    for warning in &plan.warnings {
        eprintln!("skradle: {warning}");
        log::info("launch", format!("warning: {warning}"));
    }
    let launched = core_plan::execute(&plan, effects)
        .map_err(|failure| LaunchError::Prepare(failure.message))?;
    for note in &launched.notes {
        eprintln!("skradle: {note}");
        log::info("mods", note);
    }
    if launched.kind == LaunchKind::Updater {
        // The updater starts the game with the game's own settings, so the limit goes into `extra.txt`.
        if let Some(mb) = settings.memory_mb {
            memory::write_limit(&install.path, mb)
                .map_err(|e| LaunchError::ExtraTxt(e.to_string()))?;
        }
    }
    Ok((launched.handle, launched.kind))
}

/// Makes sure the Steam client is running and signed in, starting it and waiting (up to a minute, cancellable) when it
/// is not. Reports the "Starting Steam" step to the window while it waits.
pub fn ensure_steam(waiter: &mut dyn Waiter, ctx: &Context<'_>) -> Result<(), LaunchError> {
    let mut reported = false;
    let mut on_wait = |state: ClientState| {
        if !reported {
            reported = true;
            log::info(
                "steam",
                format!("Steam is {}: starting it and waiting", state.describe()),
            );
            (ctx.sink)(Event::Phase(Phase::Launching, Step::Steam));
        }
    };
    let outcome = steam::ensure_ready(
        waiter,
        &|| ctx.cancelled(),
        &mut on_wait,
        steam::WAIT_TIMEOUT,
        steam::WAIT_POLL,
        steam::WAIT_SETTLE,
    );
    match outcome {
        Waited::AlreadyReady => log::info("steam", "Steam is running and signed in"),
        Waited::BecameReady { started } => {
            log::info(
                "steam",
                format!("Steam is ready (SKradle started it: {started})"),
            );
            (ctx.sink)(Event::Phase(Phase::Launching, Step::Preparing));
        }
        Waited::Cancelled => log::info("steam", "cancelled while waiting for Steam"),
        other => log::warn("steam", format!("Steam did not become ready: {other:?}")),
    }
    match outcome {
        Waited::Cancelled => Err(LaunchError::Refused("Cancelled.".to_owned())),
        other => steam::failure_message(other).map_or(Ok(()), |m| Err(LaunchError::Refused(m))),
    }
}

/// Starts the game the way the core's planner decides.
pub struct PlannedLauncher {
    timing: Timing,
}

impl PlannedLauncher {
    pub fn new() -> Self {
        Self {
            timing: Timing::default(),
        }
    }
}

impl Launcher for PlannedLauncher {
    fn start(&self, request: Request, sink: Sink) -> Result<Box<dyn Running>, LaunchError> {
        let game_root = request.install.path.clone();
        let prepared: Prepare = Box::new(move |ctx| {
            let mut request = request;
            let machine = Machine::current();
            let mut around = Surroundings::detect();
            // The launch button only knows the folder: work out whether this is a Steam install.
            if request.install.kind == Kind::Custom {
                request.install.kind =
                    discovery::detect_kind(&request.install.path, &game::locate(&Env::current()));
            }
            let core_kind = request.install.to_core().kind;
            let private = around.server.is_some();
            log::info(
                "launch",
                format!(
                    "launch requested: {} install at {}, server {}",
                    request.install.kind.as_str(),
                    request.install.path.display(),
                    around
                        .server
                        .as_ref()
                        .map_or("official", |s| s.host.as_str())
                ),
            );
            if steam::needs_client(core_kind, private) {
                ensure_steam(&mut steam::System::new(machine.os), ctx)?;
                around.steam_running = true;
            } else {
                log::debug("steam", "no Steam sign-in needed for this launch");
            }
            let local_getdown = std::fs::read_to_string(request.install.path.join("getdown.txt"))
                .unwrap_or_default();
            let report = |step: Step| (ctx.sink)(Event::Phase(Phase::Launching, step));
            let mut effects = Deferred {
                inner: RealEffects,
                report: &report,
            };
            let (spec, kind) = prepare(&request, &machine, &around, &mut effects)?;
            // Standalone installs check for a newer game in the background, after the launch is already going.
            if kind == LaunchKind::Direct && request.install.kind != Kind::Steam {
                if let Some(dir) = around.data_dir.clone() {
                    pending::check_in_background(dir, local_getdown);
                }
            }
            Ok((spec, kind))
        });
        Ok(Box::new(supervisor::start_prepared(
            prepared,
            game_root,
            self.timing,
            sink,
        )))
    }
}

/// Splits the player's extra JVM arguments on spaces, like a shell would: `"..."` or `'...'` keeps spaces inside one
/// argument (`-javaagent:"C:\Program Files\x.jar"` is one argument, with the quotes removed). A backslash is not an
/// escape (Windows paths), and an unclosed quote runs to the end of the text.
pub(crate) fn split_args(text: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut started = false;
    let mut quote: Option<char> = None;
    for c in text.chars() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => current.push(c),
            None if c == '"' || c == '\'' => {
                quote = Some(c);
                started = true;
            }
            None if c.is_whitespace() => {
                if started {
                    args.push(std::mem::take(&mut current));
                    started = false;
                }
            }
            None => {
                current.push(c);
                started = true;
            }
        }
    }
    if started {
        args.push(current);
    }
    args
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discovery::{Install, Kind};
    use skradle_core::game::Os;
    use skradle_core::runtime::RuntimeEnv;
    use std::fs;

    const GETDOWN: &str = include_str!("../../../core/tests/fixtures/getdown.txt");
    const FULL: &str =
        "JAVA_VERSION=\"25.0.3\"\nMODULES=\"java.base java.desktop java.instrument\"\n";

    /// Records what the planner asked for; "start the game" returns the command.
    #[derive(Default)]
    struct Recorder {
        calls: Vec<&'static str>,
        mod_options: Vec<ApplyOptions>,
        fail_connection: bool,
        /// The server the planner asked `config.jar` to be pointed at.
        connection: Option<Option<Server>>,
    }

    impl Effects for Recorder {
        type Handle = CommandSpec;
        fn clean_extra_txt(&mut self, _: &Path) -> Result<(), String> {
            self.calls.push("clean");
            Ok(())
        }
        fn apply_mods(
            &mut self,
            _: &Path,
            _: &Path,
            options: ApplyOptions,
        ) -> Result<Vec<String>, String> {
            self.calls.push("mods");
            self.mod_options.push(options);
            Ok(vec![])
        }
        fn apply_connection(&mut self, _: &Path, server: Option<&Server>) -> Result<(), String> {
            self.calls.push("connection");
            self.connection = Some(server.cloned());
            if self.fail_connection {
                Err("config.jar is locked".into())
            } else {
                Ok(())
            }
        }
        fn spawn(&mut self, command: &CommandSpec) -> Result<CommandSpec, String> {
            self.calls.push("spawn");
            Ok(command.clone())
        }
    }

    struct World {
        dir: tempfile::TempDir,
        request: Request,
        machine: Machine,
    }

    /// A game folder with every jar the fixture names and a Java that satisfies it.
    fn world() -> World {
        let dir = tempfile::tempdir().unwrap();
        let game = dir.path().join("game");
        fs::create_dir_all(game.join("code")).unwrap();
        fs::write(game.join("getdown.txt"), GETDOWN).unwrap();
        for line in GETDOWN.lines().filter_map(|l| l.strip_prefix("code = ")) {
            let name = line.rsplit(' ').next().unwrap();
            fs::write(game.join(name), b"jar").unwrap();
        }
        let java = dir.path().join("jdk");
        fs::create_dir_all(java.join("bin")).unwrap();
        fs::write(java.join("release"), FULL).unwrap();
        fs::write(java.join("bin/java"), b"").unwrap();
        World {
            request: Request {
                install: Install {
                    path: game,
                    kind: Kind::Custom,
                },
                settings: Settings::default(),
            },
            machine: Machine {
                os: Os::Linux,
                runtime_env: RuntimeEnv {
                    java_home: Some(java),
                    path: vec![],
                },
                managed_dir: None,
            },
            dir,
        }
    }

    fn around() -> Surroundings {
        Surroundings {
            arch: "x86_64".into(),
            ..Surroundings::default()
        }
    }

    fn private_server() -> Server {
        Server {
            id: "a".into(),
            name: "A".into(),
            host: "play.example.org".into(),
            port: 47624,
            public_key: String::new(),
            client_url: String::new(),
            content_url: String::new(),
        }
    }

    /// A Steam client that becomes ready after `after` polls; time passes instantly.
    struct FakeSteam {
        polls: u32,
        after: Option<u32>,
        now: std::time::Duration,
    }

    impl Waiter for FakeSteam {
        fn state(&mut self) -> ClientState {
            self.polls += 1;
            match self.after {
                Some(n) if self.polls > n => ClientState::Ready,
                _ => ClientState::NotRunning,
            }
        }
        fn start(&mut self) -> bool {
            true
        }
        fn elapsed(&self) -> std::time::Duration {
            self.now
        }
        fn sleep(&mut self, d: std::time::Duration) {
            self.now += d;
        }
    }

    fn run_ensure(
        steam: &mut FakeSteam,
        stop: &std::sync::atomic::AtomicBool,
    ) -> (Result<(), LaunchError>, Vec<Event>) {
        let seen = std::sync::Mutex::new(Vec::new());
        let sink = |e: Event| seen.lock().unwrap().push(e);
        let result = ensure_steam(steam, &Context { stop, sink: &sink });
        let events = seen.lock().unwrap().clone();
        (result, events)
    }

    #[test]
    fn waiting_for_steam_shows_the_steam_step_and_then_goes_on() {
        let stop = std::sync::atomic::AtomicBool::new(false);
        let mut steam = FakeSteam {
            polls: 0,
            after: Some(3),
            now: Default::default(),
        };
        let (result, events) = run_ensure(&mut steam, &stop);
        assert!(result.is_ok());
        assert_eq!(
            events.first(),
            Some(&Event::Phase(Phase::Launching, Step::Steam))
        );
        assert_eq!(
            events.last(),
            Some(&Event::Phase(Phase::Launching, Step::Preparing))
        );
    }

    #[test]
    fn steam_that_never_comes_up_is_an_actionable_error() {
        let stop = std::sync::atomic::AtomicBool::new(false);
        let mut steam = FakeSteam {
            polls: 0,
            after: None,
            now: Default::default(),
        };
        let (result, _) = run_ensure(&mut steam, &stop);
        let Err(LaunchError::Refused(text)) = result else {
            panic!("expected an error")
        };
        assert!(
            text.contains("did not start") && text.contains("launch again"),
            "{text}"
        );
    }

    #[test]
    fn a_cancelled_wait_ends_without_an_error_message_for_the_player() {
        let stop = std::sync::atomic::AtomicBool::new(true);
        let mut steam = FakeSteam {
            polls: 0,
            after: None,
            now: Default::default(),
        };
        let (result, _) = run_ensure(&mut steam, &stop);
        assert!(result.is_err());
    }

    #[test]
    fn a_steam_install_that_cannot_start_steam_gets_the_plain_error() {
        let mut w = world();
        w.request.install.kind = Kind::Steam;
        let around = Surroundings {
            steam_running: false,
            ..around()
        };
        let mut fx = Recorder::default();
        let error = prepare(&w.request, &w.machine, &around, &mut fx).unwrap_err();
        assert!(
            matches!(&error, LaunchError::Refused(t) if t.starts_with("Steam is not running."))
        );
    }

    #[test]
    fn a_steam_install_with_steam_running_carries_the_app_id() {
        let mut w = world();
        w.request.install.kind = Kind::Steam;
        let around = Surroundings {
            steam_running: true,
            ..around()
        };
        let mut fx = Recorder::default();
        let (spec, _) = prepare(&w.request, &w.machine, &around, &mut fx).unwrap();
        let names: Vec<_> = spec
            .env
            .iter()
            .map(|(k, _)| k.to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["SteamAppId", "SteamGameId"]);
    }

    #[test]
    fn a_standalone_install_never_asks_for_steam() {
        let w = world();
        let around = Surroundings {
            steam_running: false,
            ..around()
        };
        let mut fx = Recorder::default();
        let (spec, _) = prepare(&w.request, &w.machine, &around, &mut fx).unwrap();
        assert!(spec.env.is_empty());
    }

    #[test]
    fn an_up_to_date_game_is_started_directly_after_the_server_settings() {
        let w = world();
        let mut fx = Recorder::default();
        let (spec, kind) = prepare(&w.request, &w.machine, &around(), &mut fx).unwrap();
        assert_eq!(kind, LaunchKind::Direct);
        assert_eq!(fx.calls.last(), Some(&"spawn"));
        assert!(fx.calls.contains(&"connection"));
        assert!(
            !fx.calls.contains(&"mods"),
            "no mods folder, nothing to apply"
        );
        assert!(spec.program.ends_with("bin/java"), "{:?}", spec.program);
    }

    #[test]
    fn the_server_chosen_on_the_servers_page_reaches_the_games_connection_settings() {
        use crate::servers::{self as page, Book, Draft};
        let w = world();
        let config = w.dir.path().join("config");
        let book = Book::new(config.join(page::FILE));
        let id = page::save(
            &book,
            &Draft {
                name: "Haven".into(),
                address: "haven.example".into(),
                port: "9001".into(),
                ..Draft::default()
            },
        )
        .unwrap();
        page::select(&book, &id).unwrap();

        // What a launch after a restart reads, then plans with.
        let around = Surroundings {
            server: page::selected_server_from(&config),
            ..around()
        };
        let mut fx = Recorder::default();
        prepare(&w.request, &w.machine, &around, &mut fx).unwrap();
        let sent = fx
            .connection
            .clone()
            .flatten()
            .expect("a server was passed on");
        assert_eq!((sent.host.as_str(), sent.port), ("haven.example", 9001));
        let props = servers::desired_properties(Some(&sent));
        assert_eq!(props["server_host"], "haven.example");
        assert_eq!(props["server_ports"], "9001");

        // Choosing Official again gives the game Official's settings, nothing of the private server.
        page::select(&book, page::OFFICIAL_ID).unwrap();
        let around = Surroundings {
            server: page::selected_server_from(&config),
            ..around
        };
        let mut fx = Recorder::default();
        prepare(&w.request, &w.machine, &around, &mut fx).unwrap();
        assert_eq!(fx.connection, Some(None));
    }

    #[test]
    fn mods_are_applied_before_the_server_settings() {
        let w = world();
        let mods = w.dir.path().join("mods");
        fs::create_dir_all(&mods).unwrap();
        let mut fx = Recorder::default();
        let around = Surroundings {
            mods_dir: Some(mods),
            ..around()
        };
        prepare(&w.request, &w.machine, &around, &mut fx).unwrap();
        let at = |name| fx.calls.iter().position(|c| *c == name).unwrap();
        assert!(at("mods") < at("connection") && at("connection") < at("spawn"));
    }

    #[test]
    fn a_pending_update_goes_through_the_updater_and_keeps_the_memory_limit_in_extra_txt() {
        let mut w = world();
        w.request.settings.memory_mb = Some(3072);
        let mut fx = Recorder::default();
        let around = Surroundings {
            known_remote_version: Some("99999999999".into()),
            ..around()
        };
        let (_, kind) = prepare(&w.request, &w.machine, &around, &mut fx).unwrap();
        assert_eq!(kind, LaunchKind::Updater);
        assert!(!fx.calls.contains(&"connection"));
        let extra = fs::read_to_string(w.request.install.path.join("extra.txt")).unwrap();
        assert!(extra.contains("-Xmx3072M"), "{extra}");
    }

    #[test]
    fn a_private_server_is_refused_while_the_game_needs_an_update() {
        let w = world();
        let mut fx = Recorder::default();
        let around = Surroundings {
            server: Some(private_server()),
            known_remote_version: Some("99999999999".into()),
            ..around()
        };
        let error = prepare(&w.request, &w.machine, &around, &mut fx).unwrap_err();
        assert!(matches!(error, LaunchError::Refused(_)), "{error:?}");
        assert!(error.message().contains("update"), "{}", error.message());
        assert!(fx.calls.is_empty(), "nothing may run: {:?}", fx.calls);
    }

    #[test]
    fn a_failed_step_stops_the_launch_before_the_game_starts() {
        let w = world();
        let mut fx = Recorder {
            fail_connection: true,
            ..Recorder::default()
        };
        let error = prepare(&w.request, &w.machine, &around(), &mut fx).unwrap_err();
        assert!(matches!(error, LaunchError::Prepare(_)), "{error:?}");
        assert!(error.message().contains("config.jar is locked"));
        assert!(!fx.calls.contains(&"spawn"));
    }

    #[test]
    fn graphics_enhancements_put_the_agent_into_the_direct_launch() {
        let mut w = world();
        w.request.settings.graphics_enabled = true;
        w.request.settings.render_scale = 150;
        let jar = w.dir.path().join("skgfx-agent.jar");
        fs::write(&jar, b"jar").unwrap();
        let mut fx = Recorder::default();
        let around = Surroundings {
            agent_jar: Some(jar.clone()),
            ..around()
        };
        let (spec, kind) = prepare(&w.request, &w.machine, &around, &mut fx).unwrap();
        assert_eq!(kind, LaunchKind::Direct);
        let agent = format!("-javaagent:{}", jar.display());
        assert!(spec.args.contains(&agent.into()), "{:?}", spec.args);
        assert!(spec.args.contains(&"-Dskgfx.scale=1.5".into()));
    }

    #[test]
    fn graphics_switched_off_means_no_agent_and_the_games_own_java() {
        let mut w = world();
        // The game's own trimmed runtime (no java.instrument) next to a full JDK on JAVA_HOME.
        let own = w.request.install.path.join("java_vm");
        fs::create_dir_all(own.join("bin")).unwrap();
        fs::write(
            own.join("release"),
            "JAVA_VERSION=\"25.0.3\"\nMODULES=\"java.base java.desktop\"\n",
        )
        .unwrap();
        fs::write(own.join("bin/java"), b"").unwrap();
        let jar = w.dir.path().join("skgfx-agent.jar");
        fs::write(&jar, b"jar").unwrap();
        let around = Surroundings {
            agent_jar: Some(jar),
            ..around()
        };
        // Off (the default), with options the player had set before: nothing of the agent is passed.
        assert!(!w.request.settings.graphics_enabled);
        w.request.settings.render_scale = 150;
        let (spec, _) = prepare(&w.request, &w.machine, &around, &mut Recorder::default()).unwrap();
        let args = arg_strings(&spec);
        assert!(!args.iter().any(|a| a.contains("skgfx")), "{args:?}");
        assert!(spec.program.starts_with(&own), "{:?}", spec.program);
        // On: the agent is passed and the JDK that can load it runs the game instead.
        w.request.settings.graphics_enabled = true;
        let (spec, _) = prepare(&w.request, &w.machine, &around, &mut Recorder::default()).unwrap();
        assert!(arg_strings(&spec)
            .iter()
            .any(|a| a.starts_with("-javaagent:")));
        assert!(!spec.program.starts_with(&own), "{:?}", spec.program);
        // Off with the player's own Java: that Java still wins.
        w.request.settings.graphics_enabled = false;
        w.request.settings.java_path = w.dir.path().join("jdk").display().to_string();
        let (spec, _) = prepare(&w.request, &w.machine, &around, &mut Recorder::default()).unwrap();
        // The player's path is resolved through links (macOS temp folders are one).
        let jdk = fs::canonicalize(w.dir.path().join("jdk")).unwrap();
        assert!(spec.program.starts_with(&jdk), "{:?}", spec.program);
    }

    #[test]
    fn the_players_chosen_java_that_is_missing_is_reported_by_name() {
        let mut w = world();
        w.request.settings.java_path = w.dir.path().join("nothing").display().to_string();
        let mut fx = Recorder::default();
        let error = prepare(&w.request, &w.machine, &around(), &mut fx).unwrap_err();
        assert!(matches!(error, LaunchError::CustomJavaInvalid { .. }));
        assert!(fx.calls.is_empty());
    }

    fn arg_strings(spec: &CommandSpec) -> Vec<String> {
        spec.args
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn the_jvm_settings_reach_the_game_command_in_a_fixed_order() {
        let mut w = world();
        w.request.settings.memory_mb = Some(6144);
        w.request.settings.collector = Collector::Zgc;
        w.request.settings.disable_explicit_gc = true;
        w.request.settings.extra_jvm_args = "-Dplayer=1 -Xss4m".into();
        // The game's own extra.txt names another collector and its own heap: both are replaced.
        fs::write(
            w.request.install.path.join("extra.txt"),
            "-XX:+UseG1GC\n-Xmx2g\n-Dgame=1\n",
        )
        .unwrap();
        let mut fx = Recorder::default();
        let (spec, kind) = prepare(&w.request, &w.machine, &around(), &mut fx).unwrap();
        assert_eq!(kind, LaunchKind::Direct);
        let args = arg_strings(&spec);
        let at = |flag: &str| {
            args.iter()
                .position(|a| a == flag)
                .unwrap_or_else(|| panic!("{flag} in {args:?}"))
        };
        assert_eq!(
            args.iter()
                .filter(|a| a.starts_with("-XX:+Use") && a.ends_with("GC"))
                .count(),
            1
        );
        assert!(!args.contains(&"-XX:+UseG1GC".to_owned()) && !args.contains(&"-Xmx2g".to_owned()));
        // game flags, then memory, collector, explicit GC, then the player's arguments last, then the class.
        assert!(at("-Dgame=1") < at("-Xmx6144M"));
        assert!(at("-Xmx6144M") < at("-XX:+UseZGC"));
        assert!(at("-XX:+UseZGC") < at("-XX:+DisableExplicitGC"));
        assert!(at("-XX:+DisableExplicitGC") < at("-Dplayer=1"));
        assert_eq!(args[at("-Dplayer=1") + 1], "-Xss4m");
        assert!(at("-Xss4m") < at("com.threerings.projectx.client.ProjectXApp"));
    }

    #[test]
    fn default_settings_use_the_recommended_memory_collector_and_explicit_gc() {
        let w = world();
        let mut fx = Recorder::default();
        let (spec, _) = prepare(&w.request, &w.machine, &around(), &mut fx).unwrap();
        let args = arg_strings(&spec);
        let collectors: Vec<_> = args
            .iter()
            .filter(|a| a.starts_with("-XX:+Use") && a.ends_with("GC"))
            .collect();
        assert_eq!(collectors, ["-XX:+UseG1GC"], "{args:?}");
        assert_eq!(
            args.iter()
                .filter(|a| *a == "-XX:+DisableExplicitGC")
                .count(),
            1
        );
        assert!(args.contains(&"-Xmx4096M".to_owned()), "{args:?}");
    }

    #[test]
    fn file_cleaning_and_purging_reach_the_mods_step() {
        let mods_for = |clean: bool, purge: bool| {
            let mut w = world();
            w.request.settings.file_cleaning = clean;
            w.request.settings.file_purging = purge;
            let mods = w.dir.path().join("mods");
            fs::create_dir_all(&mods).unwrap();
            let mut fx = Recorder::default();
            let around = Surroundings {
                mods_dir: Some(mods),
                ..around()
            };
            prepare(&w.request, &w.machine, &around, &mut fx).unwrap();
            fx.mod_options
        };
        let normal = mods_for(true, true);
        assert_eq!(normal, [ApplyOptions::default()]);
        let dirty = mods_for(false, false);
        assert_eq!(
            dirty,
            [ApplyOptions {
                keep_modified_originals: true,
                keep_added_files: true,
                ..ApplyOptions::default()
            }]
        );
        assert!(!mods_for(true, false)[0].keep_modified_originals);
        assert!(mods_for(true, false)[0].keep_added_files);
    }

    #[test]
    fn extra_jvm_arguments_split_like_a_shell_without_backslash_escapes() {
        assert_eq!(split_args("-Xss4m  -Dx=1"), ["-Xss4m", "-Dx=1"]);
        assert_eq!(
            split_args(r#"-javaagent:"C:\Program Files\a b.jar" -Dk='v w'"#),
            [r"-javaagent:C:\Program Files\a b.jar", "-Dk=v w"]
        );
        assert_eq!(split_args(r#"-a "" -b"#), ["-a", "", "-b"]);
        assert_eq!(split_args(r#"-a "b c"#), ["-a", "b c"]);
        assert!(split_args("   ").is_empty());
    }
}
