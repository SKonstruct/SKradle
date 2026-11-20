//! Planning a launch. Several pieces must run in a fixed order and each ordering has been found to fail silently when
//! broken, so the order lives here, in one place, with tests:
//!
//! 1. Is an update pending (or the game's files incomplete)? Then the game's own updater runs: it verifies and
//!    repairs the game, replacing every file mods changed, and starts the game once on the game's own runtime.
//!    A private server is never launched in this state, because the updater would restore `config.jar` and the game
//!    would connect to Official.
//! 2. Otherwise: apply the mods, THEN point `config.jar` at the chosen server (mods rebuild that jar from the game's
//!    original and would discard the server settings), THEN start the client directly. The updater must not start
//!    the game after mods are applied: it would see the changed files and put the originals back.
//! 3. The graphics agent needs a Java that can load it, which the game's trimmed runtime is not; the direct launch
//!    uses one that can, or goes on without the agent (with a warning) when there is none.
//!
//! [`plan`] only decides; [`execute`] runs the steps through an [`Effects`] implementation and stops at the first
//! failure, so nothing launches on top of a half-applied state. [`RealEffects`] is the real implementation.

use std::path::{Path, PathBuf};
use std::process::Child;

use crate::extra_txt;
use crate::game::{GameInstall, Os};
use crate::getdown::{update_and_launch, CommandSpec};
use crate::getdown_launch::{LaunchConfig, Target};
use crate::getdown_txt::GetdownConfig;
use crate::launch::direct::{self, DirectOptions, Inputs};
use crate::mods::ApplyOptions;
use crate::runtime::{pick_for_game, pick_for_graphics_agent, Runtime};
use crate::servers::{self, Server};

/// Everything the planner needs; gather it with the real modules (see `docs/launching.md`).
pub struct PlanInputs<'a> {
    pub install: &'a GameInstall,
    pub os: Os,
    /// The CPU as `std::env::consts::ARCH` spells it.
    pub arch: &'a str,
    /// Every Java found, best source first (`runtime::find_runtimes`).
    pub runtimes: &'a [Runtime],
    /// The game's `getdown.txt` and `extra.txt`.
    pub game_files: &'a Inputs,
    /// The selected private server; `None` is Official.
    pub server: Option<&'a Server>,
    /// The folder of the player's mods; `None` when there is none to apply.
    pub mods_dir: Option<&'a Path>,
    /// The graphics agent's JVM arguments (`-javaagent:...` and the `-Dskgfx.*` settings); `None` when graphics
    /// enhancements are off.
    pub graphics_args: Option<&'a [String]>,
    /// Official has a newer game than the installed one (`servers::update_pending`; an unreachable server is false).
    pub update_pending: bool,
    /// Memory and extra JVM arguments the player chose. On the updater path only `jvm_args` are used (for the
    /// updater's own JVM; the updater starts the game with the game's own settings, so `memory` waits for the next,
    /// direct launch). On the direct path `skip_extra_line` is replaced by the planner's own rule for the graphics
    /// module's lines.
    pub options: &'a DirectOptions,
    /// How mods are applied (file cleaning and purging).
    pub mod_options: ApplyOptions,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchKind {
    /// Through the game's updater.
    Updater,
    /// The client directly.
    Direct,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// Remove the graphics module's lines from `extra.txt`, so a stale agent line cannot stop the game's own runtime.
    CleanExtraTxt { game_root: PathBuf },
    ApplyMods {
        game_root: PathBuf,
        mods_dir: PathBuf,
        options: ApplyOptions,
    },
    ApplyConnection {
        game_root: PathBuf,
        server: Option<Server>,
    },
    Spawn {
        kind: LaunchKind,
        command: CommandSpec,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Warning {
    /// Mods were not applied: the updater is about to restore the game's own files.
    ModsSkippedForUpdate,
    /// The graphics agent was not loaded because the updater starts the game on its own runtime.
    AgentSkippedForUpdate,
    /// Graphics enhancements are on but no Java that can load the agent is available.
    AgentRuntimeUnavailable,
    /// The game's files were incomplete, so the updater repairs them first.
    RepairingGameFiles(Vec<String>),
}

impl std::fmt::Display for Warning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Warning::ModsSkippedForUpdate => write!(f, "Mods are applied on the next launch, after the game has updated."),
            Warning::AgentSkippedForUpdate => write!(f, "Graphics enhancements start on the next launch, after the game has updated."),
            Warning::AgentRuntimeUnavailable => write!(f, "Graphics enhancements need Java 24 or newer, which was not found; starting without them."),
            Warning::RepairingGameFiles(files) => write!(f, "Repairing the game first ({} files missing).", files.len()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub steps: Vec<Step>,
    pub warnings: Vec<Warning>,
}

/// Why no launch could be planned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// No Java satisfies the game's minimum (the number is the updater's encoding, `25000000`).
    NoJava { minimum: u64 },
    /// A private server was chosen but the game must update first; update it, then launch.
    UpdateRequiredForPrivateServer,
    /// A private server was chosen but game files are missing; let the updater repair the game first.
    GameFilesIncompleteForPrivateServer(Vec<String>),
    /// The game's `getdown.txt` could not be understood.
    Config(String),
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Refusal::NoJava { minimum } => {
                write!(f, "No Java {} or newer was found.", minimum / 1_000_000)
            }
            Refusal::UpdateRequiredForPrivateServer => write!(
                f,
                "The game has an update. Update it before joining a private server."
            ),
            Refusal::GameFilesIncompleteForPrivateServer(files) => write!(
                f,
                "Game files are missing ({}). Repair the game before joining a private server.",
                files.len()
            ),
            Refusal::Config(why) => write!(f, "The game's configuration could not be read: {why}"),
        }
    }
}

