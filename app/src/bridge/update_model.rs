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
        /// "idle", "checking", "uptodate", "available", "downloading", "ready" or "error".
        #[qproperty(QString, state, READ, WRITE, NOTIFY)]
        /// The new version while one is offered.
        #[qproperty(QString, version, READ, WRITE, NOTIFY)]
        /// Download progress, 0 to 1, while "downloading".
        #[qproperty(f64, progress, READ, WRITE, NOTIFY)]
        /// What went wrong, in the "error" state.
        #[qproperty(QString, message, READ, WRITE, NOTIFY)]
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
    offered: Option<Update>,
    installer: Option<PathBuf>,
}

impl Default for UpdateModelRust {
    fn default() -> Self {
        Self {
            state: QString::from("idle"),
            version: QString::default(),
            progress: 0.0,
            message: QString::default(),
            offered: None,
            installer: None,
        }
    }
}

type Thread = CxxQtThread<qobject::UpdateModel>;

impl qobject::UpdateModel {
    fn fail(mut self: Pin<&mut Self>, why: String) {
        self.as_mut().set_message(QString::from(why.as_str()));
        self.set_state(QString::from("error"));
    }

    pub fn check(mut self: Pin<&mut Self>) {
        if matches!(
            self.state().to_string().as_str(),
            "checking" | "downloading"
        ) {
            return;
        }
        self.as_mut().set_state(QString::from("checking"));
        let thread: Thread = self.qt_thread();
        let skipped = settings::global()
            .get()
            .skipped_update
            .as_deref()
            .and_then(Version::parse);
        let _ = std::thread::Builder::new()
            .name("skradle-update-check".into())
            .spawn(move || {
                let result = update::check(
                    &Client::new(),
                    Version::current(),
                    Os::current(),
                    Arch::current(),
                    skipped,
                );
                let _ = thread.queue(move |mut m| match result {
                    Ok(Some(update)) => {
                        m.as_mut()
                            .set_version(QString::from(update.version.to_string().as_str()));
                        m.as_mut().rust_mut().offered = Some(update);
                        m.set_state(QString::from("available"));
                    }
                    Ok(None) => m.set_state(QString::from("uptodate")),
                    Err(e) => m.fail(e.to_string()),
                });
            });
    }

    pub fn download(mut self: Pin<&mut Self>) {
        let Some(offered) = self.offered.clone() else {
            return;
        };
        if self.state().to_string() != "available" {
            return;
        }
        self.as_mut().set_progress(0.0);
        self.as_mut().set_state(QString::from("downloading"));
        let thread: Thread = self.qt_thread();
        let _ = std::thread::Builder::new()
            .name("skradle-update-download".into())
            .spawn(move || {
                let progress_thread = thread.clone();
                let mut last = 0.0;
                let result = update::download(
                    &Client::new(),
                    &offered,
                    &crate::selfupdate::download_dir(),
                    &mut |p| {
                        if let Some(total) = p.total.filter(|t| *t > 0) {
                            let fraction = p.done as f64 / total as f64;
                            // Not more than a hundred updates to the UI thread.
                            if fraction - last >= 0.01 {
                                last = fraction;
                                let _ = progress_thread.queue(move |m| m.set_progress(fraction));
                            }
                        }
                    },
                );
                let _ = thread.queue(move |mut m| match result {
                    Ok(path) => {
                        m.as_mut().rust_mut().installer = Some(path);
                        m.as_mut().set_progress(1.0);
                        m.set_state(QString::from("ready"));
                    }
                    Err(e) => m.fail(e.to_string()),
                });
            });
    }

    pub fn install(self: Pin<&mut Self>) {
        let Some(path) = self.installer.clone() else {
            return;
        };
        if let Err(e) = crate::selfupdate::start_installer(&path) {
            self.fail(format!("Could not start the installer: {e}"));
        }
    }

    pub fn skip(mut self: Pin<&mut Self>) {
        if let Some(offered) = self.as_mut().rust_mut().offered.take() {
            let version = offered.version.to_string();
            settings::global().update(|s| s.skipped_update = Some(version));
            self.set_state(QString::from("idle"));
        }
    }
}
