//! `LaunchModel` (QML singleton `LaunchModel`): the launch state machine the Home page's button drives. Choosing
//! the Java, writing the memory limit and starting the updater happen on a worker thread; the supervisor thread
//! then reports back through the queued update mechanism, so the UI thread never waits on a process.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        /// "ready", "updating", "launching", "running" or "error".
        #[qproperty(QString, state, READ, WRITE, NOTIFY)]
        /// While updating: "checking" or "downloading".
        #[qproperty(QString, step, READ, WRITE, NOTIFY)]
        /// Why the last launch failed, as a sentence ("" unless the state is "error").
        #[qproperty(QString, message, READ, WRITE, NOTIFY)]
        /// 0 to 1 when `determinate`. The game's updater reports no progress outside its own window, so this stays
        /// false for launches through it and the UI shows the indeterminate ring.
        #[qproperty(f64, progress, READ, WRITE, NOTIFY)]
        #[qproperty(bool, determinate, READ, WRITE, NOTIFY)]
        type LaunchModel = super::LaunchModelRust;

        /// Starts the game from the install folder `path`.
        #[qinvokable]
        fn start(self: Pin<&mut LaunchModel>, path: &QString);

        /// Stops the updater or the game.
        #[qinvokable]
        fn stop(self: Pin<&mut LaunchModel>);

        /// Same as `stop`: the button reads "Cancel" while updating.
        #[qinvokable]
        fn cancel(self: Pin<&mut LaunchModel>);

        /// Leaves the "error" state.
        #[qinvokable]
        fn dismiss(self: Pin<&mut LaunchModel>);

        /// Screenshots only: shows a state without running anything.
        #[qinvokable]
        fn force(self: Pin<&mut LaunchModel>, state: &QString, progress: f64);
    }

    impl cxx_qt::Threading for LaunchModel {}
}

use core::pin::Pin;
use std::sync::{Arc, OnceLock};

use cxx_qt::{CxxQtThread, CxxQtType, Threading};
use cxx_qt_lib::QString;

use crate::discovery::{Install, Kind};
use crate::launch::planned::PlannedLauncher;
use crate::launch::{Event, LaunchError, Launcher, Phase, Request, Running, Sink, Status, Step};
use crate::settings;

/// The launcher in use. One place to swap in the core's launch planner once it exists.
fn launcher() -> &'static Arc<dyn Launcher> {
    static LAUNCHER: OnceLock<Arc<dyn Launcher>> = OnceLock::new();
    LAUNCHER.get_or_init(|| Arc::new(PlannedLauncher::new()))
}

pub struct LaunchModelRust {
    state: QString,
    step: QString,
    message: QString,
    progress: f64,
    determinate: bool,
    status: Status,
    /// Identifies the current launch; events from an older one are ignored.
    launch_id: u64,
    running: Option<Box<dyn Running>>,
}

impl Default for LaunchModelRust {
    /// Starts in the `Ready` state: the properties must agree with `status` from the first frame, or the Home
    /// page shows "Game is running" and a blank button until something calls `show`.
    fn default() -> Self {
        let status = Status::default();
        Self {
            state: QString::from(status.phase.as_str()),
            step: QString::from(status.step.as_str()),
            message: QString::from(status.message.as_str()),
            progress: 0.0,
            determinate: false,
            status,
            launch_id: 0,
            running: None,
        }
    }
}

impl qobject::LaunchModel {
    /// Copies the status into the properties.
    fn show(mut self: Pin<&mut Self>) {
        let status = self.status.clone();
        crate::presence_driver::phase_changed(
            status.phase,
            settings::global().get().discord_presence,
        );
        self.as_mut()
            .set_state(QString::from(status.phase.as_str()));
        self.as_mut().set_step(QString::from(status.step.as_str()));
        self.as_mut()
            .set_message(QString::from(status.message.as_str()));
        self.as_mut().set_progress(status.progress.unwrap_or(0.0));
        self.as_mut().set_determinate(status.progress.is_some());
    }

