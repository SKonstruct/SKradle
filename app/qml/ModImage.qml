import QtQuick
import QtQuick.Effects
import com.skradle.app

// A mod's picture as a square tile that fills the item, or a quiet neutral tile when it has none (or the file cannot
// be shown). `source` is a file URL from the core's checked cache, or "". A mod with a warning gets the amber tile.
// Only the left corners are rounded: the tile is drawn wider by one radius and clipped, so the right edge is straight.
// The owner sizes it (no padding of its own) and sets `radius` to the card's inner radius.
Item {
    id: root
    property url source
    property bool warned: false
    property int radius: Theme.rSm
    implicitWidth: Theme.hRow
    implicitHeight: Theme.hRow
    clip: true

    readonly property bool shown: pic.status === Image.Ready
    // Rounded corners need a shader; the software renderer has none, so the picture is drawn square there.
    readonly property bool canMask: GraphicsInfo.api !== GraphicsInfo.Software

    Item {
        id: tile
        width: root.width + root.radius
        height: root.height

        Rectangle {   // the placeholder, and the backdrop of a picture
            anchors.fill: parent
            radius: root.radius
            color: root.warned ? Theme.warnTint : Theme.raised
        }
        Icon {
            visible: !root.shown
            x: (root.width - width) / 2
            y: (root.height - height) / 2
            name: "mods"
            size: 24
            color: root.warned ? Theme.amberText : Theme.textTertiary
        }
        Image {
            id: pic
            anchors.fill: parent
            source: root.source
            visible: root.shown
            opacity: root.canMask ? 0 : 1   // masked: the effect below draws it
            asynchronous: true
            cache: true
            smooth: true
            mipmap: true
            fillMode: Image.PreserveAspectCrop
            sourceSize: Qt.size(root.width * 2, root.height * 2)
        }
        Item {
            id: mask
            anchors.fill: parent
            visible: false
            layer.enabled: root.canMask
            Rectangle { anchors.fill: parent; radius: root.radius; color: "black" }
        }
        Loader {   // the shader is only created where the renderer has shaders
            anchors.fill: parent
            active: root.canMask && root.shown
            sourceComponent: MultiEffect {
                source: pic
                maskEnabled: true
                maskSource: mask
            }
        }
    }
}
