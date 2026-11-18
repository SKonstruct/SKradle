//! SKradle: the desktop launcher. A Qt Quick UI (embedded QML) on top of `skradle-core`, bridged with cxx-qt.

mod bridge;
mod discovery;
mod launch;
mod options;
mod reporter;
mod selfupdate;
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

    // Tests and screenshot runs never touch the player's own settings.
    if (options.selftest || options.shots.is_some())
        && std::env::var_os("SKRADLE_CONFIG_DIR").is_none()
    {
        let dir = std::env::temp_dir().join(format!("skradle-run-{}", std::process::id()));
        std::env::set_var("SKRADLE_CONFIG_DIR", dir);
    }
    reporter::init(settings::global().get().crash_reports && !options.selftest);

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
    let status = app.as_mut().map_or(1, |app| app.exec());
    settings::global().flush();
    std::process::exit(status);
}
