import QtQuick
import com.skradle.app
import QtQuick.Layouts

// One titled card of the Settings page. Children go below the heading, usually SettingRow and Divider items.
Card {
    id: root
    property string title
    default property alias content: body.data
    Layout.fillWidth: true
    implicitHeight: col.implicitHeight

    ColumnLayout {
        id: col
        width: parent.width
        spacing: 0
        Txt {
            Layout.margins: Theme.padCard
            Layout.bottomMargin: Theme.s1
            text: root.title
            kind: "heading"
            Accessible.role: Accessible.Heading
        }
        ColumnLayout {
            id: body
            Layout.fillWidth: true
            spacing: 0
        }
        Item { Layout.preferredHeight: Theme.s2 }
    }
}
