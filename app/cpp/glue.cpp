#include "glue.h"

#include <QtCore/QCoreApplication>
#include <QtCore/QLibrary>
#include <QtCore/QLocale>
#include <QtCore/QTranslator>
#include <QtGui/QGuiApplication>
#include <QtGui/QIcon>
#include <QtGui/QImage>
#include <QtQuick/QQuickWindow>
#include <QtCore/QDir>
#include <QtCore/QPluginLoader>
#include <QtCore/QFileInfo>
#include <QtCore/QPointer>
#include <atomic>
#include <mutex>
#ifdef SKRADLE_WEBVIEW
#include <QtWebView/QtWebView>
#endif
#include <cstdio>
#include <memory>

namespace {

// A QPointer: it becomes null when the engine is destroyed, so a late call never reaches a dead engine.
QPointer<QQmlApplicationEngine> g_engine;
std::atomic<int> g_qml_warnings{0};
std::atomic<QtMessageHandler> g_previous_handler{nullptr};

// Counts warnings that point into our own QML files (binding errors, undefined values, missing types), so the
// self-test can fail on them. Everything is still passed on to Qt's normal output.
void message_handler(QtMsgType type, const QMessageLogContext &context, const QString &message) {
    // Our QML lives under qrc:/qt/qml/com/skradle/app/ (qml/ and web/); Qt puts the file in the message or in the context.
    const QLatin1String ours("qrc:/qt/qml/com/skradle/app/");
    const bool in_our_qml = message.startsWith(ours) || (context.file && QString::fromUtf8(context.file).startsWith(ours));
    if ((type == QtWarningMsg || type == QtCriticalMsg) && in_our_qml) {
        ++g_qml_warnings;
    }
    if (const QtMessageHandler previous = g_previous_handler.load()) previous(type, context, message);
    else std::fprintf(stderr, "%s\n", qPrintable(message));
}
std::unique_ptr<QTranslator> g_translator;

// Pseudo-localisation: every word grows by about a third and gets accents, which shows clipped or fixed-width text
// at once. Placeholders (%1) and words with digits keep their shape so the strings still work.
class PseudoTranslator : public QTranslator {
public:
    QString translate(const char *context, const char *source, const char *disambiguation, int n) const override {
        Q_UNUSED(context);
        Q_UNUSED(disambiguation);
        Q_UNUSED(n);
        if (!source || !*source) return QString();
        const QString text = QString::fromUtf8(source);
        static const QString accentsFrom = QStringLiteral("aeiouAEIOU");
        // Escapes, not raw accents: MSVC reads a source file as the ANSI code page unless told it is UTF-8.
        static const QString accentsTo = QStringLiteral(u"\u00e1\u00e9\u00ed\u00f6\u00fc\u00c5\u00c9\u00cd\u00d6\u00dc");
        static const QString filler = QStringLiteral(u"\u00df\u00fc\u00f6\u00e4enschaft");
        QStringList words = text.split(QLatin1Char(' '));
        for (QString &word : words) {
            if (word.isEmpty() || word.contains(QLatin1Char('%'))) continue;
            QString out;
            for (const QChar c : word) {
                const qsizetype i = accentsFrom.indexOf(c);
                out += i >= 0 ? accentsTo.at(i) : c;
            }
            const int extra = qMax(1, qRound(word.size() * 0.35));
            for (int j = 0; j < extra; ++j) out += filler.at(j % filler.size());
            word = out;
        }
        return words.join(QLatin1Char(' '));
    }
    bool isEmpty() const override { return false; }
};

}  // namespace

