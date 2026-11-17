import QtQuick

// Small transient message, bottom centre. Opacity + y only.
Item {
    id: root
    property string message
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
        width: label.implicitWidth + 32
        height: label.implicitHeight + 20
        radius: height / 2
        color: Theme.dark ? "#2F2F2F" : "#222222"
        border.width: 1
        border.color: Theme.dark ? Qt.rgba(1, 1, 1, 0.12) : Qt.rgba(0, 0, 0, 0.2)
        Txt {
            id: label
            anchors.centerIn: parent
            text: root.message
            kind: "caption"
            color: "#F2F2F2"
            wrapMode: Text.NoWrap
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
