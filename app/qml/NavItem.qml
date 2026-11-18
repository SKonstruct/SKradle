import QtQuick
import com.skradle.app
import QtQuick.Controls

AbstractButton {
    id: root
    property string glyph
    property bool selected: false
    property bool collapsed: false
    implicitHeight: 42
    hoverEnabled: true
    activeFocusOnTab: true
    focusPolicy: Qt.StrongFocus
    property int radius: Theme.rMd
    Accessible.role: Accessible.PageTab
    Accessible.name: text
    Accessible.selected: selected
    scale: down ? 0.97 : 1
    Behavior on scale { NumberAnimation { duration: Motion.instant; easing.type: Easing.OutQuad } }

    background: Rectangle {
        radius: root.radius
        color: root.down ? Theme.pressTint : Theme.hoverTint
        opacity: root.down ? 1 : (root.hovered && !root.selected ? 1 : 0)
        Behavior on opacity { NumberAnimation { duration: Motion.fast } }
    }
    contentItem: Item {
        Icon {
            x: 14
            anchors.verticalCenter: parent.verticalCenter
            name: root.glyph
            size: 20
            color: root.selected ? Theme.text : (root.hovered ? Theme.text : Theme.textSecondary)
            Behavior on color { ColorAnimation { duration: Motion.fast } }
        }
        Txt {
            x: 14 + 20 + 12
            width: parent.width - x - 8
            anchors.verticalCenter: parent.verticalCenter
            text: root.text
            wrapMode: Text.NoWrap
            elide: Text.ElideRight
            font.weight: root.selected ? Font.DemiBold : Font.Medium
            color: root.selected || root.hovered ? Theme.text : Theme.textSecondary
            opacity: root.collapsed ? 0 : 1
            Behavior on opacity { NumberAnimation { duration: root.collapsed ? Motion.instant : Motion.base } }
            Behavior on color { ColorAnimation { duration: Motion.fast } }
        }
    }
    FocusRing {}
    Tip { text: root.text; wanted: root.collapsed && root.hovered; placement: "right"; delay: 300 }
}
