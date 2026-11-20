//! `GameTools` (QML singleton): what the Settings page's Game, Files and Tools sections ask the machine and the game
//! folder. Everything slow runs on worker threads; the sentences are composed in QML from the codes here.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qstringlist.h");
        type QStringList = cxx_qt_lib::QStringList;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        /// The Java a launch would use, `25.0.3`; "" when there is none.
        #[qproperty(QString, java_version, cxx_name = "javaVersion", READ, WRITE, NOTIFY)]
        /// "game", "managed", "env", "path", "custom" or "".
        #[qproperty(QString, java_source, cxx_name = "javaSource", READ, WRITE, NOTIFY)]
        /// "" (fine), "notfound", "invalid" (the entered path is no Java) or "tooold".
        #[qproperty(QString, java_problem, cxx_name = "javaProblem", READ, WRITE, NOTIFY)]
        /// The version of the player's too-old Java, for the "tooold" problem.
        #[qproperty(QString, java_found, cxx_name = "javaFound", READ, WRITE, NOTIFY)]
        /// The game's minimum Java major version (0 when unknown).
        #[qproperty(i32, java_needed, cxx_name = "javaNeeded", READ, WRITE, NOTIFY)]
        /// True when a Java that can load the graphics agent exists.
        #[qproperty(bool, agent_java_ok, cxx_name = "agentJavaOk", READ, WRITE, NOTIFY)]
        /// True when the graphics agent file is installed.
        #[qproperty(
            bool,
            agent_installed,
            cxx_name = "agentInstalled",
            READ,
            WRITE,
            NOTIFY
        )]
        /// The machine's memory in MB (0 when unknown) and the slider's range.
        #[qproperty(i32, total_memory_mb, cxx_name = "totalMemoryMb", READ, WRITE, NOTIFY)]
        #[qproperty(i32, memory_min_mb, cxx_name = "memoryMinMb", READ, WRITE, NOTIFY)]
        #[qproperty(i32, memory_max_mb, cxx_name = "memoryMaxMb", READ, WRITE, NOTIFY)]
        /// One entry per editor: "" when it can start, else "game", "java" or "files".
        #[qproperty(
            QStringList,
            editor_blockers,
            cxx_name = "editorBlockers",
            READ,
            WRITE,
            NOTIFY
        )]
        /// A reset is running.
        #[qproperty(bool, busy, READ, WRITE, NOTIFY)]
        /// The last answer to the player, as a code plus detail: "reset-done", "reset-error", "editor-error",
        /// "folder-error"; `noticeDetail` carries the count or the reason.
        #[qproperty(QString, notice, READ, WRITE, NOTIFY)]
        #[qproperty(QString, notice_detail, cxx_name = "noticeDetail", READ, WRITE, NOTIFY)]
        /// Increases with every notice, so the same one twice still fires.
        #[qproperty(i32, notice_serial, cxx_name = "noticeSerial", READ, WRITE, NOTIFY)]
        type GameTools = super::GameToolsRust;

        /// Looks at the machine and the game folder `path` (empty for none) again.
        #[qinvokable]
        fn refresh(self: Pin<&mut GameTools>, path: &QString);

        /// Opens the game folder in the file manager.
        #[qinvokable]
        #[cxx_name = "openFolder"]
        fn open_folder(self: Pin<&mut GameTools>, path: &QString);

        /// Starts editor number `index` (the order of the Tools section).
        #[qinvokable]
        #[cxx_name = "launchEditor"]
        fn launch_editor(self: Pin<&mut GameTools>, index: i32, path: &QString);

        /// Removes every applied mod and every game file the updater lists, so the updater downloads them again.
        #[qinvokable]
        #[cxx_name = "resetGameFiles"]
        fn reset_game_files(self: Pin<&mut GameTools>, path: &QString);
    }

    impl cxx_qt::Threading for GameTools {}
}

use core::pin::Pin;
use std::path::PathBuf;

use cxx_qt::{CxxQtThread, Threading};
use cxx_qt_lib::{QString, QStringList};
use skradle_core::editors::Editor;

