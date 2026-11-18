//! Hosting a private game server from the launcher: managing the Tortodrone engine, a separate Java program the
//! player runs on their own machine. See `docs/server-host.md` for the overview, the licence handling and what is
//! unverified.
//!
//! The modules, from the bottom up:
//!
//! * [`layout`]: where everything lives in the server folder, and path safety.
//! * [`terms`]: the recorded acceptance of the engine's noncommercial licence that gates the first download.
//! * [`engine`]: finds, downloads, verifies and installs an engine release, keeps the previous one for rollback.
//! * [`settings`]: the engine's own `config/server.yml`, read and written without losing comments or unknown keys.
//! * [`console`]: the engine's console protocol as a pure state machine over lines, plus typed commands.
//! * [`db`]: a read-only reader of the engine's SQLite database (accounts, knights).
//! * [`backups`]: copies of `data/` and `config/` while the server is stopped, restore and reset.
//! * [`register`]: the `servers::Server` entry for the local server and the join info for other players.
//! * [`process`]: the child JVM, its output, readiness, the lifecycle lock and the console connection.
//!
//! Nothing here depends on the engine being present except [`process`]; everything else is testable on its own.

// pub mod backups;
// pub mod console;
// pub mod db;
pub mod engine;
mod fsx;
pub mod layout;
// pub mod process;
// pub mod register;
// pub mod settings;
pub mod terms;
