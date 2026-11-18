//! SKradle: the desktop launcher. A Qt Quick UI (embedded QML) on top of `skradle-core`, bridged with cxx-qt.

mod bridge;
mod discovery;
mod launch;
mod options;
mod reporter;
mod selfupdate;
mod server_host;
mod settings;

use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QQuickStyle, QString, QUrl};

use options::Options;

const MAIN_QML: &str = "qrc:/qt/qml/com/skradle/app/qml/Main.qml";

fn main() {
    let reduce = std::env::var("SKRADLE_REDUCE_MOTION").is_ok_and(|v| !v.is_empty() && v != "0");
    let options = match Options::parse(std::env::args().skip(1), reduce) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("skradle: {message}");
            std::process::exit(2);
        }
    };
    options::set(options.clone());

    if let Some(dir) = &options.shots {
        if let Err(e) = std::fs::create_dir_all(dir) {
            eprintln!("skradle: cannot create {}: {e}", dir.display());
            std::process::exit(2);
        }
    }

    // Tests and screenshot runs never touch the player's own settings; their throw-away folder goes at the end.
    let mut run_dir = None;
    if (options.selftest || options.shots.is_some())
        && std::env::var_os("SKRADLE_CONFIG_DIR").is_none()
    {
        let dir = std::env::temp_dir().join(format!("skradle-run-{}", std::process::id()));
        std::env::set_var("SKRADLE_CONFIG_DIR", &dir);
        run_dir = Some(dir);
    }
    // No reports from the self-test or screenshot runs, which are not a player's session.
    reporter::init(
        settings::global().get().crash_reports && !(options.selftest || options.shots.is_some()),
    );

    // The Basic style looks the same on every OS and all visible controls are our own templates.
    if let Some(style) = &options.style {
        QQuickStyle::set_style(&QString::from(style.as_str()));
    } else if std::env::var_os("QT_QUICK_CONTROLS_STYLE").is_none() {
        QQuickStyle::set_style(&QString::from("Basic"));
    }

    // A watchdog for the self-test: a QML load error leaves the event loop running with no window.
    if options.selftest {
        std::thread::spawn(|| {
            std::thread::sleep(std::time::Duration::from_secs(60));
            eprintln!("selftest: FAILED (timed out)");
            std::process::exit(1);
        });
    }

    // Qt WebView needs this before the application object exists.
    bridge::glue::init_webview();
    let mut app = QGuiApplication::new();
    bridge::glue::init_app();
    options::set_style_name(QQuickStyle::name().to_string());

    let mut engine = QQmlApplicationEngine::new();
    if let Some(mut engine) = engine.as_mut() {
        bridge::glue::register_engine(engine.as_mut());
        bridge::glue::set_language(&bridge::settings_model::language_for(
            &settings::global().get().language,
        ));
        engine.as_mut().load(&QUrl::from(MAIN_QML));
    }
    // A QML load error leaves no window; without this check the event loop would run forever, windowless.
    if !bridge::glue::has_window() {
        eprintln!("skradle: the interface could not be loaded (see the messages above)");
        settings::global().flush();
        remove_run_dir(&run_dir);
        std::process::exit(1);
    }
    let status = app.as_mut().map_or(1, |app| app.exec());
    settings::global().flush();
    // Destroy the QML engine (and any web view in it) and then the application object before `exit` runs
    // the C++ static destructors; with them alive Qt crashes on the way out.
    drop(engine);
    drop(app);
    remove_run_dir(&run_dir);
    std::process::exit(status);
}

/// Deletes the settings folder a self-test or screenshot run made for itself, if it made one.
fn remove_run_dir(dir: &Option<std::path::PathBuf>) {
    if let Some(dir) = dir {
        let _ = std::fs::remove_dir_all(dir);
    }
}
