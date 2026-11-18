//! `UpdateModel` (QML singleton `UpdateModel`): checking for, downloading and starting a launcher update. All
//! network work runs on worker threads.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        /// "idle", "checking", "uptodate", "available", "downloading", "ready", "installing" (the installer was
        /// started and the launcher is about to quit) or "error".
        #[qproperty(QString, state, READ, WRITE, NOTIFY)]
        /// The new version while one is offered.
        #[qproperty(QString, version, READ, WRITE, NOTIFY)]
        /// Download progress, 0 to 1, while "downloading".
        #[qproperty(f64, progress, READ, WRITE, NOTIFY)]
        /// What went wrong, in the "error" state.
        #[qproperty(QString, message, READ, WRITE, NOTIFY)]
        /// What was being done when it went wrong: "check", "download" or "install" ("" outside "error").
        #[qproperty(QString, failed_during, cxx_name = "failedDuring", READ, WRITE, NOTIFY)]
        type UpdateModel = super::UpdateModelRust;

        /// Asks GitHub for the newest release.
        #[qinvokable]
        fn check(self: Pin<&mut UpdateModel>);

        /// Downloads the offered installer (verified against its published SHA-256).
        #[qinvokable]
        fn download(self: Pin<&mut UpdateModel>);

        /// Starts the downloaded installer.
        #[qinvokable]
        fn install(self: Pin<&mut UpdateModel>);

        /// Stops offering the version that is offered now.
        #[qinvokable]
        fn skip(self: Pin<&mut UpdateModel>);
    }

    impl cxx_qt::Threading for UpdateModel {}
}

use core::pin::Pin;
use std::path::PathBuf;

use cxx_qt::{CxxQtThread, CxxQtType, Threading};
use cxx_qt_lib::QString;
use skradle_core::game::Os;
use skradle_core::index::Arch;
use skradle_core::net::Client;
use skradle_core::update::{self, Update, Version};

use crate::settings;

pub struct UpdateModelRust {
    state: QString,
    version: QString,
    progress: f64,
    message: QString,
    failed_during: QString,
    offered: Option<Update>,
    installer: Option<PathBuf>,
    /// Identifies the latest request; answers from older ones are ignored.
    request: u64,
}

impl Default for UpdateModelRust {
    fn default() -> Self {
        Self {
            state: QString::from("idle"),
            version: QString::default(),
            progress: 0.0,
            message: QString::default(),
            failed_during: QString::default(),
            offered: None,
            installer: None,
            request: 0,
        }
    }
}

type Thread = CxxQtThread<qobject::UpdateModel>;

/// A download's share, 0 to 1, from the bytes so far.
fn fraction(done: u64, total: u64) -> f64 {
    if total == 0 {
        return 0.0;
    }
    (done as f64 / total as f64).clamp(0.0, 1.0)
}

impl qobject::UpdateModel {
    fn fail(mut self: Pin<&mut Self>, during: &str, why: String) {
        self.as_mut().set_message(QString::from(why.as_str()));
        self.as_mut().set_failed_during(QString::from(during));
        self.set_state(QString::from("error"));
    }

    /// Starts a new request: older workers' answers are ignored from now on.
    fn next_request(mut self: Pin<&mut Self>) -> u64 {
        let mut rust = self.as_mut().rust_mut();
        rust.request += 1;
        rust.request
    }

    pub fn check(mut self: Pin<&mut Self>) {
        if matches!(
            self.state().to_string().as_str(),
            "checking" | "downloading" | "installing"
        ) {
            return;
        }
        let id = self.as_mut().next_request();
        // Nothing of an earlier answer may linger.
        self.as_mut().rust_mut().offered = None;
        self.as_mut().rust_mut().installer = None;
        self.as_mut().set_message(QString::default());
        self.as_mut().set_failed_during(QString::default());
        self.as_mut().set_version(QString::default());
        self.as_mut().set_state(QString::from("checking"));
        let thread: Thread = self.qt_thread();
        let skipped = settings::global()
            .get()
            .skipped_update
            .as_deref()
            .and_then(Version::parse);
        let worker = thread.clone();
        let spawned = std::thread::Builder::new()
            .name("skradle-update-check".into())
            .spawn(move || {
                let result = update::check(
                    &Client::new(),
                    Version::current(),
                    Os::current(),
                    Arch::current(),
                    skipped,
                );
                let _ = worker.queue(move |mut m| {
                    if m.request != id {
                        return;
                    }
                    match result {
                        Ok(Some(update)) => {
                            m.as_mut()
                                .set_version(QString::from(update.version.to_string().as_str()));
                            m.as_mut().rust_mut().offered = Some(update);
                            m.set_state(QString::from("available"));
                        }
                        Ok(None) => m.set_state(QString::from("uptodate")),
                        Err(e) => m.fail("check", e.to_string()),
                    }
                });
            });
        if let Err(e) = spawned {
            self.fail("check", e.to_string());
        }
    }

