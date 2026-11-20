import QtQuick
import com.skradle.app

// Soft shadow under a rounded surface, drawn with a few stacked low-alpha rectangles (no blur shader).
// level 0 flat | 1 card (light only: one faint line of shadow, dark relies on the surface step and hairline)
//       2 menu and popover | 3 dialog. Put it behind the surface and give it the same radius.
Item {
    id: root
    property int level: 1
    property int radius: Theme.rMd
    anchors.fill: parent

    // [spread, y offset, alpha] per layer
    readonly property var layers: level >= 3 ? [[14, 14, 0.35], [8, 10, 0.5], [3, 4, 1]]
                                : level === 2 ? [[8, 6, 0.4], [3, 3, 0.8]]
                                : level === 1 && !Theme.dark ? [[0, 1, 1]] : []
    Repeater {
        model: root.layers
        Rectangle {
            required property var modelData
            x: -modelData[0]; y: -modelData[0] + modelData[1]
            width: root.width + 2 * modelData[0]
            height: root.height + 2 * modelData[0]
            radius: root.radius + modelData[0]
            color: root.level === 1 ? Theme.shadowCard : modelData[2] > 0.9 ? Theme.shadowNear : Theme.shadowFar
            opacity: root.level === 1 ? 1 : modelData[2]
        }
    }
}
