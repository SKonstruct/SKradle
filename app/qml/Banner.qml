import QtQuick
import com.skradle.app
import QtQuick.Layouts

// Inline message inside a page or card. kind: info | success | warn | danger.
// Icon + text (never colour alone), optional action button, optional dismiss.
//   Banner { kind: "danger"; text: qsTr("No Java was found."); actionText: qsTr("Open Java settings"); onAction: ... }
Rectangle {
    id: root
    property string kind: "info"
    property string text
    property string title                // optional bold first line
    property string actionText
    property bool dismissible: false
    signal action()
    signal dismissed()

    Layout.fillWidth: true
    implicitHeight: row.implicitHeight + 2 * Theme.s3
    radius: Theme.rMd
    readonly property color tone: kind === "success" ? Theme.ok : kind === "warn" ? Theme.warn : kind === "danger" ? Theme.danger : Theme.accentFg
    color: kind === "success" ? Theme.okTint : kind === "warn" ? Theme.warnTint : kind === "danger" ? Theme.dangerTint : Theme.stateHover
    border.width: 1
    border.color: kind === "info" ? Theme.border : Qt.rgba(tone.r, tone.g, tone.b, 0.35)
    Accessible.role: kind === "warn" || kind === "danger" ? Accessible.AlertMessage : Accessible.StaticText
    Accessible.name: title.length > 0 ? title + ". " + text : text

    RowLayout {
        id: row
        anchors.fill: parent
        anchors.margins: Theme.s3
        spacing: Theme.s3
        // One text line at the font's own height: the icon is centred on the first line whatever the font's metrics.
        Txt { id: probe; text: "M"; visible: false }
        Icon {
            Layout.alignment: Qt.AlignTop
            Layout.topMargin: Math.max(0, (probe.implicitHeight - 16) / 2)
            name: root.kind === "success" ? "check" : root.kind === "warn" ? "warning" : root.kind === "danger" ? "error" : "info"
            size: 16
            color: root.tone
        }
        ColumnLayout {
            Layout.fillWidth: true
            Layout.preferredWidth: 0
            Layout.alignment: Qt.AlignVCenter
            spacing: Theme.s0
            Txt { Layout.fillWidth: true; visible: root.title.length > 0; text: root.title; font.weight: Font.DemiBold }
            Txt { Layout.fillWidth: true; text: root.text; paragraph: true }
        }
        SButton {
            Layout.alignment: Qt.AlignVCenter
            visible: root.actionText.length > 0
            size: "sm"
            text: root.actionText
            onClicked: root.action()
        }
        IconButton {
            Layout.alignment: Qt.AlignVCenter
            visible: root.dismissible
            size: "sm"; glyph: "close"; tip: qsTr("Dismiss")
            onClicked: root.dismissed()
        }
    }
}
