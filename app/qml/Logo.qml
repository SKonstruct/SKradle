import QtQuick
import com.skradle.app

// App mark: the shared glyph on the brand-pink tile, proportioned like assets/brand/icon-1024.png
// (glyph = 58% of tile height, centred). Pure vector, crisp at any scale.
Item {
    id: root
    property real size: 40
    property bool tile: true
    width: size
    height: size
    Accessible.role: Accessible.Graphic
    Accessible.name: "SKradle"

    Rectangle {
        visible: root.tile
        anchors.fill: parent
        radius: root.size * 0.225
        color: Theme.accent
        border.width: Math.max(1, root.size / 40)
        border.color: Theme.dark ? Qt.rgba(1, 1, 1, 0.14) : Qt.rgba(0, 0, 0, 0.10)
    }
    Glyph {
        anchors.centerIn: parent
        glyphHeight: root.tile ? root.size * 0.66 : root.size
    }
}
