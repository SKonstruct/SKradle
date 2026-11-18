//! The game's own editors: commands to start them from the game folder.
//!
//! What exists, and how each was confirmed (read-only, with `javap` on `projectx-pcode.jar` and the real
//! `getdown.txt`; none was started):
//!
//! * **Crucible scene editor**: `getdown.txt` defines the app id `crucible-editor` with
//!   `crucible-editor.class = com.threerings.projectx.tools.CrucibleSceneEditor` and a `crucible.jar` resource; the class
//!   has `public static void main` (`new CrucibleSceneEditor(..).startup()`). It reads the system property
//!   `crucible.dir` (it throws "crucible.dir not defined." without it) and uses `<crucible.dir>/rsrc/config` and the
//!   `examples`/`templates` inside. `getdown.txt` passes `-Dcrucible.dir=../crucible` to every app, which is relative
//!   to the folder above the game, while `crucible.jar` unpacks to `crucible/` INSIDE the game folder; the command here
//!   therefore adds `-Dcrucible.dir=<game folder>/crucible` last, where it wins. The mismatch is read from the files,
//!   not observed.
//! * **ProjectX scene editor**: `com.threerings.projectx.tools.h` has a `main` (`new h(..).startup()`, message
//!   bundle `editor.projectx`, a `Tudey` scene editor subclass with map and prefab actions). It is not a
//!   `getdown.txt` app id, so the updater has no way to start it and nothing in the files says how the developers did.
//!   The class name is obfuscated and may change in any game update; the main class is therefore only as stable as
//!   that name. Which resources it needs beyond the game's `rsrc` is unknown.
//!
//! Also found but not offered: `com.threerings.tudey.tools.h` (the generic Tudey scene editor the one above extends,
//! also with a `main`) and `PseudoPlayer` (a server load tester that needs a server password, not an editor).
//!
//! Could NOT be confirmed: a model viewer, a particle editor and "Silverweave". No class in `projectx-pcode.jar`,
//! `projectx-config.jar` or `config.jar` with one of those names has a `main` method (only six classes in the whole
//! jar do; they are the ones above, the client, and `StatType`). The config editor classes (`ProjectXConfigEditor`,
//! `ConfigEditor`, `ModelTool`) exist without a `main`, so they are probably opened from inside other tools; how
//! is unknown. Nothing here pretends otherwise.
//!
//! All editors run with the client's classpath and JVM arguments (the editors are Swing/LWJGL tools in the same jars),
//! in the game folder, like the updater would. They skip the graphics agent's `extra.txt` lines.

use std::path::Path;

use crate::extra_txt::is_skgfx_line;
use crate::game::{GameInstall, Os};
use crate::getdown::CommandSpec;
use crate::getdown_launch::ConfigError;
use crate::launch::direct::{self, DirectOptions, Inputs};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Editor {
    /// The Crucible scene editor, the one the game's `getdown.txt` names.
    Crucible,
    /// The ProjectX scene editor (`com.threerings.projectx.tools.h`); obfuscated class name, see the module docs.
    ProjectXSceneEditor,
}

impl Editor {
    pub const ALL: [Editor; 2] = [Editor::Crucible, Editor::ProjectXSceneEditor];

    pub fn label(self) -> &'static str {
        match self {
            Editor::Crucible => "Crucible scene editor",
            Editor::ProjectXSceneEditor => "Scene editor",
        }
    }

    /// The `getdown.txt` app id, when the game defines one.
    pub fn app_id(self) -> Option<&'static str> {
        match self {
            Editor::Crucible => Some("crucible-editor"),
            Editor::ProjectXSceneEditor => None,
        }
    }

    pub fn main_class(self) -> &'static str {
        match self {
            Editor::Crucible => "com.threerings.projectx.tools.CrucibleSceneEditor",
            Editor::ProjectXSceneEditor => "com.threerings.projectx.tools.h",
        }
    }

    /// Files and folders (relative to the game folder) the editor needs besides the client's jars.
    pub fn required_paths(self) -> &'static [&'static str] {
        match self {
            Editor::Crucible => &["crucible.jar", "crucible/rsrc/config"],
            Editor::ProjectXSceneEditor => &[],
        }
    }
}

/// The paths of `editor.required_paths()` that are not in the game folder.
pub fn missing_resources(install: &GameInstall, editor: Editor) -> Vec<&'static str> {
    editor
        .required_paths()
        .iter()
        .copied()
        .filter(|path| !install.root.join(path).exists())
        .collect()
}

