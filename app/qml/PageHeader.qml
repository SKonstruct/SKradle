import QtQuick
import com.skradle.app
import QtQuick.Layouts

RowLayout {
    id: root
    property string title
    property string subtitle
    default property alias actions: actionRow.data
    spacing: Theme.s4

    ColumnLayout {
        Layout.fillWidth: true
        spacing: 4
        Txt {
            Layout.fillWidth: true
            text: root.title
            kind: "title"
            Accessible.role: Accessible.Heading
        }
        Txt {
            Layout.fillWidth: true
            visible: root.subtitle.length > 0
            text: root.subtitle
            kind: "body"
            color: Theme.textSecondary
        }
    }
    RowLayout {
        id: actionRow
        Layout.alignment: Qt.AlignTop | Qt.AlignRight
        spacing: Theme.s2
    }
}
