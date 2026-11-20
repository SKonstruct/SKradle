import QtQuick
import com.skradle.app
import QtQuick.Layouts

// Page title (24, semibold), a subtitle only when it informs, actions on the right (at most two buttons).
// With no title and no actions the header takes no space.
RowLayout {
    id: root
    property string title
    property string subtitle
    default property alias actions: actionRow.data
    spacing: Theme.s4
    visible: title.length > 0 || actionRow.children.length > 0

    ColumnLayout {
        Layout.fillWidth: true
        spacing: Theme.s1
        visible: root.title.length > 0
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
    Item { Layout.fillWidth: true; visible: root.title.length === 0 }
    RowLayout {
        id: actionRow
        Layout.alignment: Qt.AlignTop | Qt.AlignRight
        spacing: Theme.s2
    }
}
