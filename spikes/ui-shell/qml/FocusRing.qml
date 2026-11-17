import QtQuick

// Keyboard focus visual for custom items. Parent must expose visualFocus (any Control does).
Rectangle {
    property bool active: !!(parent && parent.visualFocus)
    property int grow: 3
    anchors.fill: parent
    anchors.margins: -grow
    radius: (parent && parent.radius !== undefined ? parent.radius : Theme.rMd) + grow
    color: "transparent"
    border.width: 2
    border.color: Theme.accentFocus
    opacity: active ? 1 : 0
    visible: opacity > 0.01
    Behavior on opacity { NumberAnimation { duration: Motion.fast } }
}
