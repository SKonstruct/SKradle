import QtQuick
import com.skradle.app
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Shapes

// Drop affordance: dashed outline, turns into a solid brand-tinted target while a drag hovers (or when forced).
Control {
    id: root
    property bool forceActive: false
    readonly property bool active: da.containsDrag || forceActive
    signal dropped(int count)
    implicitHeight: 84
    hoverEnabled: true
    activeFocusOnTab: true
    focusPolicy: Qt.StrongFocus
    property int radius: Theme.rLg
    Accessible.role: Accessible.Button
    Accessible.name: L.t("Drop mod files here, or press Enter to browse")
    Keys.onReturnPressed: Data.toast(L.t("Browse for mod files…"))
    Keys.onSpacePressed: Data.toast(L.t("Browse for mod files…"))

    DropArea {
        id: da
        anchors.fill: parent
        onDropped: (drop) => root.dropped(drop.urls.length)
    }
    TapHandler { onTapped: Data.toast(L.t("Browse for mod files…")) }

    background: Item {
        Rectangle {
            id: tint
            anchors.fill: parent
            radius: root.radius
            color: Theme.accentTint
            opacity: root.active ? 1 : (root.hovered ? 0.4 : 0)
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
        RowLayout {
            anchors.centerIn: parent
            width: Math.min(parent.width - 2 * Theme.s5, implicitWidth)
            spacing: Theme.s4
            Item {
                Layout.preferredWidth: 40; Layout.preferredHeight: 40
                Layout.alignment: Qt.AlignVCenter
                Icon {
                    anchors.centerIn: parent
                    anchors.verticalCenterOffset: root.active ? -3 : 0
                    name: "upload"; size: 26
                    color: root.active ? Theme.accent : Theme.textSecondary
                    Behavior on anchors.verticalCenterOffset { NumberAnimation { duration: Motion.base; easing.type: Easing.BezierSpline; easing.bezierCurve: Motion.decel } }
                    Behavior on color { ColorAnimation { duration: Motion.base } }
                }
            }
            ColumnLayout {
                Layout.fillWidth: true
                Layout.alignment: Qt.AlignVCenter
                spacing: 2
                Txt { Layout.fillWidth: true; text: root.active ? L.t("Release to add") : L.t("Drop mod files here"); font.weight: Font.Medium }
                Txt { Layout.fillWidth: true; text: L.t("Accepts .jar and .zip. Or click to browse."); kind: "caption" }
            }
        }
    }
    scale: root.active ? 1.006 : 1
    Behavior on scale { NumberAnimation { duration: Motion.base; easing.type: Easing.BezierSpline; easing.bezierCurve: Motion.decel } }
    FocusRing {}
}
