import QtQuick
import com.skradle.app
import QtQuick.Controls

AbstractButton {
    id: root
    property string glyph
    property string tip
    property real iconSize: 18
    property bool toggledOn: false
    implicitWidth: 34
    implicitHeight: 34
    hoverEnabled: true
    activeFocusOnTab: true
    focusPolicy: Qt.StrongFocus
    property int radius: Theme.rSm + 2
    Accessible.name: tip
    Accessible.role: Accessible.Button
    scale: down ? 0.92 : 1
    Behavior on scale { NumberAnimation { duration: Motion.instant; easing.type: Easing.OutQuad } }

    background: Rectangle {
        radius: root.radius
        color: root.down ? Theme.pressTint : Theme.hoverTint
        opacity: root.down || root.hovered || root.toggledOn ? 1 : 0
        Behavior on opacity { NumberAnimation { duration: Motion.fast } }
    }
    contentItem: Item {
        Icon {
            anchors.centerIn: parent
            name: root.glyph
            size: root.iconSize
            color: root.hovered || root.down ? Theme.text : Theme.textSecondary
        }
    }
    FocusRing {}
    Tip { text: root.tip; wanted: root.hovered && root.tip.length > 0 }
}