namespace skradle {

void init_webview() {
#ifdef SKRADLE_WEBVIEW
    QtWebView::initialize();
#endif
}

bool webview_backend_available() {
#ifdef SKRADLE_WEBVIEW
    // Native web views are child windows of the real window; platforms without foreign windows (offscreen, the
    // headless self-test) cannot host one.
    const QString platform = QGuiApplication::platformName();
    if (platform == QLatin1String("offscreen") || platform == QLatin1String("minimal")) return false;
    // The web view loads one of the plugins in <plugins>/webview. A plugin file can be present and still unusable
    // (the WebEngine plugin without the WebEngine libraries), so each one is really loaded.
    for (const QString &path : QCoreApplication::libraryPaths()) {
        const QDir dir(path + QStringLiteral("/webview"));
        for (const QString &file : dir.entryList(QDir::Files)) {
            // Only real plugin libraries (not a stray text or debug-symbol file), and only those that load.
            if (!QLibrary::isLibrary(file)) continue;
            QPluginLoader loader(dir.absoluteFilePath(file));
            if (loader.load()) return true;
        }
    }
#endif
    return false;
}

void init_app() {
    // Once only: a second call would make the handler its own predecessor and recurse forever.
    static std::once_flag once;
    std::call_once(once, [] { g_previous_handler.store(qInstallMessageHandler(message_handler)); });
    // The translator is a QObject and must not outlive the application object (static destruction runs after it).
    QObject::connect(QCoreApplication::instance(), &QCoreApplication::aboutToQuit, [] {
        if (g_translator) {
            QCoreApplication::removeTranslator(g_translator.get());
            g_translator.reset();
        }
    });
    QCoreApplication::setApplicationName(QStringLiteral("SKradle"));
    QCoreApplication::setOrganizationName(QStringLiteral("SKradle"));
    QCoreApplication::setOrganizationDomain(QStringLiteral("skradle.invalid"));
    QIcon icon;
    for (const int size : {16, 24, 32, 48, 64, 128, 256, 512})
        icon.addFile(QStringLiteral(":/qt/qml/com/skradle/app/icons/skradle-%1.png").arg(size));
    QGuiApplication::setWindowIcon(icon);
}

void register_engine(QQmlApplicationEngine &engine) { g_engine = &engine; }

bool has_window() { return g_engine && !g_engine->rootObjects().isEmpty(); }

bool grab_window(const QString &path) {
    if (!g_engine) return false;
    auto *window = qobject_cast<QQuickWindow *>(g_engine->rootObjects().value(0));
    if (!window) return false;
    // The screenshot folder may not exist yet.
    QFileInfo(path).dir().mkpath(QStringLiteral("."));
    return window->grabWindow().save(path);
}

void quit_app(int code) { QCoreApplication::exit(code); }

int qml_warning_count() { return g_qml_warnings.load(); }

QString qt_version() { return QString::fromLatin1(qVersion()); }

QString set_language(const QString &language) {
    if (g_translator) {
        QCoreApplication::removeTranslator(g_translator.get());
        g_translator.reset();
    }
    QString used;
    if (language == QLatin1String("pseudo")) {
        g_translator = std::make_unique<PseudoTranslator>();
        used = QStringLiteral("pseudo");
    } else {
        QStringList candidates;
        if (language == QLatin1String("system")) {
            candidates = QLocale::system().uiLanguages(QLocale::TagSeparator::Underscore);
        } else {
            // "de-AT" (BCP 47) is "de_AT" here, so the fallback to "de" below works for both spellings.
            QString tag = language;
            tag.replace(QLatin1Char('-'), QLatin1Char('_'));
            candidates << tag;
        }
        // "de_AT" falls back to "de".
        QStringList names;
        for (const QString &c : std::as_const(candidates)) {
            names << c;
            const qsizetype cut = c.indexOf(QLatin1Char('_'));
            if (cut > 0) names << c.left(cut);
        }
        for (const QString &name : std::as_const(names)) {
            auto translator = std::make_unique<QTranslator>();
            if (translator->load(QStringLiteral(":/i18n/skradle_%1.qm").arg(name))) {
                g_translator = std::move(translator);
                used = name;
                break;
            }
        }
    }
    if (g_translator) QCoreApplication::installTranslator(g_translator.get());
    if (g_engine) g_engine->retranslate();
    return used;
}

}  // namespace skradle
