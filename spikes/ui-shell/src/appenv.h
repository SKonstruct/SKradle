// Process-wide switches (flags/env) exposed to QML, plus the frame-time recorder behind --bench.
#pragma once
#include <QObject>
#include <QString>
#include <atomic>
#include <cstdint>
#include <utility>
#include <vector>

class QQuickWindow;
class QQmlEngine;
class QJSEngine;

class AppEnv : public QObject {
    Q_OBJECT
    Q_PROPERTY(bool pseudo MEMBER m_pseudo CONSTANT)
    Q_PROPERTY(bool reduceMotion MEMBER m_reduceMotion CONSTANT)
    Q_PROPERTY(bool bench MEMBER m_bench CONSTANT)
    Q_PROPERTY(QString scheme MEMBER m_scheme CONSTANT)  // "", "light" or "dark" (forced)
    Q_PROPERTY(QString styleName MEMBER m_style CONSTANT)
    Q_PROPERTY(bool nativeStyle MEMBER m_native CONSTANT)  // macOS / Windows / FluentWinUI3 style
    Q_PROPERTY(QString os MEMBER m_os CONSTANT)
    Q_PROPERTY(QString qtVersion MEMBER m_qt CONSTANT)
    Q_PROPERTY(int initialPage MEMBER m_page CONSTANT)
    Q_PROPERTY(bool initialCollapsed MEMBER m_collapsed CONSTANT)
    Q_PROPERTY(int initialWidth MEMBER m_w CONSTANT)
    Q_PROPERTY(int initialHeight MEMBER m_h CONSTANT)

public:
    explicit AppEnv(QObject *parent = nullptr);
    static AppEnv *instance();

    bool m_pseudo = false, m_reduceMotion = false, m_bench = false, m_native = false, m_collapsed = false;
    QString m_scheme, m_style, m_os, m_qt;
    int m_page = 0, m_w = 1180, m_h = 780;

    void attach(QQuickWindow *w);  // start recording frameSwapped
    Q_INVOKABLE void benchPhase(const QString &name);
    Q_INVOKABLE void benchFinish();

private:
    void report();
    QQuickWindow *m_win = nullptr;
    std::vector<int64_t> m_ts;  // ns, written only from the render thread
    std::atomic<size_t> m_n{0};
    std::vector<std::pair<QString, size_t>> m_phases;
};
