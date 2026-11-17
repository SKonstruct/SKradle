// Spike: native look with Qt Widgets. A settings page like the planned Graphics tab.
// Usage: nl_widgets [--accent] [--style <name>] [--shot out.png]
#include <QApplication>
#include <QCheckBox>
#include <QComboBox>
#include <QFile>
#include <QFormLayout>
#include <QGroupBox>
#include <QHBoxLayout>
#include <QLineEdit>
#include <QListWidget>
#include <QPalette>
#include <QProgressBar>
#include <QPushButton>
#include <QSlider>
#include <QSpinBox>
#include <QStyle>
#include <QTimer>
#include <QVBoxLayout>
#include <QWidget>

int main(int argc, char **argv) {
    QApplication app(argc, argv);
    QString shot, styleName;
    bool accent = false;
    const QStringList args = app.arguments();
    for (int i = 1; i < args.size(); ++i) {
        if (args[i] == "--accent") accent = true;
        else if (args[i] == "--shot" && i + 1 < args.size()) shot = args[++i];
        else if (args[i] == "--style" && i + 1 < args.size()) styleName = args[++i];
    }
    if (!styleName.isEmpty()) QApplication::setStyle(styleName);
    if (accent) {
        QPalette p = app.palette();
        p.setColor(QPalette::Highlight, QColor("#AB4A81"));
        p.setColor(QPalette::HighlightedText, Qt::white);
        app.setPalette(p);
    }

    QWidget w;
    w.setWindowTitle("SKradle spike (Widgets, style: " + app.style()->objectName() + ")");
    auto *root = new QHBoxLayout(&w);

    auto *nav = new QListWidget;
    nav->addItems({"Launcher", "Mods", "Servers", "Graphics", "About"});
    nav->setCurrentRow(3);
    nav->setFixedWidth(150);
    root->addWidget(nav);

    auto *page = new QWidget;
    auto *pageLayout = new QVBoxLayout(page);

    auto *gfx = new QGroupBox("Graphics enhancements");
    gfx->setCheckable(true);
    gfx->setChecked(true);
    auto *form = new QFormLayout(gfx);
    auto *scale = new QSlider(Qt::Horizontal);
    scale->setRange(50, 200);
    scale->setValue(100);
    form->addRow("Render scale", scale);
    auto *sharpen = new QSlider(Qt::Horizontal);
    sharpen->setValue(40);
    form->addRow("Sharpening", sharpen);
    auto *mode = new QComboBox;
    mode->addItems({"Adaptive", "Fixed multiplier"});
    form->addRow("Frame generation mode", mode);
    auto *target = new QSpinBox;
    target->setRange(30, 360);
    target->setValue(120);
    target->setSuffix(" fps");
    form->addRow("Target frame rate", target);
    form->addRow("Sharp HUD", new QCheckBox("Draw the interface at full resolution"));
    auto *fps = new QCheckBox("Show FPS counter");
    fps->setChecked(true);
    form->addRow("FPS counter", fps);
    form->addRow("Java path", new QLineEdit("/usr/bin/java"));
    pageLayout->addWidget(gfx);

    auto *bar = new QProgressBar;
    bar->setValue(62);
    pageLayout->addWidget(bar);

    auto *buttons = new QHBoxLayout;
    buttons->addStretch();
    buttons->addWidget(new QPushButton("Reset"));
    auto *ok = new QPushButton("Save");
    ok->setDefault(true);
    buttons->addWidget(ok);
    pageLayout->addLayout(buttons);
    pageLayout->addStretch();
    root->addWidget(page, 1);

    w.resize(760, 520);
    w.show();
    if (!shot.isEmpty()) {
        QTimer::singleShot(600, [&] {
            const bool saved = w.grab().save(shot);
            QFile f(shot + ".status");
            if (f.open(QIODevice::WriteOnly)) f.write(saved ? "ok" : "failed");
            app.quit();
        });
    }
    return app.exec();
}