use crate::launch::memory;
use crate::launch::plan::Machine;
use crate::settings;
use crate::tools::{self, AgentJava, JavaInfo, JavaProblem};

pub struct GameToolsRust {
    java_version: QString,
    java_source: QString,
    java_problem: QString,
    java_found: QString,
    java_needed: i32,
    agent_java_ok: bool,
    agent_installed: bool,
    total_memory_mb: i32,
    memory_min_mb: i32,
    memory_max_mb: i32,
    editor_blockers: QStringList,
    busy: bool,
    notice: QString,
    notice_detail: QString,
    notice_serial: i32,
}

impl Default for GameToolsRust {
    fn default() -> Self {
        let (min, max) = memory::slider_range(None, 0);
        let mut blockers = QStringList::default();
        for _ in Editor::ALL {
            blockers.append(QString::from("game"));
        }
        Self {
            java_version: QString::default(),
            java_source: QString::default(),
            java_problem: QString::default(),
            java_found: QString::default(),
            java_needed: 0,
            agent_java_ok: false,
            agent_installed: false,
            total_memory_mb: 0,
            memory_min_mb: min as i32,
            memory_max_mb: max as i32,
            editor_blockers: blockers,
            busy: false,
            notice: QString::default(),
            notice_detail: QString::default(),
            notice_serial: 0,
        }
    }
}

/// Everything `refresh` computes, off the UI thread.
struct Snapshot {
    java: JavaInfo,
    agent_installed: bool,
    total_mb: Option<u32>,
    blockers: Vec<&'static str>,
}

/// The same values for self-test and screenshot runs, so they never depend on the machine.
fn fixture_snapshot(has_game: bool) -> Snapshot {
    let java = JavaInfo {
        version: "25.0.3".into(),
        source: "managed",
        problem: JavaProblem::None,
        needed_major: 25,
        agent: AgentJava::Ok,
    };
    Snapshot {
        agent_installed: false,
        total_mb: Some(16384),
        blockers: Editor::ALL
            .iter()
            .map(|_| if has_game { "" } else { "game" })
            .collect(),
        java,
    }
}

fn agent_jar() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("SKradle").join("skgfx").join("skgfx-agent.jar"))
}

fn snapshot(path: &str, java_override: &str) -> Snapshot {
    if crate::options::get().is_test_run() {
        return fixture_snapshot(!path.is_empty());
    }
    let machine = Machine::current();
    let root = (!path.is_empty()).then(|| PathBuf::from(path));
    let java = root
        .as_deref()
        .map(|r| tools::java_info(&machine, r, java_override))
        .unwrap_or(JavaInfo {
            version: String::new(),
            source: "",
            problem: JavaProblem::NotFound,
            needed_major: 0,
            agent: AgentJava::Missing,
        });
    let blockers = Editor::ALL
        .iter()
        .map(|e| tools::editor_blocker(root.as_deref(), &java, *e).unwrap_or(""))
        .collect();
    Snapshot {
        agent_installed: agent_jar().is_some_and(|p| p.is_file()),
        total_mb: memory::total_ram_mb(),
        blockers,
        java,
    }
}

impl qobject::GameTools {
    fn say(mut self: Pin<&mut Self>, notice: &str, detail: &str) {
        self.as_mut().set_notice(QString::from(notice));
        self.as_mut().set_notice_detail(QString::from(detail));
        let serial = self.notice_serial().wrapping_add(1);
        self.set_notice_serial(serial);
    }

