use std::fs;
use std::path::Path;

use cxx_qt_build::{CxxQtBuilder, QmlFile, QmlModule};

fn main() {
    println!("cargo:rerun-if-changed=qml");
    println!("cargo:rerun-if-changed=cpp");
    println!("cargo:rerun-if-changed=../assets");

    // Every file in qml/ is part of the module; the ones that start with `pragma Singleton` are registered as
    // singletons.
    let mut names: Vec<String> = fs::read_dir("qml")
        .expect("the qml folder")
        .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
        .filter(|name| name.ends_with(".qml"))
        .collect();
    names.sort();
    let mut module = QmlModule::new("com.skradle.app");
    for name in names {
        let singleton = fs::read_to_string(Path::new("qml").join(&name))
            .is_ok_and(|text| text.lines().any(|l| l.trim() == "pragma Singleton"));
        module = module.qml_file(QmlFile::from(format!("qml/{name}")).singleton(singleton));
    }

    CxxQtBuilder::new_qml_module(module)
        .qt_module("Quick")
        .qt_module("QuickControls2")
        .qt_module("QuickShapes")
        .qt_module("QuickDialogs2")
        .qrc("resources.qrc")
        .cpp_file("cpp/glue.cpp")
        .include_dir("cpp")
        .files([
            "src/bridge/app_env.rs",
            "src/bridge/glue.rs",
            "src/bridge/settings_model.rs",
            "src/bridge/game_model.rs",
            "src/bridge/launch_model.rs",
            "src/bridge/update_model.rs",
        ])
        .build();
}
