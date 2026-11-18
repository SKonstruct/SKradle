//! Launching the game: pick a Java, run the game's own updater (which updates, verifies and starts the game) and
//! follow it. Plain Rust, no Qt; `bridge::launch_model` wraps it.
//!
//! What the updater lets us see (read from the updater's code in the installed game; running it against the real
//! install is not done in tests):
//! - It reports download progress only to its own window. Standard output and error are redirected into
//!   `launcher.log` in the game folder, and that log holds phase messages, no byte counts or percentages. So the
//!   progress is indeterminate; the log only says which phase the updater is in.
//! - After it starts the game it exits (about a second later, on every platform but Windows 98/ME). The game keeps
//!   running as an orphan, so "the process we started" is only the updater. On Unix the updater is started in its
//!   own process group, which the game inherits: the group is how the game is followed and stopped.

pub mod log;
pub mod memory;
pub mod plan;
pub mod planned;
pub mod supervisor;
pub mod winprobe;

use std::sync::Arc;

use crate::discovery::Install;
use crate::settings::Settings;

/// Where the launch is, as the UI shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Phase {
    #[default]
    Ready,
    /// The updater is running and the game has not been started yet.
    Updating,
    /// The updater started the game; waiting for it to be up.
    Launching,
    Running,
    /// The last launch failed; `Status::message` says why. Behaves like `Ready` for the next launch.
    Error,
}

impl Phase {
    pub fn as_str(self) -> &'static str {
        match self {
            Phase::Ready => "ready",
            Phase::Updating => "updating",
            Phase::Launching => "launching",
            Phase::Running => "running",
            Phase::Error => "error",
        }
    }
}

/// What the updater is doing while the phase is `Updating`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Step {
    /// Looking for a newer version; nothing is downloading yet (and perhaps never will).
    #[default]
    Checking,
    Downloading,
}

impl Step {
    pub fn as_str(self) -> &'static str {
        match self {
            Step::Checking => "checking",
            Step::Downloading => "downloading",
        }
    }
}

/// Everything that can go wrong before or while launching, with the sentence to show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchError {
    NoGame,
    NoJava {
        needed: Option<u64>,
    },
    JavaTooOld {
        found: String,
        needed: u64,
    },
    CustomJavaInvalid {
        path: String,
    },
    CustomJavaTooOld {
        path: String,
        found: String,
        needed: u64,
    },
    ExtraTxt(String),
    Spawn(String),
    /// The updater ended with an error; `detail` is the end of its output.
    UpdaterFailed {
        code: Option<i32>,
        detail: String,
    },
    /// The updater ended cleanly but no game is running.
    NothingStarted,
    /// The launch planner refused (the sentence says why and what to do, for example to update first).
    Refused(String),
    /// A step before the game started failed (applying mods, writing the server settings); the game was not started.
    Prepare(String),
    /// A game that was started directly ended with an error soon after the start.
    GameFailed {
        code: Option<i32>,
        detail: String,
    },
}

fn java_name(encoded: u64) -> String {
    let major = encoded / 1_000_000;
    let minor = encoded / 10_000 % 100;
    let patch = encoded / 100 % 100;
    if minor == 0 && patch == 0 {
        major.to_string()
    } else {
        format!("{major}.{minor}.{patch}")
    }
}

