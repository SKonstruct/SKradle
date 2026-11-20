//! Discord Rich Presence while the game runs: maps the launch phase to what Discord shows and talks to Discord on a
//! thread of its own (a Discord that is not running, or slow, must never hold up the window).
//!
//! The Discord application id is [`skradle_core::presence::DISCORD_APPLICATION_ID`]; while it is empty nothing here
//! starts a thread or opens a socket, whatever the setting says.

use std::sync::mpsc::{self, Sender};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use skradle_core::presence::{self, Presence, State, Status};

use crate::launch::Phase;

/// What to tell Discord after a change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Show(Status),
    Clear,
}

/// Decides the action for a phase. `started` keeps the time the launch began, so the timer does not restart when
/// the phase moves from "launching" to "running". Off or unconfigured always clears.
pub fn decide(
    phase: Phase,
    enabled: bool,
    configured: bool,
    started: &mut Option<u64>,
    now: u64,
) -> Action {
    if !enabled || !configured {
        *started = None;
        return Action::Clear;
    }
    let state = match phase {
        Phase::Ready | Phase::Error => {
            *started = None;
            return Action::Clear;
        }
        Phase::Updating | Phase::Launching => State::Launching,
        Phase::Running => State::Playing,
    };
    let started_at = *started.get_or_insert(now);
    Action::Show(Status {
        state,
        server: None,
        started_at: Some(started_at),
    })
}

enum Message {
    Do(Action),
    Stop,
}

/// A running presence thread.
pub struct Driver {
    tx: Sender<Message>,
}

impl Driver {
    pub fn start(mut presence: Box<dyn Presence>) -> Option<Driver> {
        let (tx, rx) = mpsc::channel::<Message>();
        std::thread::Builder::new()
            .name("skradle-presence".into())
            .spawn(move || {
                while let Ok(message) = rx.recv() {
                    match message {
                        Message::Do(Action::Show(status)) => {
                            let _ = presence.update(&status);
                        }
                        Message::Do(Action::Clear) => {
                            let _ = presence.clear();
                        }
                        Message::Stop => {
                            let _ = presence.clear();
                            return;
                        }
                    }
                }
            })
            .ok()?;
        Some(Driver { tx })
    }

    pub fn send(&self, action: Action) {
        let _ = self.tx.send(Message::Do(action));
    }

    fn stop(self) {
        let _ = self.tx.send(Message::Stop);
    }
}

struct Global {
    phase: Phase,
    /// The last (phase, setting) pair sent on, so a burst of identical changes sends one update.
    last: Option<(Phase, bool)>,
    started: Option<u64>,
    driver: Option<Driver>,
}

fn global() -> &'static Mutex<Global> {
    static G: OnceLock<Mutex<Global>> = OnceLock::new();
    G.get_or_init(|| {
        Mutex::new(Global {
            phase: Phase::Ready,
            last: None,
            started: None,
            driver: None,
        })
    })
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn apply(g: &mut Global, enabled: bool, configured: bool) {
    let action = decide(g.phase, enabled, configured, &mut g.started, now());
    if !(enabled && configured) {
        if let Some(driver) = g.driver.take() {
            driver.stop();
        }
        return;
    }
    if g.driver.is_none() {
        g.driver = Driver::start(presence::from_client_id(Some(
            presence::DISCORD_APPLICATION_ID,
        )));
    }
    if let Some(driver) = &g.driver {
        driver.send(action);
    }
}

/// The launch phase changed.
pub fn phase_changed(phase: Phase, enabled: bool) {
    if crate::options::get().is_test_run() {
        return;
    }
    if let Ok(mut g) = global().lock() {
        g.phase = phase;
        if g.last == Some((phase, enabled)) {
            return;
        }
        g.last = Some((phase, enabled));
        apply(&mut g, enabled, presence::is_configured());
    }
}

/// The setting was switched.
pub fn setting_changed(enabled: bool) {
    if crate::options::get().is_test_run() {
        return;
    }
    if let Ok(mut g) = global().lock() {
        let phase = g.phase;
        if g.last == Some((phase, enabled)) {
            return;
        }
        g.last = Some((phase, enabled));
        apply(&mut g, enabled, presence::is_configured());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use skradle_core::presence::PresenceResult;
    use std::sync::{Arc, Mutex as StdMutex};

    #[test]
    fn nothing_is_shown_unless_enabled_and_configured() {
        let mut started = Some(5);
        for (enabled, configured) in [(false, true), (true, false), (false, false)] {
            assert_eq!(
                decide(Phase::Running, enabled, configured, &mut started, 9),
                Action::Clear
            );
            assert_eq!(started, None);
        }
    }

    #[test]
    fn the_timer_starts_with_the_launch_and_survives_the_phase_change() {
        let mut started = None;
        let a = decide(Phase::Updating, true, true, &mut started, 100);
        assert_eq!(
            a,
            Action::Show(Status {
                state: State::Launching,
                server: None,
                started_at: Some(100)
            })
        );
        let b = decide(Phase::Running, true, true, &mut started, 130);
        assert_eq!(
            b,
            Action::Show(Status {
                state: State::Playing,
                server: None,
                started_at: Some(100)
            })
        );
        assert_eq!(
            decide(Phase::Ready, true, true, &mut started, 200),
            Action::Clear
        );
        assert_eq!(started, None);
        let again = decide(Phase::Launching, true, true, &mut started, 300);
        assert!(matches!(
            again,
            Action::Show(Status {
                started_at: Some(300),
                ..
            })
        ));
        assert_eq!(
            decide(Phase::Error, true, true, &mut started, 400),
            Action::Clear
        );
    }

    struct Recording(Arc<StdMutex<Vec<String>>>);
    impl Presence for Recording {
        fn update(&mut self, status: &Status) -> PresenceResult {
            self.0.lock().unwrap().push(format!("{:?}", status.state));
            PresenceResult::Updated
        }
        fn clear(&mut self) -> PresenceResult {
            self.0.lock().unwrap().push("clear".into());
            PresenceResult::Updated
        }
    }

    #[test]
    fn the_driver_passes_actions_to_the_presence_in_order_and_clears_on_stop() {
        let log = Arc::new(StdMutex::new(Vec::new()));
        let driver = Driver::start(Box::new(Recording(log.clone()))).unwrap();
        driver.send(Action::Show(Status::new(State::Playing)));
        driver.send(Action::Clear);
        driver.stop();
        // The thread ends after Stop; wait for it to finish its work.
        for _ in 0..200 {
            if log.lock().unwrap().len() >= 3 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(*log.lock().unwrap(), ["Playing", "clear", "clear"]);
    }
}
