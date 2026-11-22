import QtQuick
import com.skradle.app
import QtQuick.Controls

// Icon-only "Stop server": a filled danger-coloured square button (white icon on the danger fill, 6.5:1).
AbstractButton {
    id: root
    implicitWidth: Theme.hMd
    implicitHeight: Theme.hMd
    hoverEnabled: true
    focusPolicy: Qt.StrongFocus
    activeFocusOnTab: true
    Accessible.name: qsTr("Stop server")
    Accessible.role: Accessible.Button
    scale: down ? 0.92 : 1
    Behavior on scale { NumberAnimation { duration: Motion.instant; easing.type: Easing.OutQuad } }
    background: Rectangle {
        radius: Theme.rSm
        color: !root.enabled ? Theme.track : root.down ? Theme.dangerFillPressed : root.hovered ? Theme.dangerFillHover : Theme.dangerFill
        Behavior on color { ColorAnimation { duration: Motion.fast } }
    }
    contentItem: Item { Icon { anchors.centerIn: parent; name: "stop"; size: 18; color: "#FFFFFF" } }
    FocusRing {}
    Tip { text: qsTr("Stop server"); wanted: root.hovered }
}
