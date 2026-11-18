//! The crash reporter. The DSN is compiled in from the `SKRADLE_SENTRY_DSN` environment variable at build time;
//! without one nothing is ever sent. The player's switch is the `crashReports` setting.

use std::sync::{Arc, OnceLock};

use skradle_core::telemetry::{Dsn, Reporter};

static REPORTER: OnceLock<Arc<Reporter>> = OnceLock::new();

/// The DSN given at build time, if any.
fn build_dsn() -> Option<Dsn> {
    option_env!("SKRADLE_SENTRY_DSN").and_then(Dsn::parse)
}

/// Starts the reporter with the stored switch and reports panics.
pub fn init(enabled: bool) {
    let reporter = Reporter::new(
        build_dsn(),
        enabled,
        format!("skradle@{}", env!("CARGO_PKG_VERSION")),
    );
    reporter.install_panic_hook();
    let _ = REPORTER.set(reporter);
}

/// Applies the settings switch.
pub fn set_enabled(enabled: bool) {
    if let Some(reporter) = REPORTER.get() {
        reporter.set_enabled(enabled);
    }
}

/// Whether reports would be sent (a DSN is built in and the switch is on).
pub fn is_active() -> bool {
    REPORTER.get().is_some_and(|r| r.is_active())
}
