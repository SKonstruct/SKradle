#include "appenv.h"

#include <QCoreApplication>
#include <QGuiApplication>
#include <QQuickWindow>
#include <QScreen>
#include <QtQml/QJSEngine>
#include <algorithm>
#include <chrono>
#include <cstdio>

static constexpr size_t kCap = 400000;

AppEnv::AppEnv(QObject *parent) : QObject(parent) { m_ts.resize(kCap); }

AppEnv *AppEnv::instance() {
    static AppEnv *inst = new AppEnv;
    return inst;
}

void AppEnv::attach(QQuickWindow *w) {
    m_win = w;
    if (!m_bench) return;
    // frameSwapped is emitted on the render thread; single writer, read after benchFinish().
    connect(w, &QQuickWindow::frameSwapped, this, [this] {
        const int64_t now = std::chrono::duration_cast<std::chrono::nanoseconds>(
                                std::chrono::steady_clock::now().time_since_epoch()).count();
        const size_t n = m_n.load(std::memory_order_relaxed);
        if (n < kCap) {
            m_ts[n] = now;
            m_n.store(n + 1, std::memory_order_release);
        }
    }, Qt::DirectConnection);
}

void AppEnv::benchPhase(const QString &name) { m_phases.emplace_back(name, m_n.load(std::memory_order_acquire)); }

void AppEnv::benchFinish() {
    m_phases.emplace_back(QStringLiteral("end"), m_n.load(std::memory_order_acquire));
    report();
    QCoreApplication::exit(0);
}

void AppEnv::report() {
    const double hz = m_win && m_win->screen() ? m_win->screen()->refreshRate() : 0;
    const double nominal = hz > 0 ? 1000.0 / hz : 16.67;
    std::printf("[bench] style=%s os=%s qt=%s window=%dx%d dpr=%.1f screen_refresh=%.1fHz (nominal %.2f ms)\n",
                qPrintable(m_style), qPrintable(m_os), qPrintable(m_qt),
                m_win ? m_win->width() : 0, m_win ? m_win->height() : 0, m_win ? m_win->devicePixelRatio() : 1.0,
                hz, nominal);
    std::printf("[bench] a continuous 2px animator keeps frames flowing, so every interval is a real frame\n");
    size_t allBad15 = 0, allFrames = 0;
    double allMax = 0;
    for (size_t p = 0; p + 1 < m_phases.size(); ++p) {
        const QString &name = m_phases[p].first;
        if (name == QLatin1String("warmup")) continue;
        const size_t a = m_phases[p].second, b = m_phases[p + 1].second;
        std::vector<double> d;
        for (size_t i = a + 1; i < b; ++i) d.push_back((m_ts[i] - m_ts[i - 1]) / 1e6);
        if (d.empty()) { std::printf("[bench] phase=%-18s no frames\n", qPrintable(name)); continue; }
        std::vector<double> s = d;
        std::sort(s.begin(), s.end());
        double sum = 0;
        for (double v : d) sum += v;
        auto pct = [&](double q) { return s[std::min(s.size() - 1, size_t(q * s.size()))]; };
        size_t bad15 = 0, bad2 = 0;
        const double med = pct(0.5);
        for (double v : d) { if (v > med * 1.5) ++bad15; if (v > med * 2.0) ++bad2; }
        std::printf("[bench] phase=%-18s frames=%zu avg=%.2fms p50=%.2f p95=%.2f p99=%.2f worst=%.2fms  >1.5x_median=%zu >2x_median=%zu\n",
                    qPrintable(name), d.size(), sum / d.size(), pct(0.5), pct(0.95), pct(0.99), s.back(), bad15, bad2);
        allBad15 += bad15; allFrames += d.size(); allMax = std::max(allMax, s.back());
    }
    std::printf("[bench] TOTAL frames=%zu worst=%.2fms frames_over_1.5x_phase_median=%zu\n", allFrames, allMax, allBad15);
    std::fflush(stdout);
}
