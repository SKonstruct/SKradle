import QtQuick
import com.skradle.app

// Small static label: "Update", "Needs Java 25". Not clickable. kind: neutral | accent | warn.
Rectangle {
    id: root
    property string text
    property string kind: "neutral"
    implicitWidth: label.implicitWidth + 2 * Theme.s2
    implicitHeight: label.implicitHeight + Theme.s1
    radius: Theme.rXs
    color: kind === "accent" ? Theme.accentTint : kind === "warn" ? Theme.warnTint : Theme.raised
    Accessible.role: Accessible.StaticText
    Accessible.name: text
    Txt {
        id: label
        anchors.fill: parent
        horizontalAlignment: Text.AlignHCenter
        verticalAlignment: Text.AlignVCenter
        text: root.text
        kind: "micro"
        wrapMode: Text.NoWrap
        color: root.kind === "warn" ? Theme.amberText : root.kind === "accent" ? Theme.text : Theme.textSecondary
    }
}
