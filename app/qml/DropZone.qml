import QtQuick
import com.skradle.app
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Shapes

// Drop affordance: dashed outline, a solid brand-tinted target while a drag hovers. The page owns the DropArea (the
// whole page accepts a drop) and tells this item through `dragging`. Enter or Space opens the file dialog.
Control {
    id: root
    property bool forceActive: false
    property bool dragging: false
    // A bigger, explaining version for an empty mods folder.
    property bool roomy: false
    readonly property bool active: (dragging || forceActive) && enabled
    signal browse()
    implicitHeight: roomy ? 168 : 76
    hoverEnabled: true
    activeFocusOnTab: true
    focusPolicy: Qt.StrongFocus
    property int radius: Theme.rLg
    Accessible.role: Accessible.Button
    Accessible.name: qsTr("Add mod files. Drop files here, or press Enter to choose files.")
    Keys.onReturnPressed: if (enabled) root.browse()
    Keys.onEnterPressed: if (enabled) root.browse()
    Keys.onSpacePressed: if (enabled) root.browse()
    opacity: enabled ? 1 : 0.5

    TapHandler { enabled: root.enabled; onTapped: { root.forceActiveFocus(Qt.MouseFocusReason); root.browse() } }

    background: Item {
        Rectangle {
            anchors.fill: parent
            radius: root.radius
            color: Theme.accentTint
            opacity: root.active ? 1 : (root.hovered && root.enabled ? 0.4 : 0)
            Behavior on opacity { NumberAnimation { duration: Motion.base } }
        }
        Shape {
            anchors.fill: parent
            preferredRendererType: Shape.CurveRenderer
            ShapePath {
                strokeColor: root.active ? Theme.accent : Theme.borderStrong
                strokeWidth: root.active ? 2 : 1.5
                fillColor: "transparent"
                strokeStyle: root.active ? ShapePath.SolidLine : ShapePath.DashLine
                dashPattern: [4, 4]
                Behavior on strokeColor { ColorAnimation { duration: Motion.base } }
                PathRectangle { x: 1; y: 1; width: root.width - 2; height: root.height - 2; radius: root.radius }
            }
        }
    }
    contentItem: Item {
        ColumnLayout {
            visible: root.roomy
            anchors.centerIn: parent
            width: Math.min(parent.width - 2 * Theme.s5, 460)
            spacing: Theme.s2
            Icon { Layout.alignment: Qt.AlignHCenter; name: "upload"; size: 32; color: root.active ? Theme.accent : Theme.textSecondary }
            Txt { Layout.fillWidth: true; horizontalAlignment: Text.AlignHCenter; kind: "heading"; text: root.active ? qsTr("Release to add") : qsTr("No mods yet") }
            Txt { Layout.fillWidth: true; horizontalAlignment: Text.AlignHCenter; color: Theme.textSecondary; text: qsTr("Drop .zip, .jar or .modpack files here, or click to choose them. Each file is checked before it is copied in.") }
        }
        RowLayout {
            visible: !root.roomy
            anchors.centerIn: parent
            width: Math.min(parent.width - 2 * Theme.s5, implicitWidth)
            spacing: Theme.s4
            Icon {
                Layout.alignment: Qt.AlignVCenter
                name: "upload"; size: 26
                color: root.active ? Theme.accent : Theme.textSecondary
                Behavior on color { ColorAnimation { duration: Motion.base } }
            }
            ColumnLayout {
                Layout.fillWidth: true
                Layout.alignment: Qt.AlignVCenter
                spacing: 2
                Txt { Layout.fillWidth: true; text: root.active ? qsTr("Release to add") : qsTr("Drop mod files here"); font.weight: Font.Medium }
                Txt { Layout.fillWidth: true; text: qsTr("Accepts .zip, .jar and .modpack. Or click to choose files."); kind: "caption" }
            }
        }
    }
    FocusRing {}
}
