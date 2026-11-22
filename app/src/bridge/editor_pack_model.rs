//! `EditorPack` (QML singleton): the downloadable editors (Silverweave, Model Viewer, Particle Editor, Interface
//! Tester). It says whether they are on this computer, downloads them on request, and keeps an installed pack at the
//! newest published build by itself. All network work runs on worker threads. See `skradle_core::editor_pack`.

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
        /// "idle" (not looked at yet), "checking", "none" (not downloaded), "ready", "downloading" or "error".
        #[qproperty(QString, state, READ, WRITE, NOTIFY)]
        /// The installed build (`c488ba2-1.2.1-9bd80dc`), "" when none.
        #[qproperty(QString, build, READ, WRITE, NOTIFY)]
        /// Download progress, 0 to 1, while "downloading".
        #[qproperty(f64, progress, READ, WRITE, NOTIFY)]
        /// Why it went wrong, in the "error" state: "unpublished" (no build exists yet), "ratelimit", "offline",
        /// "server", "checksum" or "other".
        #[qproperty(QString, reason, READ, WRITE, NOTIFY)]
        /// The oldest Java major version the editors run on (the installed or published pack's; 25 until known).
        #[qproperty(i32, java_min, cxx_name = "javaMin", READ, WRITE, NOTIFY)]
        /// Whether the run in progress is an update of an installed pack that nobody asked for (it stays silent).
        #[qproperty(bool, automatic, READ, WRITE, NOTIFY)]
        type EditorPack = super::EditorPackRust;

        /// Looks at the disk, then (at most every half hour) at the published build; an installed pack that is
        /// behind is updated at once, without being asked.
        #[qinvokable]
        fn check(self: Pin<&mut EditorPack>);

        /// Downloads the editors (about 25 MB).
        #[qinvokable]
        fn install(self: Pin<&mut EditorPack>);
    }

    impl cxx_qt::Threading for EditorPack {}
}

use core::pin::Pin;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use cxx_qt::{CxxQtThread, CxxQtType, Threading};
use cxx_qt_lib::QString;
use skradle_core::editor_pack::{self, Installed, Manifest, PackError, Status};
use skradle_core::net::{Client, NetError};
use skradle_core::update::{self, Failure};

use crate::tools;

/// The Java the pack needs until a manifest says otherwise (spiralview is built for Java 25).
const DEFAULT_JAVA_MIN: i32 = 25;
const CHECK_EVERY: Duration = Duration::from_secs(30 * 60);

static LAST_CHECK: Mutex<Option<Instant>> = Mutex::new(None);

pub struct EditorPackRust {
    state: QString,
    build: QString,
    progress: f64,
    reason: QString,
    java_min: i32,
    automatic: bool,
    /// The published manifest, once seen.
    published: Option<Manifest>,
}

impl Default for EditorPackRust {
    fn default() -> Self {
        Self {
            state: QString::from("idle"),
            build: QString::default(),
            progress: 0.0,
            reason: QString::default(),
            java_min: DEFAULT_JAVA_MIN,
            automatic: false,
            published: None,
        }
    }
}

type Thread = CxxQtThread<qobject::EditorPack>;

fn fraction(done: u64, total: u64) -> f64 {
    if total == 0 {
        return 0.0;
    }
    (done as f64 / total as f64).clamp(0.0, 1.0)
}

/// The code QML words a failure with.
fn reason_code(error: &PackError) -> &'static str {
    match error {
        PackError::Net(NetError::Status(404)) => "unpublished",
        PackError::Net(e) => match update::failure_of(e) {
            Failure::RateLimited => "ratelimit",
            Failure::Offline => "offline",
            Failure::Server => "server",
            Failure::Checksum => "checksum",
            Failure::Malformed | Failure::Other => "other",
        },
        PackError::Manifest(_) | PackError::Io(_) => "other",
    }
}

fn java_min_of(manifest: &Manifest) -> i32 {
    i32::try_from(manifest.java_min).unwrap_or(DEFAULT_JAVA_MIN)
}

impl qobject::EditorPack {
    fn show_installed(mut self: Pin<&mut Self>, installed: &Installed) {
        self.as_mut()
            .set_build(QString::from(installed.manifest.build.as_str()));
        self.as_mut().set_java_min(java_min_of(&installed.manifest));
        self.set_state(QString::from("ready"));
    }

    fn fail(mut self: Pin<&mut Self>, error: &PackError) {
        eprintln!("skradle: editors: {error}");
        if self.automatic {
            // An update nobody asked for stays silent: the installed pack keeps working.
            self.as_mut().set_automatic(false);
            if let Some(installed) = editor_pack::installed(&tools::pack_dir()) {
                self.show_installed(&installed);
                return;
            }
        }
        self.as_mut().set_reason(QString::from(reason_code(error)));
        self.set_state(QString::from("error"));
    }

