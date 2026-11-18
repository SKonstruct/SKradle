//! Working out what to run: which Java, which arguments, and what to write in the game folder first.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use skradle_core::game::Os;
use skradle_core::getdown::{self, CommandSpec};
use skradle_core::getdown_txt::GetdownConfig;
use skradle_core::runtime::{self, JavaRelease, Runtime, RuntimeEnv, Source};

use super::LaunchError;
use crate::discovery::Install;
use crate::settings::Settings;

/// The machine's Java-related inputs, so tests can fake them.
pub struct Machine {
    pub os: Os,
    pub runtime_env: RuntimeEnv,
    /// Where SKradle's own downloaded runtimes live.
    pub managed_dir: Option<PathBuf>,
}

impl Machine {
    pub fn current() -> Self {
        Self {
            os: Os::current(),
            runtime_env: RuntimeEnv::current(),
            managed_dir: dirs::data_dir().map(|d| d.join("SKradle").join("runtimes")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub spec: CommandSpec,
    pub game_root: PathBuf,
    /// The heap limit to write into `extra.txt` first, if the player set one.
    pub memory_mb: Option<u32>,
    /// Which Java was chosen, for logging.
    pub java_home: PathBuf,
}

/// The minimum Java the install asks for, in the updater's encoding (0 when it does not say).
pub fn java_minimum(game_root: &Path) -> u64 {
    std::fs::read_to_string(game_root.join("getdown.txt"))
        .ok()
        .and_then(|text| GetdownConfig::parse(&text).java_version)
        .unwrap_or(0)
}

/// Picks the Java for the game: the player's own choice when set (and valid), otherwise the first runtime that
/// meets the install's minimum, in the core's order (the game's own, SKradle's, `JAVA_HOME`, `PATH`).
pub fn pick_java(
    machine: &Machine,
    install: &Install,
    chosen: &str,
    minimum: u64,
) -> Result<Runtime, LaunchError> {
    if !chosen.trim().is_empty() {
        let runtime = custom_runtime(Path::new(chosen.trim()), machine.os).ok_or_else(|| {
            LaunchError::CustomJavaInvalid {
                path: chosen.trim().to_owned(),
            }
        })?;
        if !runtime.release.meets_game_minimum(minimum) {
            return Err(LaunchError::CustomJavaTooOld {
                path: chosen.trim().to_owned(),
                found: version_text(&runtime.release),
                needed: minimum,
            });
        }
        return Ok(runtime);
    }
    let runtimes = runtime::find_runtimes(
        &machine.runtime_env,
        Some(&install.path),
        machine.managed_dir.as_deref(),
    );
    match runtime::pick_for_game(&runtimes, minimum) {
        Some(runtime) => Ok(runtime.clone()),
        None => match runtimes
            .iter()
            .map(|r| &r.release)
            .max_by_key(|r| r.version)
        {
            Some(best) => Err(LaunchError::JavaTooOld {
                found: version_text(best),
                needed: minimum,
            }),
            None => Err(LaunchError::NoJava {
                needed: (minimum > 0).then_some(minimum),
            }),
        },
    }
}

fn version_text(release: &JavaRelease) -> String {
    let v = release.version;
    format!("{}.{}.{}", v.major, v.minor, v.patch)
}

/// A runtime the player pointed to: a Java home, its `bin/java` file (often a symlink such as `/usr/bin/java`), or
/// a macOS `.jdk` bundle (whose home is `Contents/Home`).
fn custom_runtime(path: &Path, os: Os) -> Option<Runtime> {
    // Follow symlinks first: `/usr/bin/java` is only a link to the real `<home>/bin/java`.
    let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let mut home = if path.is_file() {
        path.parent()?.parent()?.to_path_buf()
    } else {
        path
    };
    if !home.join("release").is_file() && home.join("Contents/Home/release").is_file() {
        home = home.join("Contents/Home");
    }
    let release = JavaRelease::read(&home)?;
    let runtime = Runtime {
        home,
        release,
        source: Source::JavaHome,
    };
    runtime.java_executable(os).is_file().then_some(runtime)
}

/// Builds the launch: checks the install, picks a Java and builds the updater command.
pub fn plan(
    machine: &Machine,
    install: Option<&Install>,
    settings: &Settings,
) -> Result<Plan, LaunchError> {
    let install = install.ok_or(LaunchError::NoGame)?;
    let minimum = java_minimum(&install.path);
    let runtime = pick_java(machine, install, &settings.java_path, minimum)?;
    let jvm_args: Vec<OsString> = settings
        .extra_jvm_args
        .split_whitespace()
        .map(OsString::from)
        .collect();
    let spec = getdown::update_and_launch(
        &install.to_core(),
        &runtime.java_executable(machine.os),
        &jvm_args,
    );
    Ok(Plan {
        spec,
        game_root: install.path.clone(),
        memory_mb: settings.memory_mb,
        java_home: runtime.home,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discovery::Kind;
    use std::fs;

    /// A fake Java home: a `release` file and a `bin/java` file (never run).
    fn fake_java(home: &Path, version: &str) {
        fs::create_dir_all(home.join("bin")).unwrap();
        fs::write(
            home.join("release"),
            format!("JAVA_VERSION=\"{version}\"\n"),
        )
        .unwrap();
        fs::write(home.join("bin/java"), b"").unwrap();
    }

    fn game(root: &Path, java_version: Option<&str>) -> Install {
        fs::create_dir_all(root).unwrap();
        let mut getdown = "version = 1\n".to_owned();
        if let Some(v) = java_version {
            getdown.push_str(&format!("java_version = {v}\n"));
        }
        fs::write(root.join("getdown.txt"), getdown).unwrap();
        fs::write(root.join("getdown-pro.jar"), b"jar").unwrap();
        Install {
            path: root.to_path_buf(),
            kind: Kind::Custom,
        }
    }

    fn machine(java_home: Option<&Path>) -> Machine {
        Machine {
            os: Os::Linux,
            runtime_env: RuntimeEnv {
                java_home: java_home.map(Path::to_path_buf),
                path: vec![],
            },
            managed_dir: None,
        }
    }

    #[test]
    fn reads_the_minimum_from_getdown_txt() {
        let dir = tempfile::tempdir().unwrap();
        let install = game(&dir.path().join("g"), Some("25000000"));
        assert_eq!(java_minimum(&install.path), 25_000_000);
        let none = game(&dir.path().join("h"), None);
        assert_eq!(java_minimum(&none.path), 0);
        assert_eq!(java_minimum(&dir.path().join("missing")), 0);
    }

    #[test]
    fn prefers_the_games_own_java() {
        let dir = tempfile::tempdir().unwrap();
        let install = game(&dir.path().join("g"), Some("25000000"));
        fake_java(&install.path.join("java_vm"), "25.0.5");
        fake_java(&dir.path().join("jh"), "25.0.1");
        let p = plan(
            &machine(Some(&dir.path().join("jh"))),
            Some(&install),
            &Settings::default(),
        )
        .unwrap();
        assert_eq!(p.java_home, install.path.join("java_vm"));
        assert_eq!(p.spec.program, install.path.join("java_vm/bin/java"));
        assert_eq!(p.spec.cwd, install.path);
        assert_eq!(p.memory_mb, None);
    }

    #[test]
    fn falls_back_to_java_home_when_the_game_has_none() {
        let dir = tempfile::tempdir().unwrap();
        let install = game(&dir.path().join("g"), Some("25000000"));
        fake_java(&dir.path().join("jh"), "26.0.0");
        let p = plan(
            &machine(Some(&dir.path().join("jh"))),
            Some(&install),
            &Settings::default(),
        )
        .unwrap();
        assert_eq!(p.java_home, dir.path().join("jh"));
    }

    #[test]
    fn no_java_is_a_readable_error() {
        let dir = tempfile::tempdir().unwrap();
        let install = game(&dir.path().join("g"), Some("25000000"));
        let err = plan(&machine(None), Some(&install), &Settings::default()).unwrap_err();
        assert_eq!(
            err,
            LaunchError::NoJava {
                needed: Some(25_000_000)
            }
        );
        assert!(err.message().contains("Java 25 or newer"));
    }

    #[test]
    fn a_java_that_is_too_old_is_reported_with_its_version() {
        let dir = tempfile::tempdir().unwrap();
        let install = game(&dir.path().join("g"), Some("25000000"));
        fake_java(&dir.path().join("jh"), "17.0.9");
        let err = plan(
            &machine(Some(&dir.path().join("jh"))),
            Some(&install),
            &Settings::default(),
        )
        .unwrap_err();
        assert_eq!(
            err,
            LaunchError::JavaTooOld {
                found: "17.0.9".into(),
                needed: 25_000_000
            }
        );
    }

    #[test]
    fn the_players_java_wins_and_is_checked() {
        let dir = tempfile::tempdir().unwrap();
        let install = game(&dir.path().join("g"), Some("25000000"));
        fake_java(&install.path.join("java_vm"), "25.0.5");
        fake_java(&dir.path().join("mine"), "27.0.0");
        let mut settings = Settings {
            java_path: dir.path().join("mine").display().to_string(),
            ..Settings::default()
        };
        let p = plan(&machine(None), Some(&install), &settings).unwrap();
        // The path is resolved (symlinks followed), so compare resolved paths.
        let same =
            |a: &Path, b: &Path| fs::canonicalize(a).unwrap() == fs::canonicalize(b).unwrap();
        assert!(same(&p.java_home, &dir.path().join("mine")));
        // The `bin/java` file works as well as the home.
        settings.java_path = dir.path().join("mine/bin/java").display().to_string();
        assert!(same(
            &plan(&machine(None), Some(&install), &settings)
                .unwrap()
                .java_home,
            &dir.path().join("mine")
        ));

        fake_java(&dir.path().join("old"), "11.0.1");
        settings.java_path = dir.path().join("old").display().to_string();
        assert!(matches!(
            plan(&machine(None), Some(&install), &settings),
            Err(LaunchError::CustomJavaTooOld { .. })
        ));
        settings.java_path = dir.path().join("nothing").display().to_string();
        assert!(matches!(
            plan(&machine(None), Some(&install), &settings),
            Err(LaunchError::CustomJavaInvalid { .. })
        ));
    }

    #[cfg(unix)]
    #[test]
    fn a_symlinked_java_and_a_jdk_bundle_are_accepted() {
        let dir = tempfile::tempdir().unwrap();
        let install = game(&dir.path().join("g"), Some("25000000"));
        // `/usr/bin/java` style: a link to the real file.
        fake_java(&dir.path().join("real/jdk"), "26.0.0");
        fs::create_dir_all(dir.path().join("usr/bin")).unwrap();
        std::os::unix::fs::symlink(
            dir.path().join("real/jdk/bin/java"),
            dir.path().join("usr/bin/java"),
        )
        .unwrap();
        let mut settings = Settings {
            java_path: dir.path().join("usr/bin/java").display().to_string(),
            ..Settings::default()
        };
        let p = plan(&machine(None), Some(&install), &settings).unwrap();
        assert_eq!(
            fs::canonicalize(&p.java_home).unwrap(),
            fs::canonicalize(dir.path().join("real/jdk")).unwrap()
        );
        // A macOS `.jdk` bundle: the home is inside it.
        fake_java(&dir.path().join("temurin-25.jdk/Contents/Home"), "25.0.2");
        settings.java_path = dir.path().join("temurin-25.jdk").display().to_string();
        let p = plan(&machine(None), Some(&install), &settings).unwrap();
        assert!(p.java_home.ends_with("temurin-25.jdk/Contents/Home"));
    }

    #[test]
    fn no_install_selected() {
        assert_eq!(
            plan(&machine(None), None, &Settings::default()).unwrap_err(),
            LaunchError::NoGame
        );
    }

    #[test]
    fn memory_and_extra_arguments_are_carried() {
        let dir = tempfile::tempdir().unwrap();
        let install = game(&dir.path().join("g"), Some("25000000"));
        fake_java(&install.path.join("java_vm"), "25.0.5");
        let settings = Settings {
            memory_mb: Some(3072),
            extra_jvm_args: "-Dhttps.proxyHost=p  -Dx=1".into(),
            ..Settings::default()
        };
        let p = plan(&machine(None), Some(&install), &settings).unwrap();
        assert_eq!(p.memory_mb, Some(3072));
        let args: Vec<_> = p
            .spec
            .args
            .iter()
            .map(|a| a.to_str().unwrap().to_owned())
            .collect();
        let jar = args.iter().position(|a| a == "-jar").unwrap();
        assert!(args[..jar].contains(&"-Dhttps.proxyHost=p".to_owned()));
        assert!(args[..jar].contains(&"-Dx=1".to_owned()));
    }
}