    fn apply(mut self: Pin<&mut Self>, snap: Snapshot) {
        let problem = match &snap.java.problem {
            JavaProblem::None => "",
            JavaProblem::NotFound => "notfound",
            JavaProblem::CustomInvalid => "invalid",
            JavaProblem::CustomTooOld { .. } => "tooold",
        };
        let found = match &snap.java.problem {
            JavaProblem::CustomTooOld { found } => found.as_str(),
            _ => "",
        };
        self.as_mut()
            .set_java_version(QString::from(snap.java.version.as_str()));
        self.as_mut()
            .set_java_source(QString::from(snap.java.source));
        self.as_mut().set_java_problem(QString::from(problem));
        self.as_mut().set_java_found(QString::from(found));
        self.as_mut()
            .set_java_needed(i32::try_from(snap.java.needed_major).unwrap_or(0));
        self.as_mut()
            .set_agent_java_ok(snap.java.agent == AgentJava::Ok);
        self.as_mut().set_agent_installed(snap.agent_installed);
        let (min, max) = memory::slider_range(snap.total_mb, 0);
        self.as_mut().set_total_memory_mb(
            snap.total_mb
                .map_or(0, |m| i32::try_from(m).unwrap_or(i32::MAX)),
        );
        self.as_mut().set_memory_min_mb(min as i32);
        self.as_mut().set_memory_max_mb(max as i32);
        let mut blockers = QStringList::default();
        for b in &snap.blockers {
            blockers.append(QString::from(*b));
        }
        self.set_editor_blockers(blockers);
    }

    pub fn refresh(self: Pin<&mut Self>, path: &QString) {
        let path = path.to_string();
        let java_override = settings::global().get().java_path;
        let thread: CxxQtThread<qobject::GameTools> = self.qt_thread();
        let _ = std::thread::Builder::new()
            .name("skradle-tools".into())
            .spawn(move || {
                let snap = snapshot(&path, &java_override);
                let _ = thread.queue(move |model| model.apply(snap));
            });
    }

    pub fn open_folder(self: Pin<&mut Self>, path: &QString) {
        if crate::options::get().is_test_run() {
            return;
        }
        if let Err(e) = tools::open_folder(std::path::Path::new(&path.to_string())) {
            self.say("folder-error", &e);
        }
    }

    pub fn launch_editor(self: Pin<&mut Self>, index: i32, path: &QString) {
        if crate::options::get().is_test_run() {
            return;
        }
        let Some(editor) = usize::try_from(index)
            .ok()
            .and_then(|i| Editor::ALL.get(i))
            .copied()
        else {
            return;
        };
        let path = path.to_string();
        let java_override = settings::global().get().java_path;
        let thread = self.qt_thread();
        let _ = std::thread::Builder::new()
            .name("skradle-editor".into())
            .spawn(move || {
                if let Err(e) = tools::launch_editor(
                    &Machine::current(),
                    std::path::Path::new(&path),
                    &java_override,
                    editor,
                ) {
                    let _ = thread.queue(move |model| model.say("editor-error", &e));
                }
            });
    }

    pub fn reset_game_files(mut self: Pin<&mut Self>, path: &QString) {
        if *self.busy() || crate::options::get().is_test_run() {
            return;
        }
        let path = path.to_string();
        if path.is_empty() {
            return;
        }
        self.as_mut().set_busy(true);
        let thread = self.qt_thread();
        let spawned = std::thread::Builder::new()
            .name("skradle-reset".into())
            .spawn({
                let thread = thread.clone();
                move || {
                    let result = skradle_core::reset::reset(std::path::Path::new(&path));
                    let _ = thread.queue(move |mut model| {
                        model.as_mut().set_busy(false);
                        match result {
                            Ok(report) => model.say("reset-done", &report.removed.to_string()),
                            Err(e) => model.say("reset-error", &e.to_string()),
                        }
                    });
                }
            });
        if spawned.is_err() {
            self.as_mut().set_busy(false);
            self.say("reset-error", "no thread");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_test_run_snapshot_is_fixed_and_has_one_blocker_entry_per_editor() {
        let with = fixture_snapshot(true);
        assert_eq!(with.blockers.len(), Editor::ALL.len());
        assert!(with.blockers.iter().all(|b| b.is_empty()));
        assert!(fixture_snapshot(false)
            .blockers
            .iter()
            .all(|b| *b == "game"));
        assert_eq!(with.java.version, "25.0.3");
    }
}
