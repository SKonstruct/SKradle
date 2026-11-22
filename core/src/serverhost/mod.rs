//! Hosting a private game server from the launcher: managing the Tortodrone engine, a separate Java program the
//! player runs on their own machine. See `docs/server-host.md` for the overview, the licence handling and what is
//! unverified.
//!
//! The modules, from the bottom up:
//!
//! * [`layout`]: where everything lives in the server folder, and path safety.
//! * [`terms`]: the recorded acceptance of the engine's noncommercial licence that gates the first download.
//! * [`engine`]: finds, downloads, verifies and installs an engine release, keeps the previous one for rollback.
//! * [`config`]: the web and port lines of `config/server.yml`, and what the engine's source says about exposure.
//! * [`prompts`]: the few console exchanges the launcher needs (confirmed shutdown, WebUI account commands).
//! * [`admin`]: the generated WebUI administrator and its private credential file.
//! * [`ports`]: busy-port detection with suggestions, and the web listener probe.
//! * [`backups`]: copies of `data/` and `config/` while the server is stopped, restore and reset.
//! * [`register`]: the `servers::Server` entry for the local server and the join info for other players.
//! * [`process`]: the child JVM, its output, readiness, graceful stop and the lifecycle lock.
//!
//! Only [`process`] needs a Java and the engine; everything else is testable on its own.

pub mod admin;
pub mod backups;
pub mod config;
pub mod console;
pub mod engine;
mod fsx;
pub mod layout;
pub mod ports;
pub mod process;
pub mod prompts;
pub mod register;
pub mod signin;
pub mod terms;
