import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import com.skradle.spike

ApplicationWindow {
    visible: true
    width: 420
    height: 220
    title: "cxx-qt spike"
    Backend { id: backend }
    Component.onCompleted: if (Qt.application.arguments.indexOf("--selftest") >= 0) {
        backend.scale = 150; backend.save(); console.log("RESULT: " + backend.status); Qt.quit()
    }
    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 16
        Slider { from: 50; to: 200; value: backend.scale; onMoved: backend.scale = Math.round(value); Layout.fillWidth: true }
        Label { text: "Render scale: " + backend.scale + "%" }
        Button { text: "Save"; highlighted: true; onClicked: backend.save() }
        Label { text: backend.status }
    }
}
