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
        fn register_engine(engine: Pin<&mut QQmlApplicationEngine>);
        #[namespace = "skradle"]
        fn qt_version() -> QString;
        #[namespace = "skradle"]
        fn set_language(language: &QString) -> QString;
    }
}

pub use ffi::{init_app, register_engine};

/// Installs the translator for `language`; returns the catalogue in use (empty for the English source text).
pub fn set_language(language: &str) -> String {
    ffi::set_language(&cxx_qt_lib::QString::from(language)).to_string()
}

/// The Qt version in use.
pub fn qt_version() -> String {
    ffi::qt_version().to_string()
}
