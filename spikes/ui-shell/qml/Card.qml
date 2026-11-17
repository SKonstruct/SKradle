import QtQuick

// Surface with a 1 px border, a faint top highlight and two stacked low-alpha shadow layers (no blur shaders).
Item {
    id: root
    property int radius: Theme.rLg
    property color fill: Theme.card
    property color fillEnd: fill
    property bool raised: false          // hover lift
    property bool outlined: true
    default property alias content: surface.data
    property alias surface: surface

    transform: Translate {
        y: root.raised ? -2 : 0
        Behavior on y { NumberAnimation { duration: Motion.fast; easing.type: Easing.OutCubic } }
    }

    Rectangle {
        x: -1; y: 2 + (root.raised ? 2 : 0)
        width: parent.width + 2; height: parent.height + 2
        radius: root.radius + 1
        color: Theme.shadowFar
        Behavior on y { NumberAnimation { duration: Motion.fast } }
    }
    Rectangle {
        x: 0; y: 1
        width: parent.width; height: parent.height + 1
        radius: root.radius
        color: Theme.shadowNear
        opacity: root.raised ? 1 : 0.7
        Behavior on opacity { NumberAnimation { duration: Motion.fast } }
    }
    Rectangle {
        id: surface
        anchors.fill: parent
        radius: root.radius
        gradient: Gradient {
            orientation: Gradient.Horizontal
            GradientStop { position: 0; color: root.fill }
            GradientStop { position: 1; color: root.fillEnd }
        }
        border.width: root.outlined ? 1 : 0
        border.color: Theme.border
        Rectangle {   // top highlight
            x: root.radius * 0.6; y: 1
            width: parent.width - root.radius * 1.2; height: 1
            color: Theme.topLight
        }
    }
}
