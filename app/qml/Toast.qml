import QtQuick
import com.skradle.app

// Transient message, bottom centre. kind: info | success | error. Opacity and y only.
// Stays 4 s (info, success) or 8 s (error, +1 s per 40 characters over 80); stays while the pointer is over it or the
// close button has focus; errors have a close button. Show it with Data.toast(text, kind).
Item {
    id: root
    property string message
    property string kind: "info"
    // The toast never grows past the window or 480 px: long messages wrap.
    readonly property real maxWidth: parent ? Math.max(160, Math.min(480, parent.width - 2 * Theme.s6)) : 480
    readonly property bool held: hover.hovered || closeBtn.activeFocus
    width: pill.width
    height: pill.height
    opacity: 0
    visible: opacity > 0.01
    Accessible.role: Accessible.AlertMessage
    Accessible.name: message

    function lifetime(text, k) {
        if (k === "error") return Motion.toastError + Math.max(0, Math.ceil((text.length - 80) / 40)) * 1000
        return Motion.toastInfo + Math.max(0, Math.ceil((text.length - 80) / 40)) * 500
    }
    function show(text, k) {
        message = text
        kind = k === "error" || k === "success" ? k : "info"
        fadeOut.stop()
        anim.restart()
        hide.interval = lifetime(text, kind)
        hide.restart()
    }
    function dismiss() { hide.stop(); fadeOut.restart() }
    onHeldChanged: {
        if (held) { hide.stop(); fadeOut.stop(); opacity = 1 }
        else if (opacity > 0.01) hide.restart()
    }

    Rectangle {
        id: pill
        width: Math.min(row.implicitWidth + 2 * Theme.s4, root.maxWidth)
        height: row.implicitHeight + 2 * Theme.s3
        radius: Theme.rMd
        color: Theme.tipBg
        border.width: 1
        border.color: Theme.tipBorder
        HoverHandler { id: hover }
        Row {
            id: row
            x: Theme.s4
            anchors.verticalCenter: parent.verticalCenter
            spacing: Theme.s3
            Icon {
                anchors.verticalCenter: parent.verticalCenter
                name: root.kind === "error" ? "error" : root.kind === "success" ? "check" : "info"
                size: 16
                color: root.kind === "error" ? Theme.danger : root.kind === "success" ? Theme.ok : Theme.tipText
            }
            Txt {
                id: label
                anchors.verticalCenter: parent.verticalCenter
                width: Math.min(implicitWidth, root.maxWidth - 2 * Theme.s4 - 16 - Theme.s3 - (closeBtn.visible ? closeBtn.width + Theme.s3 : 0))
                text: root.message
                kind: "caption"
                color: Theme.tipText
                wrapMode: Text.Wrap
            }
            IconButton {
                id: closeBtn
                anchors.verticalCenter: parent.verticalCenter
                visible: root.kind === "error"
                size: "sm"; glyph: "close"; tip: qsTr("Dismiss"); onInverse: true
                onClicked: root.dismiss()
            }
        }
    }
    transform: Translate { id: tr; y: 0 }
    ParallelAnimation {
        id: anim
        NumberAnimation { target: root; property: "opacity"; to: 1; duration: Motion.base; easing.type: Easing.BezierSpline; easing.bezierCurve: Motion.decel }
        NumberAnimation { target: tr; property: "y"; from: Theme.s2; to: 0; duration: Motion.base; easing.type: Easing.BezierSpline; easing.bezierCurve: Motion.decel }
    }
    Timer {
        id: hide
        onTriggered: fadeOut.restart()
    }
    NumberAnimation { id: fadeOut; target: root; property: "opacity"; to: 0; duration: Motion.base }
}