impl std::error::Error for Refusal {}

/// Decides how to launch. See the module documentation for the rules.
pub fn plan(inputs: &PlanInputs<'_>) -> Result<Plan, Refusal> {
    let minimum = GetdownConfig::parse(&inputs.game_files.getdown_txt)
        .java_version
        .unwrap_or(0);
    let game_java = pick_for_game(inputs.runtimes, minimum).ok_or(Refusal::NoJava { minimum })?;
    let target = Target::for_os(inputs.os, inputs.arch);
    let launch = LaunchConfig::parse(&inputs.game_files.getdown_txt, &target)
        .app(direct::CLIENT_APP_ID)
        .map_err(|e| Refusal::Config(e.to_string()))?;
    let missing = direct::missing_code_files(&inputs.install.root, &launch);
    let game_root = inputs.install.root.clone();

    if inputs.update_pending || !missing.is_empty() {
        if inputs.server.is_some() {
            return Err(if inputs.update_pending {
                Refusal::UpdateRequiredForPrivateServer
            } else {
                Refusal::GameFilesIncompleteForPrivateServer(missing)
            });
        }
        let mut warnings = Vec::new();
        if !missing.is_empty() {
            warnings.push(Warning::RepairingGameFiles(missing));
        }
        if inputs.mods_dir.is_some() {
            warnings.push(Warning::ModsSkippedForUpdate);
        }
        if inputs.graphics_args.is_some() {
            warnings.push(Warning::AgentSkippedForUpdate);
        }
        let jvm_args = inputs
            .options
            .jvm_args
            .iter()
            .map(std::ffi::OsString::from)
            .collect::<Vec<_>>();
        let command = update_and_launch(
            inputs.install,
            &game_java.java_executable(inputs.os),
            &jvm_args,
        );
        return Ok(Plan {
            steps: vec![
                Step::CleanExtraTxt { game_root },
                Step::Spawn {
                    kind: LaunchKind::Updater,
                    command,
                },
            ],
            warnings,
        });
    }

    let mut warnings = Vec::new();
    let mut options = inputs.options.clone();
    // The agent's lines are passed as arguments (below) or not at all; never through the game's own extra.txt.
    options.skip_extra_line = Some(extra_txt::is_skgfx_line);
    let java = match inputs.graphics_args {
        None => game_java,
        Some(args) => match pick_for_graphics_agent(inputs.runtimes, minimum) {
            Some(agent_java) => {
                options.jvm_args.extend(args.iter().cloned());
                agent_java
            }
            None => {
                warnings.push(Warning::AgentRuntimeUnavailable);
                game_java
            }
        },
    };
    let command = direct::command(
        inputs.install,
        &java.java_executable(inputs.os),
        inputs.game_files,
        &options,
        inputs.os,
        inputs.arch,
    )
    .map_err(|e| Refusal::Config(e.to_string()))?;

    let mut steps = Vec::new();
    if let Some(mods_dir) = inputs.mods_dir {
        steps.push(Step::ApplyMods {
            game_root: game_root.clone(),
            mods_dir: mods_dir.to_path_buf(),
            options: inputs.mod_options,
        });
    }
    // After the mods: applying them rebuilds config.jar from the game's original.
    steps.push(Step::ApplyConnection {
        game_root,
        server: inputs.server.cloned(),
    });
    steps.push(Step::Spawn {
        kind: LaunchKind::Direct,
        command,
    });
    Ok(Plan { steps, warnings })
}

