//! The QObjects QML sees. Each wraps a plain Rust module and does no work of its own on the UI thread: long
//! operations run on worker threads and post their result back through cxx-qt's queued update mechanism.

// The cxx-qt macros expand to `unsafe` code (exported symbols and C++ calls). Everything outside this module
// stays under the crate's `deny(unsafe_code)`.
#![allow(unsafe_code)]

pub mod app_env;
pub mod editor_pack_model;
pub mod feed_model;
pub mod game_model;
pub mod glue;
pub mod launch_model;
pub mod logs_model;
pub mod mods_model;
pub mod release_notes_model;
pub mod server_host_model;
pub mod servers_model;
pub mod settings_model;
pub mod tools_model;
pub mod update_model;
