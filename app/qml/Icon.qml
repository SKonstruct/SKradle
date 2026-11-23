import QtQuick
import com.skradle.app
import QtQuick.Shapes

Item {
    id: root
    property string name
    property color color: Theme.text
    property real size: 20
    property real stroke: 1.8
    width: size
    height: size
    readonly property bool filled: Glyphs.filled.indexOf(name) >= 0
    readonly property bool solid: Glyphs.solid.indexOf(name) >= 0
    readonly property real box: Glyphs.boxes[name] || 24

    Shape {
        width: root.box
        height: root.box
        scale: root.size / root.box
        transformOrigin: Item.TopLeft
        preferredRendererType: Shape.CurveRenderer
        ShapePath {
            strokeColor: root.color
            strokeWidth: root.solid ? 0 : root.filled ? 1.6 : root.stroke
            fillColor: root.filled ? root.color : "transparent"
            capStyle: ShapePath.RoundCap
            joinStyle: ShapePath.RoundJoin
            PathSvg { path: Glyphs.paths[root.name] || "" }
        }
    }
}
