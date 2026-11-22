import QtQuick
import QtQuick.Layouts
import QtQuick.Effects
import com.skradle.app

// An event's banner: a picture from the core's checked cache (PNG or JPEG, size capped) with rounded corners, shown
// whole in its own aspect ratio. It
// takes no space until the picture is ready, and none when `source` is "" or the file cannot be shown.
Item {
    id: root
    property url source
    property int radius: Theme.rMd
    readonly property bool shown: pic.status === Image.Ready
    // Rounded corners need a shader; the software renderer has none, so the picture is drawn square there.
    readonly property bool canMask: GraphicsInfo.api !== GraphicsInfo.Software

    visible: shown
    // The height follows the picture's own proportions (the feed's banners are 800x550, not a wide strip), so
    // nothing is cropped or stretched. `maxHeight` caps it for a narrow, tall picture.
    property int maxHeight: 0
    readonly property real aspect: shown && pic.implicitHeight > 0 ? pic.implicitWidth / pic.implicitHeight : 1
    implicitHeight: shown ? Math.round(maxHeight > 0 ? Math.min(width / aspect, maxHeight) : width / aspect) : 0
    Layout.preferredHeight: implicitHeight

    Image {
        id: pic
        anchors.fill: parent
        source: root.source
        opacity: root.canMask ? 0 : 1   // masked: the effect below draws it
        asynchronous: true
        cache: true
        smooth: true
        mipmap: true
        fillMode: Image.PreserveAspectFit
    }
    Item {
        id: mask
        anchors.fill: parent
        visible: false
        layer.enabled: root.canMask
        Rectangle { anchors.fill: parent; radius: root.radius; color: "black" }
    }
    Loader {
        anchors.fill: parent
        active: root.canMask && root.shown
        sourceComponent: MultiEffect {
            source: pic
            maskEnabled: true
            maskSource: mask
        }
    }
    Rectangle {   // a hairline so a dark picture still has an edge
        anchors.fill: parent
        visible: root.shown
        radius: root.radius
        color: "transparent"
        border.width: 1
        border.color: Theme.border
    }
}