    fn on_event(mut self: Pin<&mut Self>, id: u64, event: Event) {
        if id != self.launch_id {
            return;
        }
        let ended = matches!(event, Event::Failed(_) | Event::Finished | Event::Stopped);
        self.as_mut().rust_mut().status.apply(event);
        if ended {
            self.as_mut().rust_mut().running = None;
        }
        self.show();
    }

    pub fn start(mut self: Pin<&mut Self>, path: &QString) {
        // Screenshot and self-test runs never start the real game.
        if !self.status.can_start() || crate::options::get().is_test_run() {
            return;
        }
        let path = path.to_string();
        self.as_mut().rust_mut().launch_id += 1;
        let id = self.launch_id;
        self.as_mut().rust_mut().running = None;
        self.as_mut().rust_mut().status = Status::default();
        self.as_mut()
            .rust_mut()
            .status
            .apply(Event::Phase(Phase::Updating, Step::Checking));
        self.as_mut().show();

        let thread: CxxQtThread<qobject::LaunchModel> = self.qt_thread();
        let spawned = std::thread::Builder::new()
            .name("skradle-prepare".into())
            .spawn({
                let thread = thread.clone();
                move || {
                    if path.is_empty() {
                        let _ = thread
                            .queue(move |m| m.on_event(id, Event::Failed(LaunchError::NoGame)));
                        return;
                    }
                    let request = Request {
                        install: Install {
                            path: path.into(),
                            kind: Kind::Custom,
                        },
                        settings: settings::global().get(),
                    };
                    let sink: Sink = {
                        let thread = thread.clone();
                        Arc::new(move |event| {
                            let _ = thread.queue(move |m| m.on_event(id, event));
                        })
                    };
                    match launcher().start(request, sink) {
                        Ok(running) => {
                            let _ = thread.queue(move |mut m| {
                                if m.launch_id == id && m.status.can_stop() {
                                    m.as_mut().rust_mut().running = Some(running);
                                } else {
                                    // Stopped while it was being prepared.
                                    running.stop();
                                }
                            });
                        }
                        Err(error) => {
                            let _ = thread.queue(move |m| m.on_event(id, Event::Failed(error)));
                        }
                    }
                }
            });
        if let Err(e) = spawned {
            self.on_event(id, Event::Failed(LaunchError::Spawn(e.to_string())));
        }
    }

    pub fn stop(mut self: Pin<&mut Self>) {
        if !self.status.can_stop() {
            return;
        }
        if let Some(running) = &self.running {
            // The launch ends the processes and reports `Stopped`.
            running.stop();
        } else {
            // Still being prepared: nothing runs yet.
            self.as_mut().rust_mut().launch_id += 1;
            self.as_mut().rust_mut().status.apply(Event::Stopped);
            self.show();
        }
    }

    pub fn cancel(self: Pin<&mut Self>) {
        self.stop();
    }

    pub fn dismiss(mut self: Pin<&mut Self>) {
        if self.status.phase == Phase::Error {
            self.as_mut().rust_mut().status.apply(Event::Stopped);
            self.show();
        }
    }

    pub fn force(mut self: Pin<&mut Self>, state: &QString, progress: f64) {
        let phase = match state.to_string().as_str() {
            "updating" => Phase::Updating,
            "launching" => Phase::Launching,
            "running" => Phase::Running,
            "error" => Phase::Error,
            _ => Phase::Ready,
        };
        self.as_mut().rust_mut().launch_id += 1;
        self.as_mut().rust_mut().running = None;
        let message = if phase == Phase::Error {
            LaunchError::NoJava {
                needed: Some(25_000_000),
            }
            .message()
        } else {
            String::new()
        };
        let known = (phase == Phase::Updating && progress > 0.0).then_some(progress.min(1.0));
        self.as_mut().rust_mut().status = Status {
            phase,
            step: if known.is_some() {
                Step::Downloading
            } else {
                Step::Checking
            },
            progress: known,
            message,
        };
        self.show();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_ready_never_with_an_empty_state() {
        let m = LaunchModelRust::default();
        assert_eq!(m.state.to_string(), "ready");
        assert_eq!(m.step.to_string(), "checking");
        assert_eq!(m.message.to_string(), "");
        assert!(!m.determinate && m.progress == 0.0);
        assert!(m.status.can_start() && !m.status.can_stop());
    }
}
