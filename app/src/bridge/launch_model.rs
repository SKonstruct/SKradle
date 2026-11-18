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
        /// 0 to 1 when `determinate`. The updater reports no progress outside its own window, so this stays false
        /// for real launches and the UI shows the indeterminate ring.
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
use std::sync::Arc;

use cxx_qt::{CxxQtThread, CxxQtType, Threading};
use cxx_qt_lib::QString;

use crate::discovery::{Install, Kind};
use crate::launch::plan::{self, Machine};
use crate::launch::supervisor::{self, Handle, Timing};
use crate::launch::{memory, Event, LaunchError, Phase, Status, Step};
use crate::settings;

#[derive(Default)]
pub struct LaunchModelRust {
    state: QString,
    step: QString,
    message: QString,
    progress: f64,
    determinate: bool,
    status: Status,
    /// Identifies the current launch; events from an older one are ignored.
    launch_id: u64,
    handle: Option<Handle>,
}

/// Plans and starts a launch. Runs off the UI thread; returns the handle of the supervisor.
fn prepare(install: &Install, sink: supervisor::Sink) -> Result<Handle, LaunchError> {
    let settings = settings::global().get();
    let plan = plan::plan(&Machine::current(), Some(install), &settings)?;
    if let Some(mb) = plan.memory_mb {
        memory::write_limit(&plan.game_root, mb)
            .map_err(|e| LaunchError::ExtraTxt(e.to_string()))?;
    }
    Ok(supervisor::start(
        plan.spec,
        plan.game_root,
        Timing::default(),
        sink,
    ))
}

impl qobject::LaunchModel {
    /// Copies the status into the properties.
    fn show(mut self: Pin<&mut Self>) {
        let status = self.status.clone();
        self.as_mut()
            .set_state(QString::from(status.phase.as_str()));
        self.as_mut().set_step(QString::from(status.step.as_str()));
        self.as_mut()
            .set_message(QString::from(status.message.as_str()));
    }

    fn on_event(mut self: Pin<&mut Self>, id: u64, event: Event) {
        if id != self.launch_id {
            return;
        }
        let ended = matches!(event, Event::Failed(_) | Event::Finished | Event::Stopped);
        self.as_mut().rust_mut().status.apply(event);
        if ended {
            self.as_mut().rust_mut().handle = None;
        }
        self.show();
    }

    pub fn start(mut self: Pin<&mut Self>, path: &QString) {
        if !self.status.can_start() {
            return;
        }
        let path = path.to_string();
        self.as_mut().rust_mut().launch_id += 1;
        let id = self.launch_id;
        self.as_mut().rust_mut().handle = None;
        self.as_mut().rust_mut().status = Status::default();
        self.as_mut()
            .rust_mut()
            .status
            .apply(Event::Phase(Phase::Updating, Step::Checking));
        self.as_mut().set_progress(0.0);
        self.as_mut().set_determinate(false);
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
                    let install = Install {
                        path: path.into(),
                        kind: Kind::Custom,
                    };
                    let sink: supervisor::Sink = {
                        let thread = thread.clone();
                        Arc::new(move |event| {
                            let _ = thread.queue(move |m| m.on_event(id, event));
                        })
                    };
                    match prepare(&install, sink) {
                        Ok(handle) => {
                            let _ = thread.queue(move |mut m| {
                                if m.launch_id == id && m.status.can_stop() {
                                    m.as_mut().rust_mut().handle = Some(handle);
                                } else {
                                    // Stopped while preparing.
                                    handle.stop();
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
        if let Some(handle) = &self.handle {
            // The supervisor ends the processes and reports `Stopped`.
            handle.stop();
        } else {
            // Still preparing: nothing runs yet.
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
        self.as_mut().rust_mut().handle = None;
        let message = if phase == Phase::Error {
            LaunchError::NoJava {
                needed: Some(25_000_000),
            }
            .message()
        } else {
            String::new()
        };
        self.as_mut().rust_mut().status = Status {
            phase,
            step: if progress > 0.0 {
                Step::Downloading
            } else {
                Step::Checking
            },
            message,
        };
        self.as_mut().set_progress(progress);
        self.as_mut()
            .set_determinate(progress > 0.0 && phase == Phase::Updating);
        self.show();
    }
}
