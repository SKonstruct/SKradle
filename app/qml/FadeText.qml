import QtQuick
import com.skradle.app

// Text that cross-fades (opacity + 4 px y) whenever its string changes. Opacity and y only.
Item {
    id: root
    property string text
    property string kind: "body"
    property color color: kind === "caption" || kind === "micro" ? Theme.textSecondary : Theme.text
    property int maximumLineCount: 3
    implicitWidth: label.implicitWidth
    implicitHeight: label.implicitHeight

    Txt {
        id: label
        width: root.width
        kind: root.kind
        color: root.color
        maximumLineCount: root.maximumLineCount
        elide: Text.ElideRight
    }
    // The first text (a binding that runs before onCompleted) and any change under reduced motion are shown at once:
    // fading in the title at start-up looked like a flicker, and reduced motion means no movement.
    property bool completed: false
    Component.onCompleted: { label.text = text; completed = true }
    onTextChanged: {
        if (!completed || Motion.reduce) {
            swap.stop()
            label.text = text
            label.opacity = 1
            label.y = 0
        } else if (label.text !== text) {
            swap.restart()
        }
    }

    SequentialAnimation {
        id: swap
        ParallelAnimation {
            NumberAnimation { target: label; property: "opacity"; to: 0; duration: Motion.fast * 0.7; easing.type: Easing.InQuad }
            NumberAnimation { target: label; property: "y"; to: 4; duration: Motion.fast * 0.7; easing.type: Easing.InQuad }
        }
        ScriptAction { script: { label.text = root.text; label.y = -4 } }
        ParallelAnimation {
            NumberAnimation { target: label; property: "opacity"; to: 1; duration: Motion.base; easing.type: Easing.BezierSpline; easing.bezierCurve: Motion.decel }
            NumberAnimation { target: label; property: "y"; to: 0; duration: Motion.base; easing.type: Easing.BezierSpline; easing.bezierCurve: Motion.decel }
        }
    }
}