/// The command that starts `editor` for this machine's OS and CPU. `java` must satisfy the game's `java_version`.
pub fn editor_command(
    install: &GameInstall,
    java: &Path,
    editor: Editor,
    inputs: &Inputs,
) -> Result<CommandSpec, ConfigError> {
    editor_command_for(
        install,
        java,
        editor,
        inputs,
        Os::current(),
        std::env::consts::ARCH,
    )
}

/// [`editor_command`] for a given OS and architecture.
pub fn editor_command_for(
    install: &GameInstall,
    java: &Path,
    editor: Editor,
    inputs: &Inputs,
    os: Os,
    arch: &str,
) -> Result<CommandSpec, ConfigError> {
    let mut options = DirectOptions {
        // The ProjectX editor has no app id; the defaults give the shared jars and JVM arguments.
        app_id: editor.app_id().unwrap_or("").to_owned(),
        main_class: Some(editor.main_class().to_owned()),
        skip_extra_line: Some(is_skgfx_line),
        ..DirectOptions::default()
    };
    if editor == Editor::Crucible {
        options.jvm_args.push(format!(
            "-Dcrucible.dir={}",
            install.root.join("crucible").display()
        ));
    }
    direct::command_with_env(install, java, inputs, &options, os, arch, &|n| {
        std::env::var(n).ok()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::InstallKind;
    use crate::getdown_launch::FIXTURE;

    fn install(root: &Path) -> GameInstall {
        GameInstall {
            root: root.to_path_buf(),
            kind: InstallKind::Steam,
        }
    }

    fn inputs() -> Inputs {
        Inputs {
            getdown_txt: FIXTURE.to_owned(),
            extra_txt: Some("-Xmx2g\n-javaagent:/m/skgfx-agent.jar\n-Dskgfx.scale=2\n".into()),
        }
    }

    fn args(editor: Editor) -> Vec<String> {
        let spec = editor_command_for(
            &install(Path::new("/games/sk")),
            Path::new("/jre/bin/java"),
            editor,
            &inputs(),
            Os::Linux,
            "x86_64",
        )
        .unwrap();
        assert_eq!(spec.cwd, Path::new("/games/sk"));
        spec.args
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn crucible_uses_its_app_id_class_and_the_unpacked_resource_folder() {
        let args = args(Editor::Crucible);
        assert_eq!(
            args.last().unwrap(),
            "com.threerings.projectx.tools.CrucibleSceneEditor"
        );
        // The corrected property comes after the game's relative one, so it wins.
        let wrong = args
            .iter()
            .position(|a| a == "-Dcrucible.dir=../crucible")
            .unwrap();
        let right = args
            .iter()
            .position(|a| a == "-Dcrucible.dir=/games/sk/crucible")
            .unwrap();
        assert!(wrong < right);
        assert!(right < args.len() - 1, "JVM flags come before the class");
    }

    #[test]
    fn scene_editor_uses_the_default_classpath_and_its_own_class() {
        let args = args(Editor::ProjectXSceneEditor);
        assert_eq!(args.last().unwrap(), "com.threerings.projectx.tools.h");
        assert!(args[1].contains("code/projectx-pcode.jar"));
        assert!(!args.iter().any(|a| a.starts_with("-Dcrucible.dir=/games")));
    }

    #[test]
    fn editors_keep_heap_flags_but_not_the_graphics_agent() {
        for editor in Editor::ALL {
            let args = args(editor);
            assert!(args.contains(&"-Xmx2g".to_owned()));
            assert!(!args.iter().any(|a| a.contains("skgfx")));
        }
    }

    #[test]
    fn labels_and_ids() {
        assert_eq!(Editor::Crucible.app_id(), Some("crucible-editor"));
        assert_eq!(Editor::ProjectXSceneEditor.app_id(), None);
        assert_ne!(
            Editor::Crucible.label(),
            Editor::ProjectXSceneEditor.label()
        );
    }

    #[test]
    fn reports_missing_editor_resources() {
        let dir = tempfile::tempdir().unwrap();
        let install = install(dir.path());
        assert_eq!(
            missing_resources(&install, Editor::Crucible),
            ["crucible.jar", "crucible/rsrc/config"]
        );
        assert!(missing_resources(&install, Editor::ProjectXSceneEditor).is_empty());
        std::fs::write(dir.path().join("crucible.jar"), b"").unwrap();
        std::fs::create_dir_all(dir.path().join("crucible/rsrc/config")).unwrap();
        assert!(missing_resources(&install, Editor::Crucible).is_empty());
    }
}
