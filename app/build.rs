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

/// Compiles the translation catalogues (`i18n/*.ts`) with Qt's `lrelease` and returns a resource file that embeds
/// the results under `:/i18n/`. Without `lrelease` the build still works and the UI stays in English.
fn compile_translations(out_dir: &Path) -> Option<std::path::PathBuf> {
    println!("cargo:rerun-if-changed=i18n");
    let qmake = std::env::var("QMAKE").unwrap_or_else(|_| "qmake6".to_owned());
    let bins = Command::new(&qmake)
        .args(["-query", "QT_HOST_BINS"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())?;
    let lrelease = Path::new(&bins).join(if cfg!(windows) {
        "lrelease.exe"
    } else {
        "lrelease"
    });
    if !lrelease.is_file() {
        println!("cargo:warning=lrelease not found in {bins}: translations are not embedded");
        return None;
    }
    let mut files = String::new();
    let mut sources: Vec<_> = fs::read_dir("i18n")
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "ts"))
        .collect();
    sources.sort();
    for source in sources {
        let qm = out_dir.join(source.file_stem()?).with_extension("qm");
        let status = Command::new(&lrelease)
            .arg("-silent")
            .arg(&source)
            .arg("-qm")
            .arg(&qm)
            .status()
            .ok()?;
        assert!(status.success(), "lrelease failed for {}", source.display());
        files.push_str(&format!(
            "    <file alias=\"{}\">{}</file>\n",
            qm.file_name()?.to_string_lossy(),
            qm.display()
        ));
    }
    let qrc = out_dir.join("i18n.qrc");
    fs::write(
        &qrc,
        format!("<RCC>\n  <qresource prefix=\"/i18n\">\n{files}  </qresource>\n</RCC>\n"),
    )
    .ok()?;
    Some(qrc)
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
    if let Some(qrc) = compile_translations(Path::new(&std::env::var("OUT_DIR").expect("OUT_DIR")))
    {
        builder = builder.qrc(qrc);
    }
    builder.build();
}
