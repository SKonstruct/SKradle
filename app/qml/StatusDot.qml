import QtQuick
import com.skradle.app

// Status dot; online dots emit a soft pulse (scale + opacity on the render thread).
Item {
    id: root
    property color color: Theme.ok
    property bool pulse: false
    width: 16
    height: 16
    readonly property bool animating: pulse && !Motion.reduce && visible

    Rectangle {
        id: ring
        anchors.centerIn: parent
        width: 9; height: 9; radius: 5
        color: root.color
        opacity: 0
        visible: root.pulse
        ParallelAnimation {
            running: root.animating
            loops: Animation.Infinite
            ScaleAnimator { target: ring; from: 1; to: 2.3; duration: Motion.pulse; easing.type: Easing.OutCubic }
            OpacityAnimator { target: ring; from: 0.45; to: 0; duration: Motion.pulse; easing.type: Easing.OutCubic }
        }
    }
    Rectangle {
        anchors.centerIn: parent
        width: 9; height: 9; radius: 5
        color: root.color
    }
}
