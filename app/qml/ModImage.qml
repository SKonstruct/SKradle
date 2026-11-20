import QtQuick
import QtQuick.Effects
import com.skradle.app

// A mod's picture at a fixed size with rounded corners, or the generic mod glyph when it has none (or the file
// cannot be shown). `source` is a file URL from the core's checked cache, or "".
Item {
    id: root
    property url source
    property bool warned: false
    property int radius: 12
    implicitWidth: 48
    implicitHeight: 48
    width: implicitWidth
    height: implicitHeight

    readonly property bool shown: pic.status === Image.Ready
    // Rounded corners need a shader; the software renderer has none, so the picture is drawn square there.
    readonly property bool canMask: GraphicsInfo.api !== GraphicsInfo.Software

    Rectangle {   // the placeholder, and the frame behind a picture
        anchors.fill: parent
        radius: root.radius
        color: root.warned ? Theme.amberTint : Theme.accentTint
        border.width: 1
        border.color: Theme.border
        Icon {
            visible: !root.shown
            anchors.centerIn: parent
            name: "mods"
            size: 24
            color: root.warned ? Theme.amberText : Theme.accentFg
        }
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
        sourceSize: Qt.size(width * 2, height * 2)
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
    Rectangle {   // a hairline over the picture so a dark picture still has an edge
        anchors.fill: parent
        visible: root.shown
        radius: root.radius
        color: "transparent"
        border.width: 1
        border.color: Theme.border
    }
}
