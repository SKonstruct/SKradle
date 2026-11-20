import QtQuick
import com.skradle.app
import QtQuick.Controls

// A jump link of the Settings page: the current section is tinted like the sidebar pill.
AbstractButton {
    id: root
    property bool current: false
    property int radius: 15
    hoverEnabled: true
    activeFocusOnTab: true
    focusPolicy: Qt.StrongFocus
    implicitHeight: 30
    implicitWidth: label.implicitWidth + 24
    Accessible.name: text
    Accessible.role: Accessible.Button
    scale: down ? 0.96 : 1
    Behavior on scale { NumberAnimation { duration: Motion.instant } }

    background: Rectangle {
        radius: root.radius
        color: root.current ? Theme.accentTint : (root.hovered ? Theme.hoverTint : "transparent")
        border.width: 1
        border.color: root.current ? Qt.rgba(Theme.accent.r, Theme.accent.g, Theme.accent.b, 0.55) : Theme.border
        Behavior on color { ColorAnimation { duration: Motion.fast } }
    }
    contentItem: Item {
        Txt {
            id: label
            anchors.centerIn: parent
            text: root.text
            kind: "caption"
            wrapMode: Text.NoWrap
            font.weight: Font.Medium
            color: root.current ? Theme.text : Theme.textSecondary
        }
    }
    FocusRing {}
}
