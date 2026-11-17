// SKradle UI shell spike.
// Flags: --scheme light|dark  --pseudo  --bench  --style <Name>  --page <0-4>  --collapsed
//        --size WxH  --shots <dir> [--prefix name]   Env: SKRADLE_REDUCE_MOTION=1
#include <QGuiApplication>
#include <QImage>
#include <QQmlApplicationEngine>
#include <QtQml/qqml.h>
#include <QQuickStyle>
#include <QQuickWindow>
#include <QStyleHints>
#include <QTimer>
#include <QDir>
#include <QFile>

#include "appenv.h"

int main(int argc, char **argv) {
    QGuiApplication app(argc, argv);
    app.setApplicationName("SKradle");
    app.setOrganizationName("SKradle");

    AppEnv &env = *AppEnv::instance();
    QString shotDir, prefix = "shot", styleArg;
    const QStringList args = app.arguments();
    auto next = [&](int &i) { return i + 1 < args.size() ? args[++i] : QString(); };
    for (int i = 1; i < args.size(); ++i) {
        const QString &a = args[i];
        if (a == "--pseudo") env.m_pseudo = true;
        else if (a == "--bench") env.m_bench = true;
        else if (a == "--collapsed") env.m_collapsed = true;
        else if (a == "--scheme") env.m_scheme = next(i);
        else if (a == "--style") styleArg = next(i);
        else if (a == "--page") env.m_page = next(i).toInt();
        else if (a == "--shots") shotDir = next(i);
        else if (a == "--prefix") prefix = next(i);
        else if (a == "--size") {
            const QStringList wh = next(i).split('x');
            if (wh.size() == 2) { env.m_w = wh[0].toInt(); env.m_h = wh[1].toInt(); }
        }
    }
    env.m_reduceMotion = qEnvironmentVariableIntValue("SKRADLE_REDUCE_MOTION") != 0;

    // Controls style: native where one exists. Nothing is set on macOS (the macOS style is the default).
    if (!styleArg.isEmpty()) QQuickStyle::setStyle(styleArg);
    else if (!qEnvironmentVariableIsSet("QT_QUICK_CONTROLS_STYLE")) {
#if defined(Q_OS_WIN)
        QQuickStyle::setStyle("FluentWinUI3");
#elif !defined(Q_OS_MACOS)
        QQuickStyle::setStyle("Fusion");
#endif
    }
    env.m_style = QQuickStyle::name();
    env.m_native = env.m_style == "macOS" || env.m_style == "Windows" || env.m_style == "FluentWinUI3";
    env.m_os = QSysInfo::prettyProductName();
    env.m_qt = QString::fromLatin1(qVersion());

    if (env.m_scheme == "dark") app.styleHints()->setColorScheme(Qt::ColorScheme::Dark);
    else if (env.m_scheme == "light") app.styleHints()->setColorScheme(Qt::ColorScheme::Light);

    qmlRegisterSingletonInstance("SKradleUi", 1, 0, "AppEnv", &env);
    QQmlApplicationEngine engine;
    QObject::connect(&engine, &QQmlApplicationEngine::objectCreationFailed, &app, [] { QCoreApplication::exit(1); },
                     Qt::QueuedConnection);
    engine.loadFromModule("SKradleUi", "Main");
    auto *win = qobject_cast<QQuickWindow *>(engine.rootObjects().value(0));
    if (!win) return 1;
    env.attach(win);

    if (!shotDir.isEmpty()) {
        QDir().mkpath(shotDir);
        const QStringList names = win->property("shotNames").toStringList();
        auto *idx = new int(0);
        auto *step = new std::function<void()>;
        *step = [=, &app]() {
            if (*idx > 0) {
                const QImage img = win->grabWindow();
                const QString path = QString("%1/%2-%3.png").arg(shotDir, prefix, names[*idx - 1]);
                if (!img.save(path)) qWarning("failed to save %s", qPrintable(path));
            }
            if (*idx >= names.size()) { app.quit(); return; }
            QMetaObject::invokeMethod(win, "setupShot", Q_ARG(QVariant, names[*idx]));
            ++*idx;
            QTimer::singleShot(850, app.instance(), [=]() { (*step)(); });
        };
        QTimer::singleShot(700, &app, [=]() { (*step)(); });
    }
    return app.exec();
}
