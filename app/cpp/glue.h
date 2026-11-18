// The few Qt calls cxx-qt-lib does not wrap: the window icon, the translators and the QML engine's retranslate.
#pragma once

#include <QtCore/QString>
#include <QtQml/QQmlApplicationEngine>

namespace skradle {

// Qt WebView must be initialised before the QGuiApplication is created. A no-op when this build has no QtWebView.
void init_webview();

// Whether a web view backend plugin is installed (the platform's own on macOS and Windows; WebEngine's on Linux).
bool webview_backend_available();

// Application name, organisation and window icon. Call once, after the QGuiApplication exists.
void init_app();

// The engine to retranslate when the language changes.
void register_engine(QQmlApplicationEngine &engine);

// Whether the engine loaded a root object (a window). False after a QML load error.
bool has_window();

// Saves a screenshot of the first window to `path` (PNG). Returns false when there is no window or saving failed.
bool grab_window(const QString &path);

// Ends the event loop; `exec()` returns `code`. The self-test uses this instead of `exit()` so that the QML engine
// and its web view are destroyed in order (calling `exit()` with them alive crashes in Qt's static destructors).
void quit_app(int code);

// Number of QML/Qt warnings seen so far whose text points into the app's QML ("qrc:/..."); the self-test fails on any.
int qml_warning_count();

// The Qt version the program runs with, for example "6.11.3".
QString qt_version();

// Installs the translator for `language` ("system", a locale such as "de", or "pseudo" for the pseudo-localisation
// test mode) and retranslates the UI. Returns the name of the catalogue in use, or an empty string when the English
// source text is shown.
QString set_language(const QString &language);

}  // namespace skradle
