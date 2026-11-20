import QtQuick
import com.skradle.app
import QtQuick.Controls

// Square icon-only button. size: sm (28) | md (36, default). Always set `tip` (tooltip and accessible name).
AbstractButton {
    id: root
    property string glyph
    property string tip
    property string size: "md"
    property real iconSize: 18
    property bool toggledOn: false
    property bool onInverse: false      // drawn on a dark chip (toast, tooltip) in either scheme
    implicitWidth: size === "sm" ? Theme.hSm : Theme.hMd
    implicitHeight: implicitWidth
    hoverEnabled: true
    activeFocusOnTab: true
    focusPolicy: Qt.StrongFocus
    property int radius: Theme.rSm
    Accessible.name: tip
    Accessible.role: Accessible.Button
    scale: down ? 0.92 : 1
    Behavior on scale { NumberAnimation { duration: Motion.instant; easing.type: Easing.OutQuad } }

    background: Rectangle {
        radius: root.radius
        color: root.onInverse ? Qt.rgba(1, 1, 1, root.down ? 0.2 : 0.12) : root.down ? Theme.statePress : Theme.stateHover
        opacity: root.down || root.hovered || root.toggledOn ? 1 : 0
        Behavior on opacity { NumberAnimation { duration: Motion.fast } }
    }
    contentItem: Item {
        Icon {
            anchors.centerIn: parent
            name: root.glyph
            size: root.iconSize
            color: root.onInverse ? Theme.tipText : !root.enabled ? Theme.textTertiary : root.hovered || root.down || root.toggledOn ? Theme.text : Theme.textSecondary
        }
    }
    FocusRing {}
    Tip { text: root.tip; wanted: root.hovered && root.tip.length > 0 }
}
