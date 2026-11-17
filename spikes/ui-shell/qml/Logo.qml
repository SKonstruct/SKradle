import QtQuick
import QtQuick.Shapes

// Logo mark: brand-pink tile with a mint spiral (nod to the game). Drawn here, no external asset.
Item {
    id: root
    property real size: 40
    width: size
    height: size

    Rectangle {
        anchors.fill: parent
        radius: root.size * 0.27
        color: Theme.accent
        border.width: Math.max(1, root.size / 28)
        border.color: Theme.dark ? Qt.rgba(1, 1, 1, 0.16) : Qt.rgba(0, 0, 0, 0.12)
    }
    Shape {
        width: 24
        height: 24
        scale: root.size / 24
        transformOrigin: Item.TopLeft
        preferredRendererType: Shape.CurveRenderer
        ShapePath {
            strokeColor: Theme.mint
            strokeWidth: 1.9
            fillColor: "transparent"
            capStyle: ShapePath.RoundCap
            joinStyle: ShapePath.RoundJoin
            // semicircle spiral, shifted so its visual centre sits in the tile centre
            PathSvg { path: "M12.2 9.6 a1.5 1.5 0 0 1 3 0 a3 3 0 0 1 -6 0 a4.5 4.5 0 0 1 9 0 a6 6 0 0 1 -12 0" }
        }
    }
}
