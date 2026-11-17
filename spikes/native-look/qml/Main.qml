import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ApplicationWindow {
    id: win
    visible: true
    width: 760
    height: 520
    title: "SKradle spike (QML, style: " + styleName + ")"
    palette.highlight: useAccent ? "#AB4A81" : palette.highlight
    palette.highlightedText: useAccent ? "white" : palette.highlightedText

    RowLayout {
        anchors.fill: parent
        anchors.margins: 12
        spacing: 12

        ListView {
            Layout.preferredWidth: 150
            Layout.fillHeight: true
            model: ["Launcher", "Mods", "Servers", "Graphics", "About"]
            currentIndex: 3
            clip: true
            delegate: ItemDelegate {
                width: ListView.view.width
                text: modelData
                highlighted: ListView.isCurrentItem
            }
        }

        ColumnLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            spacing: 10

            GroupBox {
                title: "Graphics enhancements"
                Layout.fillWidth: true
                GridLayout {
                    anchors.fill: parent
                    columns: 2
                    columnSpacing: 16
                    rowSpacing: 8
                    Label { text: "Render scale" }
                    Slider { from: 50; to: 200; value: 100; Layout.fillWidth: true }
                    Label { text: "Sharpening" }
                    Slider { from: 0; to: 100; value: 40; Layout.fillWidth: true }
                    Label { text: "Frame generation mode" }
                    ComboBox { model: ["Adaptive", "Fixed multiplier"]; Layout.fillWidth: true }
                    Label { text: "Target frame rate" }
                    SpinBox { from: 30; to: 360; value: 120; editable: true }
                    Label { text: "Sharp HUD" }
                    CheckBox { text: "Draw the interface at full resolution" }
                    Label { text: "FPS counter" }
                    CheckBox { text: "Show FPS counter"; checked: true }
                    Label { text: "Java path" }
                    TextField { text: "/usr/bin/java"; Layout.fillWidth: true }
                }
            }

            ProgressBar { from: 0; to: 100; value: 62; Layout.fillWidth: true }

            RowLayout {
                Layout.fillWidth: true
                Item { Layout.fillWidth: true }
                Button { text: "Reset" }
                Button { text: "Save"; highlighted: true }
            }
            Item { Layout.fillHeight: true }
        }
    }
}
