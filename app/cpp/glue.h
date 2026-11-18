// The few Qt calls cxx-qt-lib does not wrap: the window icon, the translators and the QML engine's retranslate.
#pragma once

#include <QtCore/QString>
#include <QtQml/QQmlApplicationEngine>

namespace skradle {

// Application name, organisation and window icon. Call once, after the QGuiApplication exists.
void init_app();

// The engine to retranslate when the language changes.
void register_engine(QQmlApplicationEngine &engine);

// The Qt version the program runs with, for example "6.11.3".
QString qt_version();

// Installs the translator for `language` ("system", a locale such as "de", or "pseudo" for the pseudo-localisation
// test mode) and retranslates the UI. Returns the name of the catalogue in use, or an empty string when the English
// source text is shown.
QString set_language(const QString &language);

}  // namespace skradle