impl LaunchError {
    /// A sentence for the player.
    pub fn message(&self) -> String {
        match self {
            LaunchError::NoGame => "No Spiral Knights install is selected.".to_owned(),
            LaunchError::NoJava { needed: Some(n) } => format!(
                "No Java was found. Spiral Knights needs Java {} or newer; install one and try again.",
                java_name(*n)
            ),
            LaunchError::NoJava { needed: None } => {
                "No Java was found. Install Java and try again.".to_owned()
            }
            LaunchError::JavaTooOld { found, needed } => format!(
                "The Java on this computer (version {found}) is too old. Spiral Knights needs Java {} or newer.",
                java_name(*needed)
            ),
            LaunchError::CustomJavaInvalid { path } => {
                format!("The Java you chose ({path}) was not found or is not a Java runtime. Choose another or clear it.")
            }
            LaunchError::CustomJavaTooOld { path, found, needed } => format!(
                "The Java you chose ({path}) is version {found}, but Spiral Knights needs Java {} or newer.",
                java_name(*needed)
            ),
            LaunchError::ExtraTxt(e) => format!("Could not apply the memory limit to the game folder: {e}"),
            LaunchError::Spawn(e) => format!("Could not start the updater: {e}"),
            LaunchError::UpdaterFailed { code, detail } => {
                let code = code.map_or("no exit code".to_owned(), |c| format!("exit code {c}"));
                if detail.is_empty() {
                    format!("The game's updater stopped ({code}).")
                } else {
                    format!("The game's updater stopped ({code}): {detail}")
                }
            }
            LaunchError::NothingStarted => {
                "The updater finished but the game did not start. Try again.".to_owned()
            }
            LaunchError::Refused(why) => why.clone(),
            LaunchError::Prepare(why) => {
                format!("The game was not started because a step before it failed: {why}")
            }
            LaunchError::GameFailed { code, detail } => {
                let code = code.map_or("no exit code".to_owned(), |c| format!("exit code {c}"));
                if detail.is_empty() {
                    format!("The game stopped right after it started ({code}).")
                } else {
                    format!("The game stopped right after it started ({code}): {detail}")
                }
            }
        }
    }
}

/// Where a launch reports to.
pub type Sink = Arc<dyn Fn(Event) + Send + Sync>;

/// What to launch.
#[derive(Debug, Clone)]
pub struct Request {
    pub install: Install,
    pub settings: Settings,
}

/// A launch in progress.
pub trait Running: Send {
    /// Ends the updater and the game; the launch then reports `Event::Stopped`.
    fn stop(&self);
}

/// Something that can run a launch. `PlannedLauncher` is the real one: it follows the core's launch planner (the update
/// check, mods, server connection, then the updater or a direct launch with the graphics agent).
/// `start` may block on file access: it is called on a worker thread, never the UI thread. Progress, phase changes
/// and the end are reported through the sink; an `Err` means nothing was started.
pub trait Launcher: Send + Sync {
    fn start(&self, request: Request, sink: Sink) -> Result<Box<dyn Running>, LaunchError>;
}

/// What a launch tells its owner.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Phase(Phase, Step),
    /// Download progress, 0 to 1, while updating. Only launchers that know it send this; the updater does not.
    #[allow(dead_code)] // sent by launchers that know the progress; the updater does not
    Progress(f64),
    Failed(LaunchError),
    /// The game exited by itself.
    Finished,
    /// The player stopped it.
    Stopped,
}

/// The launch state machine's current state. `apply` is the only way it changes.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Status {
    pub phase: Phase,
    pub step: Step,
    /// Known download progress (0 to 1); `None` shows the indeterminate ring.
    pub progress: Option<f64>,
    /// Set in the `Error` phase.
    pub message: String,
}

impl Status {
    pub fn apply(&mut self, event: Event) {
        match event {
            Event::Phase(phase, step) => {
                // A late event from a finished launch must not undo a failure or a stop.
                if matches!(self.phase, Phase::Ready | Phase::Error) && phase != Phase::Updating {
                    return;
                }
                self.phase = phase;
                self.step = step;
                if phase != Phase::Updating {
                    self.progress = None;
                }
                self.message.clear();
            }
            Event::Progress(p) => {
                if self.phase == Phase::Updating && p.is_finite() {
                    self.step = Step::Downloading;
                    self.progress = Some(p.clamp(0.0, 1.0));
                }
            }
            Event::Failed(error) => {
                self.phase = Phase::Error;
                self.progress = None;
                self.step = Step::Checking;
                self.message = error.message();
            }
            Event::Finished | Event::Stopped => {
                self.phase = Phase::Ready;
                self.progress = None;
                self.step = Step::Checking;
                self.message.clear();
            }
        }
    }