/// The side effects of a launch, separated so the order and the failure handling can be tested.
pub trait Effects {
    /// What a started game is (a child process for the real implementation).
    type Handle;

    fn clean_extra_txt(&mut self, game_root: &Path) -> Result<(), String>;
    /// Returns notes (skipped mods, warnings) for the player.
    fn apply_mods(
        &mut self,
        game_root: &Path,
        mods_dir: &Path,
        options: ApplyOptions,
    ) -> Result<Vec<String>, String>;
    fn apply_connection(&mut self, game_root: &Path, server: Option<&Server>)
        -> Result<(), String>;
    fn spawn(&mut self, command: &CommandSpec) -> Result<Self::Handle, String>;
}

#[derive(Debug)]
pub struct Launched<H> {
    pub handle: H,
    pub kind: LaunchKind,
    /// Notes from applying mods.
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecError {
    /// The index in `Plan::steps` of the step that failed.
    pub step: usize,
    pub message: String,
}

impl std::fmt::Display for ExecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ExecError {}

/// Runs the plan's steps in order and stops at the first failure: nothing after a failed step runs, and the game is
/// only started when every step before it succeeded.
pub fn execute<E: Effects>(plan: &Plan, effects: &mut E) -> Result<Launched<E::Handle>, ExecError> {
    let mut notes = Vec::new();
    for (index, step) in plan.steps.iter().enumerate() {
        let fail = |message: String| ExecError {
            step: index,
            message,
        };
        match step {
            Step::CleanExtraTxt { game_root } => {
                effects.clean_extra_txt(game_root).map_err(fail)?
            }
            Step::ApplyMods {
                game_root,
                mods_dir,
                options,
            } => notes.extend(
                effects
                    .apply_mods(game_root, mods_dir, *options)
                    .map_err(fail)?,
            ),
            Step::ApplyConnection { game_root, server } => effects
                .apply_connection(game_root, server.as_ref())
                .map_err(fail)?,
            Step::Spawn { kind, command } => {
                let handle = effects.spawn(command).map_err(fail)?;
                return Ok(Launched {
                    handle,
                    kind: *kind,
                    notes,
                });
            }
        }
    }
    Err(ExecError {
        step: plan.steps.len(),
        message: "the plan has no step that starts the game".into(),
    })
}

/// The real effects: the mods engine, the connection writer, `extra.txt` and a child process.
#[derive(Debug, Default)]
pub struct RealEffects;

impl Effects for RealEffects {
    type Handle = Child;

    fn clean_extra_txt(&mut self, game_root: &Path) -> Result<(), String> {
        extra_txt::update_file(&game_root.join("extra.txt"), &[], extra_txt::is_skgfx_line)
            .map_err(|e| format!("could not clean extra.txt: {e}"))
    }

    fn apply_mods(
        &mut self,
        game_root: &Path,
        mods_dir: &Path,
        options: ApplyOptions,
    ) -> Result<Vec<String>, String> {
        let report = crate::mods::apply_with_options(game_root, mods_dir, options)
            .map_err(|e| e.to_string())?;
        let mut notes = report.warnings;
        notes.extend(
            report
                .skipped
                .iter()
                .map(|s| format!("{}: {}", s.file_name, s.reason)),
        );
        Ok(notes)
    }

