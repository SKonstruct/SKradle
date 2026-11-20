// `CxxQtBuilder::cc_builder` is an `unsafe fn` (the callback may break the build's own flags); the one use below
// only adds a preprocessor define.
#![allow(unsafe_code)]

use std::fs;
use std::path::Path;
use std::process::Command;

use cxx_qt_build::{CxxQtBuilder, QmlFile, QmlModule};

/// The `qmake` to ask about the Qt install: `QMAKE` if set, else `qmake6`, then `qmake` (the official Qt 6 packages,
/// aqtinstall and install-qt-action ship `qmake`, Linux distributions `qmake6`). The first that answers wins, in the
/// order cxx-qt's own discovery uses, so both build steps see the same Qt.
fn find_qmake() -> Option<String> {
    let from_env = std::env::var("QMAKE").ok().filter(|v| !v.is_empty());
    from_env
        .into_iter()
        .chain(["qmake6".to_owned(), "qmake".to_owned()])
        .find(|candidate| {
            Command::new(candidate)
                .args(["-query", "QT_VERSION"])
                .output()
                .is_ok_and(|o| o.status.success())
        })
}

/// Escapes text for an XML attribute or element (paths may contain `&` or `<`).
fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// Asks cargo to rerun the build script when any file under `dir` changes (a directory-level directive only sees
/// files being added or removed on some platforms, not edits).
fn watch_files(dir: &Path) {
    println!("cargo:rerun-if-changed={}", dir.display());
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            watch_files(&path);
        } else {
            println!("cargo:rerun-if-changed={}", path.display());
        }
    }
}

/// Whether the Qt in use has the QtWebView add-on (headers and library). Qt installs without it still build; the web
/// view then compiles out and the UI falls back to the system browser.
fn qt_has_webview() -> bool {
    let Some(qmake) = find_qmake() else {
        return false;
    };
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
    watch_files(Path::new("i18n"));
    let qmake = find_qmake()?;
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
            xml_escape(&qm.file_name()?.to_string_lossy()),
            xml_escape(&qm.display().to_string())
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
    println!("cargo:rerun-if-env-changed=PATH");
    watch_files(Path::new("qml"));
    watch_files(Path::new("qml-web"));
    watch_files(Path::new("cpp"));
    watch_files(Path::new("../assets"));
    println!("cargo:rerun-if-changed=resources.qrc");

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
        .qt_module("QuickVectorImage")
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
            "src/bridge/mods_model.rs",
            "src/bridge/feed_model.rs",
            "src/bridge/tools_model.rs",
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