    pub fn download(mut self: Pin<&mut Self>) {
        let Some(offered) = self.offered.clone() else {
            return;
        };
        if self.state().to_string() != "available" {
            return;
        }
        let id = self.as_mut().next_request();
        self.as_mut().set_progress(0.0);
        self.as_mut().set_state(QString::from("downloading"));
        let thread: Thread = self.qt_thread();
        let worker = thread.clone();
        let spawned = std::thread::Builder::new()
            .name("skradle-update-download".into())
            .spawn(move || {
                let progress_thread = worker.clone();
                let mut last = 0.0;
                let result = update::download(
                    &Client::new(),
                    &offered,
                    &crate::selfupdate::download_dir(),
                    &mut |p| {
                        if let Some(total) = p.total.filter(|t| *t > 0) {
                            let fraction = fraction(p.done, total);
                            // Not more than a hundred updates to the UI thread.
                            if fraction - last >= 0.01 {
                                last = fraction;
                                let _ = progress_thread.queue(move |m| {
                                    if m.request == id {
                                        m.set_progress(fraction);
                                    }
                                });
                            }
                        }
                    },
                );
                let _ = worker.queue(move |mut m| {
                    if m.request != id {
                        return;
                    }
                    match result {
                        Ok(path) => {
                            m.as_mut().rust_mut().installer = Some(path);
                            m.as_mut().set_progress(1.0);
                            m.set_state(QString::from("ready"));
                        }
                        Err(e) => m.fail("download", e.to_string()),
                    }
                });
            });
        if let Err(e) = spawned {
            self.fail("download", e.to_string());
        }
    }

    /// Starts the installer (off the UI thread: scanning a fresh download can take seconds) and, once it runs, the
    /// state becomes "installing" and the window quits so the installer can replace the program.
    pub fn install(mut self: Pin<&mut Self>) {
        let Some(path) = self.installer.clone() else {
            return;
        };
        if self.state().to_string() != "ready" {
            return;
        }
        let id = self.as_mut().next_request();
        let thread: Thread = self.qt_thread();
        let worker = thread.clone();
        let spawned = std::thread::Builder::new()
            .name("skradle-update-install".into())
            .spawn(move || {
                let result = crate::selfupdate::start_installer(&path);
                let _ = worker.queue(move |m| {
                    if m.request != id {
                        return;
                    }
                    match result {
                        Ok(()) => m.set_state(QString::from("installing")),
                        Err(e) => m.fail("install", e.to_string()),
                    }
                });
            });
        if let Err(e) = spawned {
            self.fail("install", e.to_string());
        }
    }

    /// Stops offering the version that is offered now (only while it is just offered or already downloaded).
    pub fn skip(mut self: Pin<&mut Self>) {
        if !matches!(self.state().to_string().as_str(), "available" | "ready") {
            return;
        }
        if let Some(offered) = self.as_mut().rust_mut().offered.take() {
            let version = offered.version.to_string();
            settings::global().update(|s| s.skipped_update = Some(version));
            self.as_mut().rust_mut().installer = None;
            self.as_mut().next_request();
            self.set_state(QString::from("idle"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fraction;

    #[test]
    fn the_download_fraction_is_clamped() {
        assert_eq!(fraction(0, 0), 0.0);
        assert_eq!(fraction(50, 100), 0.5);
        assert_eq!(fraction(300, 100), 1.0);
    }
}
