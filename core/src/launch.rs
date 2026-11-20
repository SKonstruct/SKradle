//! The ways SKradle starts the game. See `docs/launching.md` for how they differ.
//!
//! * [`crate::getdown::update_and_launch`]: run the game's own updater, which verifies, updates and starts the game.
//! * [`direct`]: start the game's client without the updater, for a game that is known to be up to date.
//! * [`alt`]: a second, lighter client started the direct way.

pub mod alt;
pub mod direct;
pub mod plan;
pub mod steam;
