//! The QObjects QML sees. Each wraps a plain Rust module and does no work of its own on the UI thread: long
//! operations run on worker threads and post their result back through cxx-qt's queued update mechanism.

// The cxx-qt macros expand to `unsafe` code (exported symbols and C++ calls). Everything outside this module
// stays under the crate's `deny(unsafe_code)`.
#![allow(unsafe_code)]

pub mod app_env;
pub mod glue;