    /// Whether a new launch may start now.
    pub fn can_start(&self) -> bool {
        matches!(self.phase, Phase::Ready | Phase::Error)
    }

    /// Whether there is something to stop.
    pub fn can_stop(&self) -> bool {
        matches!(
            self.phase,
            Phase::Updating | Phase::Launching | Phase::Running
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(phase: Phase) -> Status {
        Status {
            phase,
            ..Status::default()
        }
    }

    #[test]
    fn the_normal_run() {
        let mut s = Status::default();
        assert!(s.can_start() && !s.can_stop());
        s.apply(Event::Phase(Phase::Updating, Step::Checking));
        assert!(!s.can_start() && s.can_stop());
        s.apply(Event::Phase(Phase::Updating, Step::Downloading));
        assert_eq!((s.phase, s.step), (Phase::Updating, Step::Downloading));
        s.apply(Event::Phase(Phase::Launching, Step::Checking));
        s.apply(Event::Phase(Phase::Running, Step::Checking));
        assert_eq!(s.phase, Phase::Running);
        s.apply(Event::Finished);
        assert_eq!(s.phase, Phase::Ready);
    }

    #[test]
    fn progress_only_counts_while_updating_and_is_clamped() {
        let mut s = status(Phase::Updating);
        s.apply(Event::Progress(0.4));
        assert_eq!((s.progress, s.step), (Some(0.4), Step::Downloading));
        s.apply(Event::Progress(7.0));
        assert_eq!(s.progress, Some(1.0));
        s.apply(Event::Phase(Phase::Launching, Step::Checking));
        assert_eq!(s.progress, None);
        s.apply(Event::Progress(0.5));
        assert_eq!(s.progress, None);
        let mut ready = Status::default();
        ready.apply(Event::Progress(0.5));
        assert_eq!(ready.progress, None);
    }

    #[test]
    fn failure_keeps_the_message_until_the_next_launch() {
        let mut s = status(Phase::Updating);
        s.apply(Event::Failed(LaunchError::NoJava {
            needed: Some(25_000_000),
        }));
        assert_eq!(s.phase, Phase::Error);
        assert!(s.message.contains("Java 25 or newer"), "{}", s.message);
        assert!(s.can_start() && !s.can_stop());
        s.apply(Event::Phase(Phase::Updating, Step::Checking));
        assert_eq!(s.phase, Phase::Updating);
        assert!(s.message.is_empty());
    }

    #[test]
    fn stop_returns_to_ready_from_every_running_phase() {
        for phase in [Phase::Updating, Phase::Launching, Phase::Running] {
            let mut s = status(phase);
            s.apply(Event::Stopped);
            assert_eq!(s.phase, Phase::Ready, "{phase:?}");
        }
    }

    #[test]
    fn late_events_cannot_undo_a_finished_launch() {
        let mut s = status(Phase::Ready);
        s.apply(Event::Phase(Phase::Running, Step::Checking));
        assert_eq!(s.phase, Phase::Ready);
        let mut e = status(Phase::Error);
        e.message = "x".into();
        e.apply(Event::Phase(Phase::Launching, Step::Checking));
        assert_eq!(e.phase, Phase::Error);
    }

    #[test]
    fn error_messages_read_well() {
        assert_eq!(java_name(25_000_000), "25");
        assert_eq!(java_name(21_000_500), "21.0.5");
        let too_old = LaunchError::JavaTooOld {
            found: "17.0.9".into(),
            needed: 25_000_000,
        };
        assert_eq!(
            too_old.message(),
            "The Java on this computer (version 17.0.9) is too old. Spiral Knights needs Java 25 or newer."
        );
        let failed = LaunchError::UpdaterFailed {
            code: Some(2),
            detail: String::new(),
        };
        assert_eq!(
            failed.message(),
            "The game's updater stopped (exit code 2)."
        );
    }
}
