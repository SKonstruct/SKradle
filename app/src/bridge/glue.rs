//! The C++ helpers in `cpp/glue.cpp`.

#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qqmlapplicationengine.h");
        type QQmlApplicationEngine = cxx_qt_lib::QQmlApplicationEngine;

        include!("glue.h");
        #[namespace = "skradle"]
        fn init_app();
        #[namespace = "skradle"]
        fn init_webview();
        #[namespace = "skradle"]
        fn webview_backend_available() -> bool;
        #[namespace = "skradle"]
        fn register_engine(engine: Pin<&mut QQmlApplicationEngine>);
        #[namespace = "skradle"]
        fn has_window() -> bool;
        #[namespace = "skradle"]
        fn grab_window(path: &QString) -> bool;
        #[namespace = "skradle"]
        fn quit_app(code: i32);
        #[namespace = "skradle"]
        fn qml_warning_count() -> i32;
        #[namespace = "skradle"]
        fn qt_version() -> QString;
        #[namespace = "skradle"]
        fn copy_text(text: &QString);
        #[namespace = "skradle"]
        fn set_language(language: &QString) -> QString;
        #[namespace = "skradle"]
        fn translate_text(source: &QString) -> QString;
    }
}

pub use ffi::{
    has_window, init_app, init_webview, quit_app, register_engine, webview_backend_available,
};

/// Installs the translator for `language`; returns the catalogue in use (empty for the English source text).
pub fn set_language(language: &str) -> String {
    ffi::set_language(&cxx_qt_lib::QString::from(language)).to_string()
}

/// The translation of `source` in the language in use (the text itself when there is none).
pub fn translate_text(source: &str) -> String {
    ffi::translate_text(&cxx_qt_lib::QString::from(source)).to_string()
}

/// Puts `text` on the clipboard.
pub fn copy_text(text: &str) {
    ffi::copy_text(&cxx_qt_lib::QString::from(text));
}

/// The Qt version in use.
pub fn qt_version() -> String {
    ffi::qt_version().to_string()
}

/// Saves a screenshot of the first window.
pub fn grab_window(path: &str) -> bool {
    ffi::grab_window(&cxx_qt_lib::QString::from(path))
}

/// QML warnings so far that point into the app's own QML files.
pub fn qml_warning_count() -> i32 {
    ffi::qml_warning_count()
}
