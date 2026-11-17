cmake_minimum_required(VERSION 3.21)
project(skradle_ui_shell LANGUAGES CXX)
set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(Qt6 6.8 REQUIRED COMPONENTS Quick QuickControls2 QuickShapes)
qt_standard_project_setup(REQUIRES 6.8)

set(QML_SINGLETONS qml/Theme.qml qml/Motion.qml qml/L.qml qml/Glyphs.qml qml/LaunchModel.qml qml/Data.qml)
set_source_files_properties(${QML_SINGLETONS} PROPERTIES QT_QML_SINGLETON_TYPE TRUE)

qt_add_executable(skradle_ui src/main.cpp)
qt_add_qml_module(skradle_ui
    URI SKradleUi
    VERSION 1.0
    SOURCES src/appenv.h src/appenv.cpp
    QML_FILES
        ${QML_SINGLETONS}
        qml/Main.qml
        qml/Txt.qml qml/Icon.qml qml/Logo.qml qml/Card.qml qml/FocusRing.qml qml/Divider.qml
        qml/FadeText.qml qml/IconButton.qml qml/Chip.qml qml/StatusDot.qml qml/SignalBars.qml
        qml/PageHeader.qml qml/PageScaffold.qml qml/SettingRow.qml qml/LockedOverlay.qml
        qml/Sidebar.qml qml/NavItem.qml qml/ProgressRing.qml qml/LaunchButton.qml
        qml/NewsCard.qml qml/Toast.qml qml/Tip.qml
        qml/HomePage.qml qml/ServersPage.qml qml/ServerCard.qml qml/AddServerDialog.qml
        qml/ModsPage.qml qml/ModRow.qml qml/DropZone.qml
        qml/GraphicsPage.qml qml/AboutPage.qml qml/GameNotFound.qml
)
target_link_libraries(skradle_ui PRIVATE Qt6::Quick Qt6::QuickControls2 Qt6::QuickShapes)
target_include_directories(skradle_ui PRIVATE src)