    /// Starts the download of `manifest` (or of the published one, fetched first when it is not known yet).
    fn start_download(mut self: Pin<&mut Self>, manifest: Option<Manifest>) {
        if self.state().to_string() == "downloading" {
            return;
        }
        self.as_mut().set_progress(0.0);
        self.as_mut().set_reason(QString::default());
        self.as_mut().set_state(QString::from("downloading"));
        let worker: Thread = self.qt_thread();
        let spawned = std::thread::Builder::new()
            .name("skradle-editors-download".into())
            .spawn(move || {
                let client = Client::new();
                let result = (|| {
                    let manifest = match manifest {
                        Some(m) => m,
                        None => editor_pack::fetch_manifest(&client)?,
                    };
                    let progress_thread = worker.clone();
                    let mut last = 0.0;
                    editor_pack::download(&client, &manifest, &tools::pack_dir(), &mut |p| {
                        let fraction = fraction(p.done, p.total.unwrap_or(manifest.jar.size));
                        // Not more than a hundred updates to the UI thread.
                        if fraction - last >= 0.01 {
                            last = fraction;
                            let _ = progress_thread.queue(move |m| m.set_progress(fraction));
                        }
                    })
                })();
                let _ = worker.queue(move |mut m| match result {
                    Ok(installed) => {
                        m.as_mut().set_automatic(false);
                        m.as_mut().set_progress(1.0);
                        m.show_installed(&installed);
                    }
                    Err(e) => m.fail(&e),
                });
            });
        if let Err(e) = spawned {
            self.fail(&PackError::Io(e));
        }
    }

    pub fn check(mut self: Pin<&mut Self>) {
        if crate::options::get().is_test_run() {
            // The same answer on every machine for screenshots and the self-test.
            self.as_mut()
                .set_build(QString::from("c488ba2-1.2.1-9bd80dc"));
            self.set_state(QString::from("ready"));
            return;
        }
        let state = self.state().to_string();
        if state == "checking" || state == "downloading" {
            return;
        }
        let on_disk = editor_pack::installed(&tools::pack_dir());
        match &on_disk {
            Some(installed) => self.as_mut().show_installed(installed),
            None => self.as_mut().set_state(QString::from("none")),
        }
        {
            let mut last = LAST_CHECK.lock().unwrap_or_else(|e| e.into_inner());
            if last.is_some_and(|t| t.elapsed() < CHECK_EVERY) {
                return;
            }
            *last = Some(Instant::now());
        }
        let was_ready = on_disk.is_some();
        let settled = if was_ready { "ready" } else { "none" };
        if !was_ready {
            self.as_mut().set_state(QString::from("checking"));
        }
        let worker: Thread = self.qt_thread();
        let spawned = std::thread::Builder::new()
            .name("skradle-editors-check".into())
            .spawn(move || {
                let result = editor_pack::fetch_manifest(&Client::new());
                let _ = worker.queue(move |mut m| match result {
                    Ok(manifest) => {
                        let disk = editor_pack::installed(&tools::pack_dir());
                        match editor_pack::status(disk.as_ref(), &manifest) {
                            Status::NotInstalled => {
                                m.as_mut().set_java_min(java_min_of(&manifest));
                                m.as_mut().rust_mut().published = Some(manifest);
                                m.set_state(QString::from("none"));
                            }
                            Status::UpToDate => {
                                if let Some(installed) = disk {
                                    m.show_installed(&installed);
                                }
                            }
                            Status::UpdateAvailable => {
                                // Always the newest build: an installed pack updates by itself.
                                m.as_mut().set_automatic(true);
                                m.start_download(Some(manifest));
                            }
                        }
                    }
                    Err(e) => {
                        // Offline or not published yet: what is on disk keeps working, and nothing is shown.
                        eprintln!("skradle: editors: {e}");
                        m.set_state(QString::from(settled));
                    }
                });
            });
        if spawned.is_err() {
            self.set_state(QString::from(settled));
        }
    }

    pub fn install(self: Pin<&mut Self>) {
        if crate::options::get().is_test_run() {
            return;
        }
        let published = self.published.clone();
        self.start_download(published);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failures_get_codes_the_page_can_word() {
        assert_eq!(
            reason_code(&PackError::Net(NetError::Status(404))),
            "unpublished"
        );
        assert_eq!(
            reason_code(&PackError::Net(NetError::Status(403))),
            "ratelimit"
        );
        assert_eq!(
            reason_code(&PackError::Net(NetError::Status(503))),
            "server"
        );
        assert_eq!(
            reason_code(&PackError::Net(NetError::Checksum {
                expected: "a".into(),
                actual: "b".into()
            })),
            "checksum"
        );
        assert_eq!(reason_code(&PackError::Manifest("x".into())), "other");
    }

    #[test]
    fn progress_is_a_fraction() {
        assert_eq!(fraction(5, 0), 0.0);
        assert_eq!(fraction(50, 100), 0.5);
        assert_eq!(fraction(500, 100), 1.0);
    }
}
