// Spike: the same settings page with Qt Quick Controls. Usage: nl_qml [--accent] [--shot out.png]
// The style comes from QT_QUICK_CONTROLS_STYLE (default = the platform's native style).
#include <QFile>
#include <QGuiApplication>
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QQuickStyle>
#include <QQuickWindow>
#include <QTimer>

int main(int argc, char **argv) {
    QGuiApplication app(argc, argv);
    QString shot;
    bool accent = false;
    const QStringList args = app.arguments();
    for (int i = 1; i < args.size(); ++i) {
        if (args[i] == "--accent") accent = true;
        else if (args[i] == "--shot" && i + 1 < args.size()) shot = args[++i];
    }
    QQmlApplicationEngine engine;
    engine.rootContext()->setContextProperty("useAccent", accent);
    engine.rootContext()->setContextProperty("styleName", QQuickStyle::name());
    QObject::connect(&engine, &QQmlApplicationEngine::objectCreationFailed, &app, [] { QCoreApplication::exit(1); }, Qt::QueuedConnection);
    engine.loadFromModule("Spike", "Main");
    if (!shot.isEmpty()) {
        QTimer::singleShot(900, [&] {
            auto *win = qobject_cast<QQuickWindow *>(engine.rootObjects().value(0));
            const bool saved = win && win->grabWindow().save(shot);
            QFile f(shot + ".status");
            if (f.open(QIODevice::WriteOnly)) f.write(saved ? "ok" : "failed");
            app.quit();
        });
    }
    return app.exec();
}
