//! Running the game's own updater. `getdown-pro.jar` in the game folder verifies and updates the game and
//! then starts it, so launching the game means running it with the game folder as the working directory.
//!
//! This module only builds the command; starting it is up to the caller.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::game::{GameInstall, Os};

/// A program to run: what to start, with which arguments, and where.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandSpec {
    pub program: PathBuf,
    pub args: Vec<OsString>,
    pub cwd: PathBuf,
    /// Variables set for the child, on top of the environment it inherits. Empty for most commands.
    pub env: Vec<(OsString, OsString)>,
}

impl CommandSpec {
    pub fn to_command(&self) -> Command {
        let mut command = Command::new(&self.program);
        command.args(&self.args).current_dir(&self.cwd);
        command.envs(self.env.iter().map(|(k, v)| (k, v)));
        command
    }
}

/// Where a game folder keeps the Java runtime it ships with (`java_vm`), if it is there.
pub fn bundled_java(game_root: &Path, os: Os) -> Option<PathBuf> {
    let names: &[&str] = match os {
        Os::Windows => &["javaw.exe", "java.exe"],
        Os::MacOs | Os::Linux => &["java"],
    };
    names
        .iter()
        .map(|name| game_root.join("java_vm").join("bin").join(name))
        .find(|path| path.is_file())
}

/// Update (if needed), verify and start the game with `java` (any Java that satisfies the game's minimum).
/// `jvm_args` go before `-jar`, for example `-Xmx2g`.
pub fn update_and_launch(install: &GameInstall, java: &Path, jvm_args: &[OsString]) -> CommandSpec {
    let mut args: Vec<OsString> = vec![
        "-Dsun.java2d.d3d=false".into(),
        "-Dcheck_unpacked=true".into(),
    ];
    args.extend(jvm_args.iter().cloned());
    args.extend(["-jar", "getdown-pro.jar", ".", "client"].map(OsString::from));
    CommandSpec {
        program: java.to_path_buf(),
        args,
        cwd: install.root.clone(),
        env: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::InstallKind;
    use std::fs;

    fn install(root: &Path) -> GameInstall {
        GameInstall {
            root: root.to_path_buf(),
            kind: InstallKind::Steam,
        }
    }

    #[test]
    fn runs_getdown_from_the_game_folder() {
        let spec = update_and_launch(
            &install(Path::new("/games/sk")),
            Path::new("/jre/bin/java"),
            &[],
        );
        assert_eq!(spec.program, Path::new("/jre/bin/java"));
        assert_eq!(spec.cwd, Path::new("/games/sk"));
        let args: Vec<_> = spec.args.iter().map(|a| a.to_str().unwrap()).collect();
        assert_eq!(
            args,
            [
                "-Dsun.java2d.d3d=false",
                "-Dcheck_unpacked=true",
                "-jar",
                "getdown-pro.jar",
                ".",
                "client"
            ]
        );
    }

    #[test]
    fn extra_jvm_args_come_before_the_jar() {
        let spec = update_and_launch(
            &install(Path::new("/g")),
            Path::new("java"),
            &["-Xmx2g".into(), "-Dx=1".into()],
        );
        let args: Vec<_> = spec.args.iter().map(|a| a.to_str().unwrap()).collect();
        let jar = args.iter().position(|a| *a == "-jar").unwrap();
        assert_eq!(&args[2..jar], ["-Xmx2g", "-Dx=1"]);
        assert_eq!(args.last(), Some(&"client"));
    }

    #[test]
    fn to_command_keeps_program_args_and_directory() {
        let spec = update_and_launch(&install(Path::new("/games/sk")), Path::new("java"), &[]);
        let command = spec.to_command();
        assert_eq!(command.get_program(), "java");
        assert_eq!(command.get_current_dir(), Some(Path::new("/games/sk")));
        assert_eq!(command.get_args().count(), spec.args.len());
    }

    #[test]
    fn finds_the_bundled_java_per_os() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("java_vm/bin");
        fs::create_dir_all(&bin).unwrap();
        assert_eq!(bundled_java(dir.path(), Os::Linux), None);
        fs::write(bin.join("java"), b"").unwrap();
        assert_eq!(bundled_java(dir.path(), Os::MacOs), Some(bin.join("java")));
        assert_eq!(bundled_java(dir.path(), Os::Windows), None);
        fs::write(bin.join("javaw.exe"), b"").unwrap();
        assert_eq!(
            bundled_java(dir.path(), Os::Windows),
            Some(bin.join("javaw.exe"))
        );
    }
}
