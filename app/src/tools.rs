//! What the Settings page's Game and Tools sections need to know about this machine, as plain Rust with tests:
//! which Java the game would get and whether the graphics agent can load in it, why an editor cannot start, and
//! starting an editor. The sentences live in QML (`qsTr`); this module only produces codes.

use std::path::Path;

use skradle_core::editors::{self, Editor};
use skradle_core::game::GameInstall;
use skradle_core::launch::direct::Inputs;
use skradle_core::runtime::{self, Runtime, Source};

use crate::discovery::{Install, Kind};
use crate::launch::plan::{self, Machine};
use crate::launch::LaunchError;

/// Whether the graphics agent can load in the Java the game would get.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentJava {
    /// A runtime that satisfies the game and supports the agent (Java 24+ with `java.instrument`) exists.
    Ok,
    /// No such runtime was found (the game's own trimmed runtime cannot load it).
    Missing,
}

/// Why no usable Java was found, when none was.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JavaProblem {
    None,
    /// No Java at all, or none that satisfies the game (`needed_major` is the game's minimum).
    NotFound,
    /// The path the player entered is not a Java.
    CustomInvalid,
    /// The player's Java is older than the game needs.
    CustomTooOld {
        found: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JavaInfo {
    /// `25.0.3`, empty when there is no usable Java.
    pub version: String,
    /// `game`, `managed`, `env`, `path`, `custom` or empty.
    pub source: &'static str,
    pub problem: JavaProblem,
    /// The game's minimum Java major version (0 when `getdown.txt` does not say).
    pub needed_major: u64,
    pub agent: AgentJava,
}

fn source_name(source: Source) -> &'static str {
    match source {
        Source::GameBundled => "game",
        Source::Managed => "managed",
        Source::JavaHome => "env",
        Source::Path => "path",
    }
}

fn version_text(runtime: &Runtime) -> String {
    let v = runtime.release.version;
    format!("{}.{}.{}", v.major, v.minor, v.patch)
}

/// The Java a launch of the game in `game_root` would use (the player's own when `java_override` is set) and whether
/// the graphics agent can load in it.
pub fn java_info(machine: &Machine, game_root: &Path, java_override: &str) -> JavaInfo {
    let minimum = plan::java_minimum(game_root);
    let install = Install {
        path: game_root.to_path_buf(),
        kind: Kind::Custom,
    };
    let needed_major = minimum / 1_000_000;
    let custom = !java_override.trim().is_empty();
    let all = runtime::find_runtimes(
        &machine.runtime_env,
        Some(game_root),
        machine.managed_dir.as_deref(),
    );
    match plan::pick_java(machine, &install, java_override, minimum) {
        Ok(chosen) => {
            let pool = if custom {
                std::slice::from_ref(&chosen)
            } else {
                &all[..]
            };
            let agent = if runtime::pick_for_graphics_agent(pool, minimum).is_some() {
                AgentJava::Ok
            } else {
                AgentJava::Missing
            };
            JavaInfo {
                version: version_text(&chosen),
                source: if custom {
                    "custom"
                } else {
                    source_name(chosen.source)
                },
                problem: JavaProblem::None,
                needed_major,
                agent,
            }
        }
        Err(error) => {
            let problem = match error {
                LaunchError::CustomJavaInvalid { .. } => JavaProblem::CustomInvalid,
                LaunchError::CustomJavaTooOld { found, .. } => JavaProblem::CustomTooOld { found },
                _ => JavaProblem::NotFound,
            };
            JavaInfo {
                version: String::new(),
                source: "",
                problem,
                needed_major,
                agent: AgentJava::Missing,
            }
        }
    }
}

/// Why an editor cannot start now: `"game"` (no game), `"java"` (no usable Java) or `"files"` (the game folder lacks
/// what the editor needs); `None` when it can.
pub fn editor_blocker(
    game_root: Option<&Path>,
    java: &JavaInfo,
    editor: Editor,
) -> Option<&'static str> {
    let Some(root) = game_root else {
        return Some("game");
    };
    if java.problem != JavaProblem::None {
        return Some("java");
    }
    let install = GameInstall {
        root: root.to_path_buf(),
        kind: skradle_core::game::InstallKind::Standalone,
    };
    if !editors::missing_resources(&install, editor).is_empty() {
        return Some("files");
    }
    None
}

/// Starts an editor detached from the launcher.
pub fn launch_editor(
    machine: &Machine,
    game_root: &Path,
    java_override: &str,
    editor: Editor,
) -> Result<(), String> {
    let install = Install {
        path: game_root.to_path_buf(),
        kind: Kind::Custom,
    };
    let java = plan::pick_java(
        machine,
        &install,
        java_override,
        plan::java_minimum(game_root),
    )
    .map_err(|e| e.message())?;
    let core_install = install.to_core();
    let inputs =
        Inputs::read(&core_install).map_err(|e| format!("could not read the game's files: {e}"))?;
    let command = editors::editor_command_for(
        &core_install,
        &java.java_executable(machine.os),
        editor,
        &inputs,
        machine.os,
        std::env::consts::ARCH,
    )
    .map_err(|e| e.to_string())?;
    let mut process = command.to_command();
    process
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    process
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("could not start the editor: {e}"))
}

