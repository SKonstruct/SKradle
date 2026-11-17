import QtQuick

// Four-bar ping indicator, drawn from rectangles. level 0..4
Row {
    id: root
    property int level: 4
    property color color: Theme.ok
    spacing: 2
    Accessible.ignored: true
    Repeater {
        model: 4
        Rectangle {
            required property int index
            width: 3; height: 5 + index * 3; radius: 1.5
            anchors.bottom: parent ? parent.bottom : undefined
            color: index < root.level ? root.color : Theme.track
            Behavior on color { ColorAnimation { duration: Motion.base } }
        }
    }
}
