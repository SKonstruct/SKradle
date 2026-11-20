import QtQuick
import com.skradle.app

// Surface: 1 px hairline, flat fill, elevation from Elevation.qml (dark: none on cards). No gradient, no nested bordered card.
// Children are anchored by the user; `padding` is the margin to use inside (Theme.padCard 20, Theme.padCompact 16);
// Card does not apply it itself.
Item {
    id: root
    property int radius: Theme.rMd
    property color fill: Theme.card
    property int elevation: 1
    property int padding: Theme.padCard
    property bool raised: false          // hover lift: only for cards that are buttons
    property bool outlined: true
    default property alias content: surface.data
    property alias surface: surface

    transform: Translate {
        y: root.raised ? -2 : 0
        Behavior on y { NumberAnimation { duration: Motion.fast; easing.type: Easing.OutCubic } }
    }

    Elevation { level: root.elevation; radius: root.radius }
    Rectangle {
        id: surface
        anchors.fill: parent
        radius: root.radius
        color: root.fill
        border.width: root.outlined ? 1 : 0
        border.color: Theme.border
    }
}
