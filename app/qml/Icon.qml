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

    Shape {
        width: 24
        height: 24
        scale: root.size / 24
        transformOrigin: Item.TopLeft
        preferredRendererType: Shape.CurveRenderer
        ShapePath {
            strokeColor: root.color
            strokeWidth: root.filled ? 1.6 : root.stroke
            fillColor: root.filled ? root.color : "transparent"
            capStyle: ShapePath.RoundCap
            joinStyle: ShapePath.RoundJoin
            PathSvg { path: Glyphs.paths[root.name] || "" }
        }
    }
}
