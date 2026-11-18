// `CxxQtBuilder::cc_builder` is an `unsafe fn` (the callback may break the build's own flags); the one use below
// only adds a preprocessor define.
#![allow(unsafe_code)]

use std::fs;
use std::path::Path;
use std::process::Command;

use cxx_qt_build::{CxxQtBuilder, QmlFile, QmlModule};

/// Whether the Qt in use has the QtWebView add-on (headers and library). Qt installs without it still build; the web
/// view then compiles out and the UI falls back to the system browser.
fn qt_has_webview() -> bool {
    let qmake = std::env::var("QMAKE").unwrap_or_else(|_| "qmake6".to_owned());
    let query = |name: &str| {
        Command::new(&qmake)
            .args(["-query", name])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
    };
    let (Some(libs), Some(headers)) = (query("QT_INSTALL_LIBS"), query("QT_INSTALL_HEADERS"))
    else {
        return false;
    };
    let libs = Path::new(&libs);
    let headers = Path::new(&headers);
    libs.join("QtWebView.framework").is_dir() || headers.join("QtWebView").is_dir()
}

fn main() {
    println!("cargo:rerun-if-env-changed=QMAKE");
    println!("cargo:rerun-if-changed=qml");
    println!("cargo:rerun-if-changed=cpp");
    println!("cargo:rerun-if-changed=../assets");

    // Every file in qml/ is part of the module; the ones that start with `pragma Singleton` are registered as
    // singletons.
    let mut names: Vec<String> = fs::read_dir("qml")
        .expect("the qml folder")
        .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
        .filter(|name| name.ends_with(".qml"))
        .collect();
    names.sort();
    let mut module = QmlModule::new("com.skradle.app");
    for name in names {
        let singleton = fs::read_to_string(Path::new("qml").join(&name))
            .is_ok_and(|text| text.lines().any(|l| l.trim() == "pragma Singleton"));
        module = module.qml_file(QmlFile::from(format!("qml/{name}")).singleton(singleton));
    }

    let webview = qt_has_webview();
    println!(
        "cargo:warning=QtWebView {}",
        if webview {
            "found: the web view is built in"
        } else {
            "not found: \"My server\" opens in the system browser"
        }
    );
    let mut builder = CxxQtBuilder::new_qml_module(module)
        .qt_module("Quick")
        .qt_module("QuickControls2")
        .qt_module("QuickShapes")
        .qt_module("QuickDialogs2")
        .qrc("resources.qrc")
        .cpp_file("cpp/glue.cpp")
        .include_dir("cpp")
        .files([
            "src/bridge/app_env.rs",
            "src/bridge/glue.rs",
            "src/bridge/settings_model.rs",
            "src/bridge/game_model.rs",
            "src/bridge/launch_model.rs",
            "src/bridge/update_model.rs",
            "src/bridge/server_host_model.rs",
        ]);
    if webview {
        builder = builder.qt_module("WebView");
        // SAFETY: only a define is added.
        builder = unsafe {
            builder.cc_builder(|cc| {
                cc.define("SKRADLE_WEBVIEW", None);
            })
        };
    }
    builder.build();
}
