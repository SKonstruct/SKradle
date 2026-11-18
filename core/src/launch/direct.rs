//! Starting the game's client directly, without the game's updater.
//!
//! The command is the one the updater builds in `Application.createProcess` (read with `javap` from
//! `getdown-pro.jar`): `java -classpath <code jars> [mac: -Xdock:icon/-Xdock:name] <jvmarg lines> <extra.txt lines>
//! <main class> <app args>`, run in the game folder. The jar list, the JVM arguments and the main class come from the
//! game's `getdown.txt` (see [`crate::getdown_launch`]), so a game update that changes them is followed automatically.
//!
//! **This skips the updater, so it also skips the update check, the digest and signature check and the repair of
//! damaged files.** Use it only when the game was updated (and so verified) moments ago, or to start a second client
//! next to a running one. Callers should run [`missing_code_files`] first; it catches a half-installed game, not a
//! corrupted one.
//!
//! What the updater adds that this leaves out: the `-Dcom.threerings.getdown=true` marker (no game class outside the
//! updater's own package contains that string), the forwarded `app.*` system properties (pass them as
//! [`DirectOptions::system_props`] without the prefix), proxy settings, and `optimum_jvmarg`/`env.txt` (unused by the
//! game's file). The game reads `username`, `password`, `encrypted`, `knight`, `action`, `arg` and `sessionKey` from
//! system properties and `+connect=` from its arguments (`ProjectXApp.main`).
//!
//! Which Java: any that satisfies the game's `java_version` and has the modules the game needs. Direct launch never
//! looks at `java_vm`, so the launcher can use its own full runtime, which is what the graphics agent needs.

use std::ffi::OsString;
use std::io;
use std::path::Path;

use crate::game::{GameInstall, Os};
use crate::getdown::CommandSpec;
use crate::getdown_launch::{
    extra_jvm_args, process_arg, AppLaunch, ConfigError, LaunchConfig, Target,
};
use crate::getdown_txt::GetdownConfig;

/// The app id the game's `getdown.txt` uses for the client.
pub const CLIENT_APP_ID: &str = "client";

/// Heap sizes in MiB. Setting either replaces every heap flag from `getdown.txt` and `extra.txt`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Memory {
    pub initial_mb: Option<u32>,
    pub max_mb: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct DirectOptions {
    pub app_id: String,
    /// Replaces the heap flags of `getdown.txt` and `extra.txt` when set.
    pub memory: Option<Memory>,
    /// Drops `extra.txt` lines for which this returns true (the alt client drops the graphics agent's).
    pub skip_extra_line: Option<fn(&str) -> bool>,
    /// Added after everything else, so they win over earlier flags.
    pub jvm_args: Vec<String>,
    /// `-Dkey=value` system properties, placed before the JVM arguments like the updater's `app.*` ones.
    pub system_props: Vec<(String, String)>,
    /// After the `apparg` lines of `getdown.txt`, for example `+connect=host`.
    pub app_args: Vec<String>,
}

impl Default for DirectOptions {
    fn default() -> Self {
        Self {
            app_id: CLIENT_APP_ID.to_owned(),
            memory: None,
            skip_extra_line: None,
            jvm_args: Vec::new(),
            system_props: Vec::new(),
            app_args: Vec::new(),
        }
    }
}

/// The two files the command is built from.
#[derive(Debug, Clone, Default)]
pub struct Inputs {
    pub getdown_txt: String,
    /// `None` when the game folder has no `extra.txt`.
    pub extra_txt: Option<String>,
}

impl Inputs {
    pub fn read(install: &GameInstall) -> io::Result<Self> {
        let getdown_txt = std::fs::read_to_string(install.root.join("getdown.txt"))?;
        let extra_txt = match std::fs::read_to_string(install.root.join("extra.txt")) {
            Ok(text) => Some(text),
            Err(e) if e.kind() == io::ErrorKind::NotFound => None,
            Err(e) => return Err(e),
        };
        Ok(Self {
            getdown_txt,
            extra_txt,
        })
    }
}

/// True for JVM flags that set the heap size.
pub fn is_heap_flag(arg: &str) -> bool {
    [
        "-Xms",
        "-Xmx",
        "-XX:MaxHeapSize=",
        "-XX:InitialHeapSize=",
        "-XX:MaxRAMPercentage=",
        "-XX:InitialRAMPercentage=",
    ]
    .iter()
    .any(|prefix| arg.starts_with(prefix))
}

/// The classpath jars the launch needs that are not in the game folder (paths as in `getdown.txt`).
pub fn missing_code_files(root: &Path, launch: &AppLaunch) -> Vec<String> {
    launch
        .code
        .iter()
        .filter(|jar| !root.join(jar).is_file())
        .cloned()
        .collect()
}

/// Builds the command for `os` and CPU `arch` (as `std::env::consts::ARCH` spells it), reading environment variables
/// for `%ENV.X%` from the process environment.
pub fn command(
    install: &GameInstall,
    java: &Path,
    inputs: &Inputs,
    options: &DirectOptions,
    os: Os,
    arch: &str,
) -> Result<CommandSpec, ConfigError> {
    let env = |name: &str| std::env::var(name).ok();
    command_with_env(install, java, inputs, options, os, arch, &env)
}

