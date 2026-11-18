//! Hosting a private game server from the launcher: managing the Tortodrone engine, a separate Java program the
//! player runs on their own machine. See `docs/server-host.md` for the overview, the licence handling and what is
//! unverified.
//!
//! The modules, from the bottom up:
//!
//! * [`layout`]: where everything lives in the server folder, and path safety.
//! * [`terms`]: the recorded acceptance of the engine's noncommercial licence that gates the first download.
//! * [`engine`]: finds, downloads, verifies and installs an engine release, keeps the previous one for rollback.
//!
//! Nothing here depends on the engine being present except [`process`]; everything else is testable on its own.

pub mod engine;
mod fsx;
pub mod layout;
pub mod terms;