    fn apply_connection(
        &mut self,
        game_root: &Path,
        server: Option<&Server>,
    ) -> Result<(), String> {
        servers::apply_connection(game_root, server)
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    fn spawn(&mut self, command: &CommandSpec) -> Result<Child, String> {
        command
            .to_command()
            .spawn()
            .map_err(|e| format!("could not start the game: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::InstallKind;
    use crate::runtime::{JavaRelease, Source};
    use std::fs;

    const GETDOWN: &str = include_str!("../../tests/fixtures/getdown.txt");

    struct Fixture {
        dir: tempfile::TempDir,
        install: GameInstall,
        files: Inputs,
        runtimes: Vec<Runtime>,
        options: DirectOptions,
    }

    fn runtime(home: &str, release: &str, source: Source) -> Runtime {
        Runtime {
            home: PathBuf::from(home),
            release: JavaRelease::parse(release).unwrap(),
            source,
        }
    }

    const TRIMMED: &str = "JAVA_VERSION=\"25.0.1\"\nMODULES=\"java.base java.desktop\"\n";
    const FULL: &str =
        "JAVA_VERSION=\"25.0.3\"\nMODULES=\"java.base java.desktop java.instrument\"\n";

    /// A game folder with every classpath jar the fixture names (for all platforms).
    fn fixture() -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("code")).unwrap();
        for line in GETDOWN.lines().filter_map(|l| l.strip_prefix("code = ")) {
            let name = line.rsplit(' ').next().unwrap();
            fs::write(dir.path().join(name), b"jar").unwrap();
        }
        let install = GameInstall {
            root: dir.path().to_path_buf(),
            kind: InstallKind::Steam,
        };
        Fixture {
            dir,
            install,
            files: Inputs {
                getdown_txt: GETDOWN.to_owned(),
                extra_txt: Some(
                    "-Xmx2g\n-Dskgfx.scale=2\n-javaagent:/old/skgfx-agent.jar\n".into(),
                ),
            },
            runtimes: vec![
                runtime("/game/java_vm", TRIMMED, Source::GameBundled),
                runtime("/managed/jdk25", FULL, Source::Managed),
            ],
            options: DirectOptions::default(),
        }
    }

    fn inputs<'a>(
        f: &'a Fixture,
        server: Option<&'a Server>,
        mods: Option<&'a Path>,
        graphics: Option<&'a [String]>,
        pending: bool,
    ) -> PlanInputs<'a> {
        PlanInputs {
            install: &f.install,
            os: Os::Linux,
            arch: "x86_64",
            runtimes: &f.runtimes,
            game_files: &f.files,
            server,
            mods_dir: mods,
            graphics_args: graphics,
            update_pending: pending,
            options: &f.options,
            mod_options: ApplyOptions::default(),
        }
    }

    fn server() -> Server {
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

    fn kinds(plan: &Plan) -> Vec<&'static str> {
        plan.steps
            .iter()
            .map(|s| match s {
                Step::CleanExtraTxt { .. } => "clean",
                Step::ApplyMods { .. } => "mods",
                Step::ApplyConnection { .. } => "connection",
                Step::Spawn {
                    kind: LaunchKind::Updater,
                    ..
                } => "updater",
                Step::Spawn {
                    kind: LaunchKind::Direct,
                    ..
                } => "direct",
            })
            .collect()
    }

    fn spawn_of(plan: &Plan) -> &CommandSpec {
        plan.steps
            .iter()
            .find_map(|s| {
                if let Step::Spawn { command, .. } = s {
                    Some(command)
                } else {
                    None
                }
            })
            .unwrap()
    }