/// [`command`] with the environment lookup supplied, for tests.
pub fn command_with_env(
    install: &GameInstall,
    java: &Path,
    inputs: &Inputs,
    options: &DirectOptions,
    os: Os,
    arch: &str,
    env: &dyn Fn(&str) -> Option<String>,
) -> Result<CommandSpec, ConfigError> {
    let target = Target::for_os(os, arch);
    let config = LaunchConfig::parse(&inputs.getdown_txt, &target);
    let launch = config.app(&options.app_id)?;
    let version = GetdownConfig::parse(&inputs.getdown_txt)
        .version
        .unwrap_or_default();
    let root = &install.root;
    let process = |arg: &str| process_arg(arg, root, &version, env);

    let mut args: Vec<OsString> = vec!["-classpath".into(), launch.classpath(os).into()];
    if os == Os::MacOs {
        if let Some(name) = config.first("ui.name") {
            let icon = config
                .first("ui.mac_dock_icon")
                .unwrap_or("../desktop.icns");
            args.push(format!("-Xdock:icon={}", root.join(icon).display()).into());
            args.push(format!("-Xdock:name={name}").into());
        }
    }
    for (key, value) in &options.system_props {
        args.push(format!("-D{key}={value}").into());
    }

    let mut jvm: Vec<String> = launch.jvm_args.iter().map(|a| process(a)).collect();
    if let Some(extra) = &inputs.extra_txt {
        jvm.extend(
            extra_jvm_args(extra, config.strict_comments(), &target)
                .into_iter()
                .filter(|line| !options.skip_extra_line.is_some_and(|skip| skip(line)))
                .map(|line| process(&line)),
        );
    }
    if let Some(memory) = options.memory {
        jvm.retain(|arg| !is_heap_flag(arg));
        if let Some(mb) = memory.initial_mb {
            jvm.push(format!("-Xms{mb}M"));
        }
        if let Some(mb) = memory.max_mb {
            jvm.push(format!("-Xmx{mb}M"));
        }
    }
    args.extend(jvm.into_iter().map(OsString::from));
    args.extend(options.jvm_args.iter().map(OsString::from));

    args.push(launch.main_class.clone().into());
    args.extend(launch.app_args.iter().map(|a| OsString::from(process(a))));
    args.extend(options.app_args.iter().map(OsString::from));

    Ok(CommandSpec {
        program: java.to_path_buf(),
        args,
        cwd: root.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::InstallKind;
    use crate::getdown_launch::FIXTURE;

    fn install() -> GameInstall {
        GameInstall {
            root: "/games/sk".into(),
            kind: InstallKind::Steam,
        }
    }

    fn inputs(extra: Option<&str>) -> Inputs {
        Inputs {
            getdown_txt: FIXTURE.to_owned(),
            extra_txt: extra.map(str::to_owned),
        }
    }

    fn strings(spec: &CommandSpec) -> Vec<String> {
        spec.args
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    }

    fn build(extra: Option<&str>, options: &DirectOptions, os: Os) -> Vec<String> {
        let spec = command_with_env(
            &install(),
            Path::new("/jre/bin/java"),
            &inputs(extra),
            options,
            os,
            "aarch64",
            &|_| None,
        )
        .unwrap();
        assert_eq!(spec.program, Path::new("/jre/bin/java"));
        assert_eq!(spec.cwd, Path::new("/games/sk"));
        strings(&spec)
    }

    #[test]
    fn builds_the_updaters_command_line_for_linux() {
        let args = build(None, &DirectOptions::default(), Os::Linux);
        assert_eq!(args[0], "-classpath");
        assert_eq!(
            args[1],
            "code/config.jar:code/projectx-config.jar:code/projectx-pcode.jar:code/lwjgl.jar:\
code/lwjgl-natives-linux.jar:code/commons-logging.jar"
        );
        assert_eq!(args[2], "-Djava.library.path=/games/sk/native");
        assert!(args.contains(&"-Dappdir=/games/sk".to_owned()));
        assert!(args.contains(&"-Dresource_dir=/games/sk/rsrc".to_owned()));
        assert!(!args.iter().any(|a| a.starts_with("-Xdock")));
        assert!(!args.iter().any(|a| a.contains("launchMechanism")));
        let n = args.len();
        assert_eq!(args[n - 1], "com.threerings.projectx.client.ProjectXApp");
    }

    #[test]
    fn mac_gets_dock_arguments_and_the_process_launch_flag() {
        let args = build(None, &DirectOptions::default(), Os::MacOs);
        assert_eq!(args[2], "-Xdock:icon=/games/sk/rsrc/ui/icon/desktop.icns");
        assert_eq!(args[3], "-Xdock:name=Spiral Knights");
        assert!(args.contains(&"-Djdk.lang.Process.launchMechanism=FORK".to_owned()));
        assert!(args[1].contains("lwjgl-natives-macos-arm64.jar"));
    }

    #[test]
    fn windows_uses_semicolons() {
        let args = build(None, &DirectOptions::default(), Os::Windows);
        assert!(args[1].contains(";code/lwjgl-natives-windows.jar;"));
        assert!(!args[1].contains(':'));
    }

    #[test]
    fn extra_txt_comes_after_the_games_own_arguments_and_before_the_class() {
        let args = build(
            Some("-XX:+UseG1GC\n-Dx = 1\n"),
            &DirectOptions::default(),
            Os::Linux,
        );
        let g1 = args.iter().position(|a| a == "-XX:+UseG1GC").unwrap();
        let opens = args
            .iter()
            .position(|a| a == "--enable-native-access=ALL-UNNAMED")
            .unwrap();
        assert!(opens < g1);
        assert_eq!(args[g1 + 1], "-Dx=1");
        assert_eq!(args[g1 + 2], "com.threerings.projectx.client.ProjectXApp");
    }

    #[test]
    fn memory_replaces_every_heap_flag() {
        let options = DirectOptions {
            memory: Some(Memory {
                initial_mb: Some(256),
                max_mb: Some(1024),
            }),
            jvm_args: vec!["-Dlast=1".into()],
            ..DirectOptions::default()
        };
        let args = build(
            Some("-Xms2048M\n-Xmx4096M\n-XX:MaxHeapSize=8g\n-Dkeep=1\n"),
            &options,
            Os::Linux,
        );
        assert!(!args
            .iter()
            .any(|a| a.contains("2048") || a.contains("4096") || a.contains("8g")));
        let ms = args.iter().position(|a| a == "-Xms256M").unwrap();
        assert_eq!(args[ms + 1], "-Xmx1024M");
        assert_eq!(args[ms + 2], "-Dlast=1");
        assert!(args.contains(&"-Dkeep=1".to_owned()));
    }

    #[test]
    fn without_memory_the_players_heap_flags_stay() {
        let args = build(Some("-Xmx4096M\n"), &DirectOptions::default(), Os::Linux);
        assert!(args.contains(&"-Xmx4096M".to_owned()));
    }

    #[test]
    fn skipped_extra_lines_and_game_arguments() {
        let options = DirectOptions {
            skip_extra_line: Some(|line| line.starts_with("-javaagent")),
            system_props: vec![("username".into(), "alt".into())],
            app_args: vec!["+connect=host".into()],
            ..DirectOptions::default()
        };
        let args = build(
            Some("-javaagent:/a/skgfx-agent.jar\n-Xss1m\n"),
            &options,
            Os::Linux,
        );
        assert!(!args.iter().any(|a| a.starts_with("-javaagent")));
        assert!(args.contains(&"-Xss1m".to_owned()));
        assert_eq!(args[2], "-Dusername=alt");
        assert_eq!(args.last().unwrap(), "+connect=host");
    }

    #[test]
    fn another_app_id_changes_only_the_class() {
        let options = DirectOptions {
            app_id: "crucible-editor".into(),
            ..DirectOptions::default()
        };
        let args = build(None, &options, Os::Linux);
        assert_eq!(
            args.last().unwrap(),
            "com.threerings.projectx.tools.CrucibleSceneEditor"
        );
    }

    #[test]
    fn unknown_config_is_an_error() {
        let bad = Inputs {
            getdown_txt: "code = a.jar\n".into(),
            extra_txt: None,
        };
        let result = command_with_env(
            &install(),
            Path::new("java"),
            &bad,
            &DirectOptions::default(),
            Os::Linux,
            "x86_64",
            &|_| None,
        );
        assert!(result.is_err());
    }

    #[test]
    fn version_substitution_uses_the_installed_version() {
        let mut inputs = inputs(None);
        inputs.getdown_txt.push_str("jvmarg = -Dv=%VERSION%\n");
        let spec = command_with_env(
            &install(),
            Path::new("java"),
            &inputs,
            &DirectOptions::default(),
            Os::Linux,
            "x86_64",
            &|_| None,
        )
        .unwrap();
        assert!(strings(&spec).contains(&"-Dv=20000101000000".to_owned()));
    }

    #[test]
    fn finds_missing_code_files_and_reads_inputs() {
        let dir = tempfile::tempdir().unwrap();
        let install = GameInstall {
            root: dir.path().to_path_buf(),
            kind: InstallKind::Standalone,
        };
        std::fs::write(dir.path().join("getdown.txt"), FIXTURE).unwrap();
        let read = Inputs::read(&install).unwrap();
        assert!(read.extra_txt.is_none());
        let config = LaunchConfig::parse(&read.getdown_txt, &Target::for_os(Os::Linux, "x86_64"));
        let launch = config.app(CLIENT_APP_ID).unwrap();
        assert_eq!(
            missing_code_files(dir.path(), &launch).len(),
            launch.code.len()
        );
        std::fs::create_dir(dir.path().join("code")).unwrap();
        for jar in &launch.code {
            std::fs::write(dir.path().join(jar), b"").unwrap();
        }
        assert!(missing_code_files(dir.path(), &launch).is_empty());
    }
}
