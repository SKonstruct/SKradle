import QtQuick
import com.skradle.app
import QtQuick.Controls

// Our own tooltip (the stock ToolTip is not natively drawn on macOS and looks wrong in both schemes).
// Appears after a short delay, fades in with a 4 px slide. Opacity and y only.
Popup {
    id: tip
    property string text
    property bool wanted: false
    // The 4 px slide-in is a transform, so the popup's own `y` keeps its binding.
    property real slide: 0
    property string placement: "bottom"   // bottom | right
    property int delay: 450
    padding: 0
    closePolicy: Popup.NoAutoClose
    focus: false
    x: placement === "right" ? parent.width + Theme.s3 : Math.round((parent.width - width) / 2)
    y: placement === "right" ? Math.round((parent.height - height) / 2) : parent.height + Theme.s2
    width: Math.min(label.implicitWidth + 2 * Theme.s3, 320)

    onWantedChanged: if (wanted) timer.restart(); else { timer.stop(); close() }
    Timer { id: timer; interval: tip.delay; onTriggered: if (tip.wanted && tip.text.length) tip.open() }

    enter: Transition {
        ParallelAnimation {
            NumberAnimation { property: "opacity"; from: 0; to: 1; duration: Motion.fast; easing.type: Easing.OutCubic }
            NumberAnimation { target: tip; property: "slide"; from: 4; to: 0; duration: Motion.base; easing.type: Easing.BezierSpline; easing.bezierCurve: Motion.decel }
        }
    }
    exit: Transition { NumberAnimation { property: "opacity"; to: 0; duration: Motion.instant } }

    background: Rectangle {
        transform: Translate { y: tip.slide }
        radius: Theme.rSm
        color: Theme.tipBg
        border.width: 1
        border.color: Theme.tipBorder
    }
    contentItem: Item {
        transform: Translate { y: tip.slide }
        implicitWidth: label.implicitWidth + 2 * Theme.s3
        implicitHeight: label.implicitHeight + Theme.s3
        Txt {
            id: label
            x: Theme.s3; y: Theme.s3 / 2
            width: Math.min(implicitWidth, 296)
            text: tip.text
            kind: "caption"
            color: Theme.tipText
        }
    }
}
