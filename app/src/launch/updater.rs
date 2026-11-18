//! The launcher that runs the game's own updater (`getdown-pro.jar`), which updates, verifies and starts the game.

use super::plan::{self, Machine};
use super::supervisor::{self, Timing};
use super::{memory, LaunchError, Launcher, Request, Running, Sink};

pub struct UpdaterLauncher {
    machine: Machine,
    timing: Timing,
}

impl UpdaterLauncher {
    pub fn new() -> Self {
        Self {
            machine: Machine::current(),
            timing: Timing::default(),
        }
    }
}

impl Launcher for UpdaterLauncher {
    fn start(&self, request: Request, sink: Sink) -> Result<Box<dyn Running>, LaunchError> {
        let plan = plan::plan(&self.machine, Some(&request.install), &request.settings)?;
        if let Some(mb) = plan.memory_mb {
            memory::write_limit(&plan.game_root, mb)
                .map_err(|e| LaunchError::ExtraTxt(e.to_string()))?;
        }
        Ok(Box::new(supervisor::start(
            plan.spec,
            plan.game_root,
            self.timing,
            sink,
        )))
    }
}
