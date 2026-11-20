//! The launcher that follows the core's launch plan (`skradle_core::launch::plan`).
//!
//! The planner owns the order that must not be broken: an update first (through the game's own updater), otherwise
//! mods, then the server settings, then the game started directly. This module gathers what the planner needs,
//! runs the steps before the process through the real effects, and hands the final command to the supervisor,
//! which starts it and follows it. All of this happens on the supervisor's thread, so the window never waits.
//!
//! Where things live (none of it is created by the launch itself):
//! - the selected server: `servers.json` in the launcher's config folder (no file means Official),
//! - the player's mods: `<data dir>/SKradle/mods` (no folder means nothing to apply),
//! - the graphics agent: `<data dir>/SKradle/skgfx/skgfx-agent.jar` (no file means graphics enhancements are off).
//!   TODO(graphics): downloading the agent is not built yet (it needs the release pipeline of the `skgfx` repository).

use std::path::{Path, PathBuf};
use std::time::Duration;

use skradle_core::getdown::CommandSpec;
use skradle_core::graphics::{agent_args, GraphicsSettings};
use skradle_core::install::LATEST_GETDOWN_TXT_URL;
use skradle_core::launch::direct::{DirectOptions, GarbageCollector, Inputs, Memory};
use skradle_core::launch::plan::{self as core_plan, Effects, LaunchKind, PlanInputs, RealEffects};
use skradle_core::mods::ApplyOptions;
use skradle_core::net;
use skradle_core::runtime::{self, Runtime};
use skradle_core::servers::{self, Server, ServerList};

use super::plan::{self, Machine};
use super::supervisor::{self, Prepare, Timing};
use super::{memory, LaunchError, Launcher, Request, Running, Sink};
use crate::settings::{self, Collector, Settings};

/// What a launch depends on besides the game folder and the settings.
#[derive(Debug, Clone, Default)]
pub struct Surroundings {
    pub arch: String,
    pub server: Option<Server>,
    pub mods_dir: Option<PathBuf>,
    pub agent_jar: Option<PathBuf>,
    /// The newest `getdown.txt` from the game's server; `None` when it could not be fetched (offline), which counts
    /// as "no update" so a private server still launches.
    pub remote_getdown: Option<String>,
}

impl Surroundings {
    /// Looks around on this computer. Fetches the remote `getdown.txt` (a few seconds at most).
    pub fn detect() -> Self {
        let data = dirs::data_dir().map(|d| d.join("SKradle"));
        Self {
            arch: std::env::consts::ARCH.to_owned(),
            server: ServerList::load(&settings::config_dir().join("servers.json"))
                .selected_server()
                .cloned(),
            mods_dir: data.as_ref().map(|d| d.join("mods")).filter(|p| p.is_dir()),
            agent_jar: data
                .as_ref()
                .map(|d| d.join("skgfx").join("skgfx-agent.jar"))
                .filter(|p| p.is_file()),
            remote_getdown: net::Client::with_timeouts(
                Duration::from_secs(4),
                Duration::from_secs(8),
            )
            .get_text(LATEST_GETDOWN_TXT_URL, 256 * 1024)
            .ok(),
        }
    }
}

/// The planner's effects, except that "start the game" only hands the command back: the supervisor starts it in
/// its own way (output captured, its own process group).
struct Deferred(RealEffects);

impl Effects for Deferred {
    type Handle = CommandSpec;

    fn clean_extra_txt(&mut self, game_root: &Path) -> Result<(), String> {
        self.0.clean_extra_txt(game_root)
    }

    fn apply_mods(
        &mut self,
        game_root: &Path,
        mods_dir: &Path,
        options: ApplyOptions,
    ) -> Result<Vec<String>, String> {
        self.0.apply_mods(game_root, mods_dir, options)
    }

    fn apply_connection(
        &mut self,
        game_root: &Path,
        server: Option<&Server>,
    ) -> Result<(), String> {
        self.0.apply_connection(game_root, server)
    }

    fn spawn(&mut self, command: &CommandSpec) -> Result<CommandSpec, String> {
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
    let graphics = around
        .agent_jar
        .as_deref()
        .map(|jar| agent_args(jar, &graphics_settings(settings)));
    let update_pending = around
        .remote_getdown
        .as_deref()
        .is_some_and(|remote| servers::update_pending(&game_files.getdown_txt, remote));

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
    })
    .map_err(|refusal| LaunchError::Refused(refusal.to_string()))?;
    for warning in &plan.warnings {
        eprintln!("skradle: {warning}");
    }
    let launched = core_plan::execute(&plan, effects)
        .map_err(|failure| LaunchError::Prepare(failure.message))?;
    for note in &launched.notes {
        eprintln!("skradle: {note}");
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
        let prepared: Prepare = Box::new(move || {
            let machine = Machine::current();
            let around = Surroundings::detect();
            prepare(&request, &machine, &around, &mut Deferred(RealEffects))
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
        fn apply_connection(&mut self, _: &Path, _: Option<&Server>) -> Result<(), String> {
            self.calls.push("connection");
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

    /// A `getdown.txt` with a different version than the installed one.
    fn newer_getdown() -> String {
        GETDOWN
            .lines()
            .map(|l| {
                if l.trim_start().starts_with("version") {
                    "version = 99999999999".to_owned()
                } else {
                    l.to_owned()
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
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
            remote_getdown: Some(newer_getdown()),
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
            remote_getdown: Some(newer_getdown()),
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
    fn default_settings_add_no_collector_and_no_explicit_gc_flag() {
        let w = world();
        let mut fx = Recorder::default();
        let (spec, _) = prepare(&w.request, &w.machine, &around(), &mut fx).unwrap();
        let args = arg_strings(&spec);
        assert!(
            !args
                .iter()
                .any(|a| a.starts_with("-XX:+Use") && a.ends_with("GC")),
            "{args:?}"
        );
        assert!(!args.contains(&"-XX:+DisableExplicitGC".to_owned()));
        assert!(
            !args.iter().any(|a| a.starts_with("-Xmx")),
            "no memory chosen, none added"
        );
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
