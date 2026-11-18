//! `AppEnv`: process-wide switches (flags and environment) for QML, read once at start.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qurl.h");
        type QUrl = cxx_qt_lib::QUrl;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        #[qproperty(bool, selftest, READ, CONSTANT)]
        #[qproperty(bool, pseudo, READ, CONSTANT)]
        #[qproperty(bool, reduce_motion, cxx_name = "reduceMotion", READ, CONSTANT)]
        #[qproperty(QString, scheme, READ, CONSTANT)]
        #[qproperty(QString, style_name, cxx_name = "styleName", READ, CONSTANT)]
        #[qproperty(QString, version, READ, CONSTANT)]
        #[qproperty(QString, qt_version, cxx_name = "qtVersion", READ, CONSTANT)]
        #[qproperty(i32, initial_page, cxx_name = "initialPage", READ, CONSTANT)]
        #[qproperty(bool, initial_collapsed, cxx_name = "initialCollapsed", READ, CONSTANT)]
        #[qproperty(i32, initial_width, cxx_name = "initialWidth", READ, CONSTANT)]
        #[qproperty(i32, initial_height, cxx_name = "initialHeight", READ, CONSTANT)]
        #[qproperty(QString, shot_dir, cxx_name = "shotDir", READ, CONSTANT)]
        #[qproperty(QString, shot_prefix, cxx_name = "shotPrefix", READ, CONSTANT)]
        type AppEnv = super::AppEnvRust;

        /// Prints a line on standard output (the self-test and the screenshot driver report through it).
        #[qinvokable]
        fn report(self: &AppEnv, text: &QString);

        /// The local path of a file URL (what a file dialog returns); empty for anything else.
        #[qinvokable]
        #[cxx_name = "localPath"]
        fn local_path(self: &AppEnv, url: &QUrl) -> QString;

        /// Saves a screenshot of the main window to `path`.
        #[qinvokable]
        fn grab(self: &AppEnv, path: &QString) -> bool;

        /// Ends the self-test: exit status 0 when `ok`.
        #[qinvokable]
        #[cxx_name = "finishSelftest"]
        fn finish_selftest(self: &AppEnv, ok: bool, text: &QString);
    }
}

use cxx_qt_lib::{QString, QUrl};

use crate::options::{self, Options};

pub struct AppEnvRust {
    selftest: bool,
    pseudo: bool,
    reduce_motion: bool,
    scheme: QString,
    style_name: QString,
    version: QString,
    qt_version: QString,
    initial_page: i32,
    initial_collapsed: bool,
    initial_width: i32,
    initial_height: i32,
    shot_dir: QString,
    shot_prefix: QString,
}

impl Default for AppEnvRust {
    fn default() -> Self {
        let o: Options = options::get().clone();
        Self {
            selftest: o.selftest,
            pseudo: o.pseudo,
            reduce_motion: o.reduce_motion,
            scheme: QString::from(o.scheme.as_str()),
            style_name: QString::from(options::style_name().as_str()),
            version: QString::from(env!("CARGO_PKG_VERSION")),
            qt_version: QString::from(super::glue::qt_version().as_str()),
            initial_page: o.page,
            initial_collapsed: o.collapsed,
            initial_width: o.width,
            initial_height: o.height,
            shot_dir: QString::from(
                o.shots
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default()
                    .as_str(),
            ),
            shot_prefix: QString::from(o.prefix.as_str()),
        }
    }
}

impl qobject::AppEnv {
    fn report(&self, text: &QString) {
        println!("{text}");
    }

    fn local_path(&self, url: &QUrl) -> QString {
        QString::from(
            crate::discovery::path_from_url_text(&url.to_qstring().to_string())
                .to_string_lossy()
                .as_ref(),
        )
    }

    fn grab(&self, path: &QString) -> bool {
        super::glue::grab_window(&path.to_string())
    }

    fn finish_selftest(&self, ok: bool, text: &QString) {
        // A binding error or missing type anywhere in the QML fails the self-test.
        let warnings = super::glue::qml_warning_count();
        let ok = ok && warnings == 0;
        let text = if warnings > 0 {
            QString::from(
                format!("{text}; {warnings} QML warning(s), see the messages above").as_str(),
            )
        } else {
            text.clone()
        };
        let text = &text;
        println!("selftest: {} ({text})", if ok { "ok" } else { "FAILED" });
        std::process::exit(if ok { 0 } else { 1 });
    }
}
