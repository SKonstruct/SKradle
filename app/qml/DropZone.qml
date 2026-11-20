import QtQuick
import com.skradle.app
import QtQuick.Layouts
import QtQuick.Shapes

// The drop affordance: nothing on the page until a file is dragged over it, then a dashed target covers the list.
// The page owns the DropArea (the whole page accepts a drop) and tells this item through `dragging`; adding by
// keyboard goes through the "Add mods" button, so this item takes no input.
Item {
    id: root
    property bool forceActive: false     // screenshots
    property bool dragging: false
    readonly property bool active: (dragging || forceActive) && enabled
    visible: opacity > 0
    opacity: active ? 1 : 0
    Behavior on opacity { NumberAnimation { duration: Motion.base } }
    Accessible.ignored: true

    Rectangle { anchors.fill: parent; radius: Theme.rLg; color: Theme.bg; opacity: 0.9 }
    Rectangle { anchors.fill: parent; radius: Theme.rLg; color: Theme.accentTint }
    Shape {
        anchors.fill: parent
        preferredRendererType: Shape.CurveRenderer
        ShapePath {
            strokeColor: Theme.accent
            strokeWidth: 2
            fillColor: "transparent"
            strokeStyle: ShapePath.DashLine
            dashPattern: [4, 4]
            PathRectangle { x: 1; y: 1; width: root.width - 2; height: root.height - 2; radius: Theme.rLg }
        }
    }
    ColumnLayout {
        anchors.centerIn: parent
        width: Math.min(parent.width - 2 * Theme.s6, Theme.readMeasure)
        spacing: Theme.s2
        Icon { Layout.alignment: Qt.AlignHCenter; name: "upload"; size: 28; color: Theme.accentFg }
        Txt { Layout.fillWidth: true; horizontalAlignment: Text.AlignHCenter; kind: "heading"; text: qsTr("Release to add mods") }
        Txt { Layout.fillWidth: true; horizontalAlignment: Text.AlignHCenter; text: qsTr("Accepts .zip, .jar and .modpack files. Each one is checked before it is copied in.") }
    }
}
