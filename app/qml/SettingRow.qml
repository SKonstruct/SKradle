import QtQuick
import com.skradle.app
import QtQuick.Layouts

// Label + hint on the left, the real control on the right.
RowLayout {
    id: root
    property string label
    property string hint
    property real controlWidth: 250
    default property alias control: slot.data
    Layout.fillWidth: true
    Layout.leftMargin: Theme.s5
    Layout.rightMargin: Theme.s5
    Layout.topMargin: 14
    Layout.bottomMargin: 14
    spacing: Theme.s5

    ColumnLayout {
        Layout.fillWidth: true
        Layout.preferredWidth: 0      // identical label column on every row
        Layout.minimumWidth: 120
        Layout.alignment: Qt.AlignVCenter
        spacing: 2
        Txt { Layout.fillWidth: true; text: root.label; font.weight: Font.Medium }
        Txt { Layout.fillWidth: true; visible: root.hint.length > 0; text: root.hint; kind: "caption" }
    }
    RowLayout {
        id: slot
        Layout.preferredWidth: root.controlWidth
        Layout.maximumWidth: root.controlWidth
        Layout.minimumWidth: Math.min(root.controlWidth, 200)
        Layout.alignment: Qt.AlignVCenter | Qt.AlignRight
        spacing: Theme.s3
    }
}
