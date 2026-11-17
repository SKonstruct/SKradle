import QtQuick
import QtQuick.Controls

// Our own tooltip (the stock ToolTip is not natively drawn on macOS and looks wrong in both schemes).
// Appears after a short delay, fades in with a 4 px slide. Opacity and y only.
Popup {
    id: tip
    property string text
    property bool wanted: false
    property string placement: "bottom"   // bottom | right
    property int delay: 450
    padding: 0
    closePolicy: Popup.NoAutoClose
    focus: false
    x: placement === "right" ? parent.width + 10 : Math.round((parent.width - width) / 2)
    y: placement === "right" ? Math.round((parent.height - height) / 2) : parent.height + 8
    width: Math.min(label.implicitWidth + 24, 320)

    onWantedChanged: if (wanted) timer.restart(); else { timer.stop(); close() }
    Timer { id: timer; interval: tip.delay; onTriggered: if (tip.wanted && tip.text.length) tip.open() }

    enter: Transition {
        ParallelAnimation {
            NumberAnimation { property: "opacity"; from: 0; to: 1; duration: Motion.fast; easing.type: Easing.OutCubic }
            NumberAnimation { property: "y"; from: tip.y + 4; to: tip.y; duration: Motion.base; easing.type: Easing.BezierSpline; easing.bezierCurve: Motion.decel }
        }
    }
    exit: Transition { NumberAnimation { property: "opacity"; to: 0; duration: Motion.instant } }

    background: Rectangle {
        radius: Theme.rSm + 2
        color: Theme.dark ? "#2E2E2E" : "#262626"
        border.width: 1
        border.color: Theme.dark ? Qt.rgba(1, 1, 1, 0.14) : Qt.rgba(0, 0, 0, 0.25)
    }
    contentItem: Item {
        implicitWidth: label.implicitWidth + 24
        implicitHeight: label.implicitHeight + 14
        Txt {
            id: label
            x: 12; y: 7
            width: Math.min(implicitWidth, 296)
            text: tip.text
            kind: "caption"
            color: "#F2F2F2"
        }
    }
}