/// Opens `folder` in the system's file manager. Never goes through a shell.
pub fn open_folder(folder: &Path) -> Result<(), String> {
    if !folder.is_dir() {
        return Err("that folder does not exist".to_owned());
    }
    let program = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(windows) {
        "explorer"
    } else {
        "xdg-open"
    };
    std::process::Command::new(program)
        .arg(folder)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("could not open the folder: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use skradle_core::game::Os;
    use skradle_core::runtime::RuntimeEnv;
    use std::fs;

    const GETDOWN: &str = include_str!("../../core/tests/fixtures/getdown.txt");
    const TRIMMED: &str = "JAVA_VERSION=\"25.0.1\"\nMODULES=\"java.base java.desktop\"\n";
    const FULL: &str =
        "JAVA_VERSION=\"25.0.3\"\nMODULES=\"java.base java.desktop java.instrument\"\n";
    const OLD: &str = "JAVA_VERSION=\"17.0.9\"\nMODULES=\"java.base java.instrument\"\n";

    fn make_java(home: &Path, release: &str) {
        fs::create_dir_all(home.join("bin")).unwrap();
        fs::write(home.join("release"), release).unwrap();
        fs::write(home.join("bin/java"), b"").unwrap();
    }

    struct World {
        dir: tempfile::TempDir,
        game: std::path::PathBuf,
        machine: Machine,
    }

    fn world(game_java: Option<&str>, env_java: Option<&str>) -> World {
        let dir = tempfile::tempdir().unwrap();
        let game = dir.path().join("game");
        fs::create_dir_all(&game).unwrap();
        fs::write(game.join("getdown.txt"), GETDOWN).unwrap();
        if let Some(release) = game_java {
            make_java(&game.join("java_vm"), release);
        }
        let mut env = RuntimeEnv::default();
        if let Some(release) = env_java {
            make_java(&dir.path().join("jdk"), release);
            env.java_home = Some(dir.path().join("jdk"));
        }
        World {
            machine: Machine {
                os: Os::Linux,
                runtime_env: env,
                managed_dir: None,
            },
            game,
            dir,
        }
    }

    #[test]
    fn the_games_trimmed_runtime_runs_the_game_but_cannot_load_the_agent() {
        let w = world(Some(TRIMMED), None);
        let info = java_info(&w.machine, &w.game, "");
        assert_eq!((info.version.as_str(), info.source), ("25.0.1", "game"));
        assert_eq!(info.problem, JavaProblem::None);
        assert_eq!(info.agent, AgentJava::Missing);
        assert_eq!(info.needed_major, 25);
    }

    #[test]
    fn a_full_runtime_found_on_the_machine_can_load_the_agent() {
        let w = world(Some(TRIMMED), Some(FULL));
        let info = java_info(&w.machine, &w.game, "");
        assert_eq!(
            info.source, "game",
            "the game's own runtime still runs the game"
        );
        assert_eq!(info.agent, AgentJava::Ok);
    }

    #[test]
    fn the_players_java_is_described_and_its_problems_are_named() {
        let w = world(Some(TRIMMED), None);
        let jdk = w.dir.path().join("mine");
        make_java(&jdk, FULL);
        let info = java_info(&w.machine, &w.game, &jdk.display().to_string());
        assert_eq!((info.version.as_str(), info.source), ("25.0.3", "custom"));
        assert_eq!(info.agent, AgentJava::Ok);

        let nothing = w.dir.path().join("nothing").display().to_string();
        assert_eq!(
            java_info(&w.machine, &w.game, &nothing).problem,
            JavaProblem::CustomInvalid
        );

        let old = w.dir.path().join("old");
        make_java(&old, OLD);
        let info = java_info(&w.machine, &w.game, &old.display().to_string());
        assert_eq!(
            info.problem,
            JavaProblem::CustomTooOld {
                found: "17.0.9".into()
            }
        );
    }

    #[test]
    fn no_java_at_all_is_not_found() {
        let w = world(None, None);
        let info = java_info(&w.machine, &w.game, "");
        assert_eq!(info.problem, JavaProblem::NotFound);
        assert_eq!(info.agent, AgentJava::Missing);
        assert_eq!(info.version, "");
    }

    #[test]
    fn an_editor_says_why_it_cannot_start() {
        let w = world(Some(TRIMMED), None);
        let ok = java_info(&w.machine, &w.game, "");
        let bare = world(None, None);
        let none = java_info(&bare.machine, &bare.game, "");
        assert_eq!(editor_blocker(None, &ok, Editor::Crucible), Some("game"));
        assert_eq!(
            editor_blocker(Some(&w.game), &none, Editor::Crucible),
            Some("java")
        );
        assert_eq!(
            editor_blocker(Some(&w.game), &ok, Editor::Crucible),
            Some("files")
        );
        assert_eq!(
            editor_blocker(Some(&w.game), &ok, Editor::ProjectXSceneEditor),
            None
        );
        fs::write(w.game.join("crucible.jar"), b"jar").unwrap();
        fs::create_dir_all(w.game.join("crucible/rsrc/config")).unwrap();
        assert_eq!(editor_blocker(Some(&w.game), &ok, Editor::Crucible), None);
    }

    #[test]
    fn opening_a_missing_folder_is_an_error_not_a_process() {
        assert!(open_folder(Path::new("/definitely/not/here")).is_err());
    }
}
