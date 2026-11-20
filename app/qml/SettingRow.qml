import QtQuick
import com.skradle.app
import QtQuick.Layouts

// Label + hint on the left, the real control on the right. One control column (280 px) on every row, so selects,
// fields and the switch edge line up; long values (paths) wrap in the hint, they do not widen the column.
RowLayout {
    id: root
    property string label
    property string hint
    property real controlWidth: 280
    default property alias control: slot.data
    Layout.fillWidth: true
    Layout.leftMargin: Theme.s5
    Layout.rightMargin: Theme.s5
    Layout.topMargin: Theme.rowV
    Layout.bottomMargin: Theme.rowV
    Layout.minimumHeight: Theme.hRow - 2 * Theme.rowV
    spacing: Theme.s5

    ColumnLayout {
        Layout.fillWidth: true
        Layout.preferredWidth: 0      // identical label column on every row
        Layout.minimumWidth: 160
        Layout.alignment: Qt.AlignVCenter
        spacing: Theme.s0
        Txt { Layout.fillWidth: true; text: root.label; font.weight: Font.Medium }
        Txt { Layout.fillWidth: true; visible: root.hint.length > 0; text: root.hint; kind: "caption" }
    }
    RowLayout {
        id: slot
        Layout.preferredWidth: root.controlWidth
        Layout.maximumWidth: root.controlWidth
        Layout.minimumWidth: Math.min(root.controlWidth, 160)
        Layout.alignment: Qt.AlignVCenter | Qt.AlignRight
        spacing: Theme.s3
    }
}
