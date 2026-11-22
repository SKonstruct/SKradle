//! Starts SKradle's log (`skradle_core::log`) and writes the session header. See `docs/logging.md`.

use std::path::PathBuf;

use skradle_core::game::Env;
use skradle_core::log::{self, Session};
use skradle_core::runtime::{self, RuntimeEnv};

use crate::options::Options;

/// Opens the log and installs the panic hook. Self-test and screenshot runs do not write into the player's log
/// folder unless `SKRADLE_LOG_DIR` names one.
pub fn start(options: &Options) {
    let explicit = std::env::var_os("SKRADLE_LOG_DIR").is_some_and(|v| !v.is_empty());
    if (options.selftest || options.shots.is_some()) && !explicit {
        return;
    }
    let Some(dir) = log::default_dir(dirs::data_dir()) else {
        return;
    };
    log::init(&dir, dirs::home_dir().as_deref());
    log::install_panic_hook();
    log::info(
        "app",
        format!("SKradle {} starting", env!("CARGO_PKG_VERSION")),
    );
}

/// Writes the session header once the install and Java are known. Runs on a thread: discovery reads Steam's files.
pub fn write_header_in_background() {
    let _ = std::thread::Builder::new()
        .name("skradle-session".into())
        .spawn(|| {
            for line in log::session_header(&detect()) {
                log::info("session", line);
            }
        });
}

fn detect() -> Session {
    let settings = crate::settings::global().get();
    let snapshot = crate::bridge::game_model::scan(&settings, &Env::current());
    let install = snapshot.resolved.selected_install().cloned();
    let java = install.as_ref().and_then(|install| {
        let minimum = crate::launch::plan::java_minimum(&install.path);
        let managed: Option<PathBuf> = dirs::data_dir().map(|d| d.join("SKradle").join("runtimes"));
        let runtimes = runtime::find_runtimes(
            &RuntimeEnv::current(),
            Some(&install.path),
            managed.as_deref(),
        );
        runtime::pick_for_game(&runtimes, minimum).map(|r| {
            let v = r.release.version;
            format!("{}.{}.{} ({:?})", v.major, v.minor, v.patch, r.source)
        })
    });
    Session {
        version: env!("CARGO_PKG_VERSION").to_owned(),
        os: std::env::consts::OS.to_owned(),
        arch: std::env::consts::ARCH.to_owned(),
        qt: crate::bridge::glue::qt_version(),
        install_kind: install.map(|i| i.kind.as_str().to_owned()),
        java,
    }
}
