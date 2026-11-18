import QtQuick
import com.skradle.app

// Small transient message, bottom centre. Opacity + y only.
Item {
    id: root
    property string message
    // The pill never grows past the window: long messages wrap.
    readonly property real maxWidth: parent ? Math.max(160, parent.width - 48) : 520
    width: pill.width
    height: pill.height
    opacity: 0
    visible: opacity > 0.01
    Accessible.role: Accessible.AlertMessage
    Accessible.name: message

    function show(text) {
        message = text
        anim.restart()
        hide.restart()
    }

    Rectangle {
        id: pill
        width: label.width + 32
        height: label.implicitHeight + 20
        radius: height / 2
        color: Theme.dark ? "#2F2F2F" : "#222222"
        border.width: 1
        border.color: Theme.dark ? Qt.rgba(1, 1, 1, 0.12) : Qt.rgba(0, 0, 0, 0.2)
        Txt {
            id: label
            anchors.centerIn: parent
            width: Math.min(implicitWidth, root.maxWidth - 32)
            text: root.message
            kind: "caption"
            color: "#F2F2F2"
            wrapMode: Text.Wrap
            horizontalAlignment: Text.AlignHCenter
        }
    }
    transform: Translate { id: tr; y: 0 }
    ParallelAnimation {
        id: anim
        NumberAnimation { target: root; property: "opacity"; to: 1; duration: Motion.base; easing.type: Easing.BezierSpline; easing.bezierCurve: Motion.decel }
        NumberAnimation { target: tr; property: "y"; from: 10; to: 0; duration: Motion.base; easing.type: Easing.BezierSpline; easing.bezierCurve: Motion.decel }
    }
    Timer {
        id: hide
        interval: 2200
        onTriggered: fadeOut.restart()
    }
    NumberAnimation { id: fadeOut; target: root; property: "opacity"; to: 0; duration: Motion.base }
}
