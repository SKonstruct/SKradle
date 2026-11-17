// Spike: Qt Widgets UI over a Rust core through a C interface. Usage: nl_ffi [--selftest]
#include <QApplication>
#include <QLabel>
#include <QPushButton>
#include <QSlider>
#include <QTextStream>
#include <QVBoxLayout>
#include <QWidget>
#include "skr_core.h"

static QString save(int scale) {
    char *raw = skr_describe_save(scale, "/usr/bin/java");
    const QString out = raw ? QString::fromUtf8(raw) : QStringLiteral("error");
    skr_free_string(raw);
    return out;
}

int main(int argc, char **argv) {
    QApplication app(argc, argv);
    if (app.arguments().contains("--selftest")) {
        QTextStream(stdout) << "RESULT: " << save(150) << "\n";
        return 0;
    }
    QWidget w;
    auto *l = new QVBoxLayout(&w);
    auto *s = new QSlider(Qt::Horizontal);
    s->setRange(50, 200);
    s->setValue(100);
    auto *label = new QLabel("not saved");
    auto *b = new QPushButton("Save");
    QObject::connect(b, &QPushButton::clicked, [=] { label->setText(save(s->value())); });
    l->addWidget(s);
    l->addWidget(b);
    l->addWidget(label);
    w.show();
    return app.exec();
}
