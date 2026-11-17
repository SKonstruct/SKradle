import QtQuick
import QtQuick.Shapes

// Determinate arc (value 0..1) or indeterminate spinner. The spinner is a fixed arc turned by a RotationAnimator,
// so it costs nothing per frame on the GUI thread.
Item {
    id: root
    property real value: 0
    property bool indeterminate: false
    property real thickness: 8
    property color color: Theme.accent
    property color trackColor: Theme.track
    property bool active: true
    default property alias center: centerItem.data
    implicitWidth: 96
    implicitHeight: 96
    Accessible.role: Accessible.ProgressBar
    Accessible.name: indeterminate ? "Working" : Math.round(value * 100) + " percent"

    readonly property real r: (width - thickness) / 2

    Shape {   // track
        anchors.fill: parent
        preferredRendererType: Shape.CurveRenderer
        ShapePath {
            strokeColor: root.trackColor
            strokeWidth: root.thickness
            fillColor: "transparent"
            PathAngleArc { centerX: root.width / 2; centerY: root.height / 2; radiusX: root.r; radiusY: root.r; startAngle: 0; sweepAngle: 360 }
        }
    }
    Shape {   // determinate
        anchors.fill: parent
        opacity: root.indeterminate || root.value < 0.004 ? 0 : 1
        Behavior on opacity { NumberAnimation { duration: Motion.base } }
        ShapePath {
            strokeColor: root.color
            strokeWidth: root.thickness
            fillColor: "transparent"
            capStyle: ShapePath.RoundCap
            PathAngleArc { centerX: root.width / 2; centerY: root.height / 2; radiusX: root.r; radiusY: root.r; startAngle: -90; sweepAngle: Math.min(359.9, root.value * 360) }
        }
    }
    Shape {   // indeterminate
        id: spinner
        anchors.fill: parent
        opacity: root.indeterminate ? 1 : 0
        visible: opacity > 0.01
        Behavior on opacity { NumberAnimation { duration: Motion.base } }
        preferredRendererType: Shape.CurveRenderer
        ShapePath {
            strokeColor: root.color
            strokeWidth: root.thickness
            fillColor: "transparent"
            capStyle: ShapePath.RoundCap
            PathAngleArc { centerX: root.width / 2; centerY: root.height / 2; radiusX: root.r; radiusY: root.r; startAngle: -90; sweepAngle: 100 }
        }
        RotationAnimator on rotation {
            from: 0; to: 360
            duration: Motion.spin
            loops: Animation.Infinite
            running: root.indeterminate && root.active && spinner.visible
        }
    }
    Item { id: centerItem; anchors.fill: parent }
}