    fn args(command: &CommandSpec) -> Vec<String> {
        command
            .args
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn a_pending_update_runs_the_updater_on_the_games_runtime() {
        let f = fixture();
        let mods = f.dir.path().join("mods");
        let agent = vec!["-javaagent:/a/skgfx-agent.jar".to_owned()];
        let p = plan(&inputs(&f, None, Some(&mods), Some(&agent), true)).unwrap();
        assert_eq!(kinds(&p), ["clean", "updater"]);
        let command = spawn_of(&p);
        assert_eq!(command.program, Path::new("/game/java_vm/bin/java"));
        assert!(args(command).contains(&"getdown-pro.jar".to_owned()));
        assert_eq!(
            p.warnings,
            [
                Warning::ModsSkippedForUpdate,
                Warning::AgentSkippedForUpdate
            ]
        );
    }

    #[test]
    fn a_private_server_is_never_launched_while_an_update_is_pending() {
        let f = fixture();
        let s = server();
        assert_eq!(
            plan(&inputs(&f, Some(&s), None, None, true)).unwrap_err(),
            Refusal::UpdateRequiredForPrivateServer
        );
    }

    #[test]
    fn incomplete_game_files_go_through_the_updater_unless_a_private_server_is_chosen() {
        let f = fixture();
        fs::remove_file(f.dir.path().join("code/projectx-pcode.jar")).unwrap();
        let p = plan(&inputs(&f, None, None, None, false)).unwrap();
        assert_eq!(kinds(&p), ["clean", "updater"]);
        assert!(
            matches!(p.warnings.as_slice(), [Warning::RepairingGameFiles(files)] if files == &["code/projectx-pcode.jar"])
        );
        let s = server();
        assert!(matches!(
            plan(&inputs(&f, Some(&s), None, None, false)),
            Err(Refusal::GameFilesIncompleteForPrivateServer(_))
        ));
    }

    #[test]
    fn an_up_to_date_game_applies_mods_then_the_server_then_launches_directly() {
        let f = fixture();
        let mods = f.dir.path().join("mods");
        let s = server();
        let p = plan(&inputs(&f, Some(&s), Some(&mods), None, false)).unwrap();
        assert_eq!(
            kinds(&p),
            ["mods", "connection", "direct"],
            "mods must come before the connection settings"
        );
        assert!(
            matches!(&p.steps[1], Step::ApplyConnection { server: Some(srv), .. } if srv.id == "a")
        );
        assert!(p.warnings.is_empty());
        let command = spawn_of(&p);
        assert_eq!(
            command.program,
            Path::new("/game/java_vm/bin/java"),
            "no graphics: the game's own runtime"
        );
        assert!(
            args(command).contains(&"-classpath".to_owned()),
            "a direct launch has a classpath and no updater jar"
        );
        assert!(!args(command).contains(&"getdown-pro.jar".to_owned()));
    }

    #[test]
    fn the_connection_is_written_even_without_mods_or_a_server_to_heal_the_jar() {
        let f = fixture();
        let p = plan(&inputs(&f, None, None, None, false)).unwrap();
        assert_eq!(kinds(&p), ["connection", "direct"]);
        assert!(matches!(
            &p.steps[0],
            Step::ApplyConnection { server: None, .. }
        ));
    }

    #[test]
    fn graphics_enhancements_use_a_runtime_that_can_load_the_agent_and_ignore_stale_extra_lines() {
        let f = fixture();
        let agent = vec![
            "-javaagent:/new/skgfx-agent.jar".to_owned(),
            "-Dskgfx.scale=1.5".to_owned(),
        ];
        let p = plan(&inputs(&f, None, None, Some(&agent), false)).unwrap();
        let command = spawn_of(&p);
        assert_eq!(
            command.program,
            Path::new("/managed/jdk25/bin/java"),
            "the full runtime, not the trimmed one"
        );
        let a = args(command);
        assert!(
            a.contains(&"-javaagent:/new/skgfx-agent.jar".to_owned())
                && a.contains(&"-Dskgfx.scale=1.5".to_owned())
        );
        assert!(
            !a.contains(&"-javaagent:/old/skgfx-agent.jar".to_owned()),
            "the stale line from extra.txt is dropped"
        );
        assert!(!a.contains(&"-Dskgfx.scale=2".to_owned()));
        assert!(
            a.contains(&"-Xmx2g".to_owned()),
            "the player's own extra.txt lines stay"
        );
        assert!(p.warnings.is_empty());
    }

    #[test]
    fn without_a_capable_runtime_the_game_starts_without_the_agent_and_says_so() {
        let mut f = fixture();
        f.runtimes.truncate(1);
        let agent = vec!["-javaagent:/new/skgfx-agent.jar".to_owned()];
        let p = plan(&inputs(&f, None, None, Some(&agent), false)).unwrap();
        assert_eq!(p.warnings, [Warning::AgentRuntimeUnavailable]);
        let a = args(spawn_of(&p));
        assert!(
            !a.iter().any(|x| x.contains("skgfx")),
            "no agent and no stale skgfx lines at all"
        );
    }

    #[test]
    fn no_suitable_java_is_refused() {
        let mut f = fixture();
        f.runtimes = vec![runtime("/old", "JAVA_VERSION=\"17.0.1\"\n", Source::Path)];
        assert_eq!(
            plan(&inputs(&f, None, None, None, false)).unwrap_err(),
            Refusal::NoJava {
                minimum: 25_000_000
            }
        );
    }

    #[test]
    fn an_unreadable_config_is_refused() {
        let mut f = fixture();
        f.files.getdown_txt = "version = 1\njava_version = 25000000\n".into();
        assert!(matches!(
            plan(&inputs(&f, None, None, None, false)),
            Err(Refusal::Config(_))
        ));
    }

    /// Records every effect in order; can fail at a chosen one.
    #[derive(Default)]
    struct Fake {
        log: Vec<String>,
        fail_at: Option<&'static str>,
    }

    impl Fake {
        fn step(&mut self, name: &'static str) -> Result<(), String> {
            self.log.push(name.to_owned());
            if self.fail_at == Some(name) {
                Err(format!("{name} failed"))
            } else {
                Ok(())
            }
        }
    }

    impl Effects for Fake {
        type Handle = u32;
        fn clean_extra_txt(&mut self, _: &Path) -> Result<(), String> {
            self.step("clean")
        }
        fn apply_mods(
            &mut self,
            _: &Path,
            _: &Path,
            _: ApplyOptions,
        ) -> Result<Vec<String>, String> {
            self.step("mods").map(|()| vec!["a note".to_owned()])
        }
        fn apply_connection(&mut self, _: &Path, _: Option<&Server>) -> Result<(), String> {
            self.step("connection")
        }
        fn spawn(&mut self, _: &CommandSpec) -> Result<u32, String> {
            self.step("spawn").map(|()| 42)
        }
    }

    fn full_plan(f: &Fixture) -> Plan {
        let mods = f.dir.path().join("mods");
        plan(&inputs(f, None, Some(&mods), None, false)).unwrap()
    }

    #[test]
    fn effects_run_in_the_planned_order_and_the_game_starts_last() {
        let f = fixture();
        let mut fake = Fake::default();
        let launched = execute(&full_plan(&f), &mut fake).unwrap();
        assert_eq!(fake.log, ["mods", "connection", "spawn"]);
        assert_eq!((launched.handle, launched.kind), (42, LaunchKind::Direct));
        assert_eq!(launched.notes, ["a note"]);
    }

    #[test]
    fn a_failing_step_stops_everything_after_it_and_the_game_never_starts() {
        let f = fixture();
        for failing in ["mods", "connection"] {
            let mut fake = Fake {
                fail_at: Some(failing),
                ..Fake::default()
            };
            let err = execute(&full_plan(&f), &mut fake).unwrap_err();
            assert_eq!(err.message, format!("{failing} failed"));
            assert!(
                !fake.log.contains(&"spawn".to_owned()),
                "nothing launches on a half-applied state ({failing})"
            );
            assert_eq!(
                fake.log.last().map(String::as_str),
                Some(failing),
                "no step runs after the failure"
            );
        }
    }

    #[test]
    fn the_updater_path_cleans_extra_txt_then_spawns() {
        let f = fixture();
        let p = plan(&inputs(&f, None, None, None, true)).unwrap();
        let mut fake = Fake::default();
        let launched = execute(&p, &mut fake).unwrap();
        assert_eq!(fake.log, ["clean", "spawn"]);
        assert_eq!(launched.kind, LaunchKind::Updater);
    }

    #[test]
    fn an_empty_plan_is_an_error_not_a_silent_success() {
        let mut fake = Fake::default();
        let err = execute(
            &Plan {
                steps: vec![],
                warnings: vec![],
            },
            &mut fake,
        )
        .unwrap_err();
        assert!(err.message.contains("no step"));
        assert!(fake.log.is_empty());
    }
}
