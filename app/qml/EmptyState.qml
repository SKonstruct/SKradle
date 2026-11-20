import QtQuick
import com.skradle.app
import QtQuick.Layouts

// Nothing to show yet, or nothing matches, or offline. One sentence of why, one primary action, an optional link.
//   EmptyState { glyph: "mods"; title: qsTr("No mods yet"); text: qsTr("..."); actionText: qsTr("Add mods"); onAction: ... }
// Fills the width it is given and is 120-200 px tall; it is not a dashed drop zone.
Item {
    id: root
    property string glyph: "info"
    property string title
    property string text
    property string actionText
    property string actionGlyph
    property string linkText
    signal action()
    signal link()

    Layout.fillWidth: true
    implicitHeight: col.implicitHeight + 2 * Theme.s6
    Accessible.role: Accessible.Grouping
    Accessible.name: title

    ColumnLayout {
        id: col
        width: Math.min(parent.width - 2 * Theme.s4, Theme.readMeasure)
        anchors.horizontalCenter: parent.horizontalCenter
        y: Theme.s6
        spacing: Theme.s2
        Icon { Layout.alignment: Qt.AlignHCenter; name: root.glyph; size: 28; color: Theme.textSecondary }
        Txt { Layout.fillWidth: true; Layout.topMargin: Theme.s1; horizontalAlignment: Text.AlignHCenter; text: root.title; kind: "heading"; Accessible.role: Accessible.Heading }
        Txt { Layout.fillWidth: true; visible: root.text.length > 0; horizontalAlignment: Text.AlignHCenter; text: root.text; color: Theme.textSecondary }
        SButton {
            Layout.alignment: Qt.AlignHCenter
            Layout.topMargin: Theme.s2
            visible: root.actionText.length > 0
            variant: "primary"
            text: root.actionText
            glyph: root.actionGlyph
            onClicked: root.action()
        }
        SButton {
            Layout.alignment: Qt.AlignHCenter
            visible: root.linkText.length > 0
            variant: "ghost"
            size: "sm"
            text: root.linkText
            onClicked: root.link()
        }
    }
}
