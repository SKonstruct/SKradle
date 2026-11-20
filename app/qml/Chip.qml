import QtQuick
import com.skradle.app
import QtQuick.Controls

// Filter chip, 28 high, radius rSm. Selected = brand tint + a leading check + semibold, so it never relies on tint alone.
// For a single choice among 2-5 options use Segmented instead.
AbstractButton {
    id: root
    property bool selected: false
    property string badge
    hoverEnabled: true
    activeFocusOnTab: true
    focusPolicy: Qt.StrongFocus
    checkable: false
    implicitHeight: Theme.hSm
    implicitWidth: row.implicitWidth + 2 * Theme.s3
    property int radius: Theme.rSm
    Accessible.name: badge.length > 0 ? text + ", " + badge : text
    Accessible.role: Accessible.RadioButton
    Accessible.checked: selected
    scale: down ? 0.96 : 1
    Behavior on scale { NumberAnimation { duration: Motion.instant } }

    background: Rectangle {
        radius: root.radius
        color: root.selected ? Theme.stateSelected : (root.hovered ? Theme.stateHover : "transparent")
        border.width: 1
        border.color: root.selected ? Theme.accentFg : Theme.border
        Behavior on color { ColorAnimation { duration: Motion.fast } }
        Behavior on border.color { ColorAnimation { duration: Motion.fast } }
    }
    contentItem: Item { Row {
        id: row
        spacing: Theme.s1
        anchors.centerIn: parent
        Icon {
            visible: root.selected
            name: "check"; size: 14; stroke: 2.4; color: Theme.accentFg
            anchors.verticalCenter: parent.verticalCenter
        }
        Txt {
            text: root.text
            kind: "caption"
            wrapMode: Text.NoWrap
            font.weight: root.selected ? Font.DemiBold : Font.Medium
            color: root.selected ? Theme.text : Theme.textSecondary
            anchors.verticalCenter: parent.verticalCenter
        }
        Txt {
            visible: root.badge.length > 0
            text: root.badge
            kind: "micro"
            numeric: true
            wrapMode: Text.NoWrap
            color: Theme.textSecondary
            anchors.verticalCenter: parent.verticalCenter
        }
    } }
    FocusRing {}
}
