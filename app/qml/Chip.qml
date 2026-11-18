import QtQuick
import com.skradle.app
import QtQuick.Controls

// Filter chip. Selected = brand tint (same language as the sidebar pill).
AbstractButton {
    id: root
    property bool selected: false
    property string badge
    hoverEnabled: true
    activeFocusOnTab: true
    focusPolicy: Qt.StrongFocus
    checkable: false
    implicitHeight: 30
    implicitWidth: row.implicitWidth + 24
    property int radius: 15
    Accessible.name: text
    Accessible.role: Accessible.RadioButton
    Accessible.checked: selected
    scale: down ? 0.96 : 1
    Behavior on scale { NumberAnimation { duration: Motion.instant } }

    background: Rectangle {
        radius: root.radius
        color: root.selected ? Theme.accentTint : (root.hovered ? Theme.hoverTint : "transparent")
        border.width: 1
        border.color: root.selected ? Qt.rgba(Theme.accent.r, Theme.accent.g, Theme.accent.b, 0.55) : Theme.border
        Behavior on color { ColorAnimation { duration: Motion.fast } }
        Behavior on border.color { ColorAnimation { duration: Motion.fast } }
    }
    contentItem: Item { Row {
        id: row
        spacing: 6
        anchors.centerIn: parent
        Txt {
            text: root.text
            kind: "caption"
            wrapMode: Text.NoWrap
            font.weight: Font.Medium
            color: root.selected ? Theme.text : Theme.textSecondary
            anchors.verticalCenter: parent.verticalCenter
        }
        Txt {
            visible: root.badge.length > 0
            text: root.badge
            kind: "micro"
            wrapMode: Text.NoWrap
            color: Theme.textTertiary
            anchors.verticalCenter: parent.verticalCenter
        }
    } }
    FocusRing {}
}
