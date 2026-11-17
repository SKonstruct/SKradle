cmake_minimum_required(VERSION 3.21)
project(skradle_ui_shell LANGUAGES CXX)
set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(Qt6 6.8 REQUIRED COMPONENTS Quick QuickControls2 QuickShapes QuickVectorImage QuickDialogs2)
qt_standard_project_setup(REQUIRES 6.8)

set(QML_SINGLETONS qml/Theme.qml qml/Motion.qml qml/L.qml qml/Glyphs.qml qml/LaunchModel.qml qml/Data.qml)
set_source_files_properties(${QML_SINGLETONS} PROPERTIES QT_QML_SINGLETON_TYPE TRUE)

# Shared brand assets live at the repo root (assets/); embed them under the QML module prefix without copying.
get_filename_component(SK_ASSETS "${CMAKE_CURRENT_SOURCE_DIR}/../../assets" ABSOLUTE)
set_source_files_properties(${SK_ASSETS}/brand/logo.svg PROPERTIES QT_RESOURCE_ALIAS brand/logo.svg)
set_source_files_properties(${SK_ASSETS}/icons/skradle-256.png PROPERTIES QT_RESOURCE_ALIAS icons/skradle-256.png)

qt_add_executable(skradle_ui src/main.cpp)
qt_add_qml_module(skradle_ui
    URI SKradleUi
    VERSION 1.0
    SOURCES src/appenv.h src/appenv.cpp
    RESOURCES ${SK_ASSETS}/brand/logo.svg ${SK_ASSETS}/icons/skradle-256.png
    QML_FILES
        ${QML_SINGLETONS}
        qml/Main.qml
        qml/Txt.qml qml/Icon.qml qml/Logo.qml qml/Card.qml qml/FocusRing.qml qml/Divider.qml
        qml/FadeText.qml qml/IconButton.qml qml/Chip.qml qml/StatusDot.qml qml/SignalBars.qml
        qml/PageHeader.qml qml/PageScaffold.qml qml/SettingRow.qml qml/LockedOverlay.qml
        qml/Sidebar.qml qml/NavItem.qml qml/ProgressRing.qml qml/LaunchButton.qml
        qml/NewsCard.qml qml/Toast.qml qml/Tip.qml
        qml/Glyph.qml qml/SButton.qml qml/SSwitch.qml qml/SCheckBox.qml qml/SSlider.qml qml/STextField.qml
        qml/SScrollBar.qml qml/SSpinBox.qml qml/SItemDelegate.qml qml/SComboBox.qml
        qml/SMenu.qml qml/SMenuItem.qml qml/SMenuSeparator.qml
        qml/HomePage.qml qml/ServersPage.qml qml/ServerCard.qml qml/AddServerDialog.qml
        qml/ModsPage.qml qml/ModRow.qml qml/DropZone.qml
        qml/GraphicsPage.qml qml/AboutPage.qml qml/GameNotFound.qml
)
target_link_libraries(skradle_ui PRIVATE Qt6::Quick Qt6::QuickControls2 Qt6::QuickShapes Qt6::QuickVectorImage Qt6::QuickDialogs2)
target_include_directories(skradle_ui PRIVATE src)
