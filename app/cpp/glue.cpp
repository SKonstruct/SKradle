#include "glue.h"

#include <QtCore/QCoreApplication>
#include <QtCore/QLocale>
#include <QtCore/QTranslator>
#include <QtGui/QGuiApplication>
#include <QtGui/QIcon>
#include <memory>

namespace {

QQmlApplicationEngine *g_engine = nullptr;
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
        static const QString accentsTo = QString::fromUtf8("áéíöüÅÉÍÖÜ");
        static const QString filler = QStringLiteral("ßüöäenschaft");
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

void init_app() {
    QCoreApplication::setApplicationName(QStringLiteral("SKradle"));
    QCoreApplication::setOrganizationName(QStringLiteral("SKradle"));
    QCoreApplication::setOrganizationDomain(QStringLiteral("skradle.invalid"));
    QIcon icon;
    for (const int size : {16, 24, 32, 48, 64, 128, 256, 512})
        icon.addFile(QStringLiteral(":/qt/qml/com/skradle/app/icons/skradle-%1.png").arg(size));
    QGuiApplication::setWindowIcon(icon);
}

void register_engine(QQmlApplicationEngine &engine) { g_engine = &engine; }

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
            candidates << language;
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
