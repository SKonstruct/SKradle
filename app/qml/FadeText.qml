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
    Component.onCompleted: label.text = text
    onTextChanged: if (label.text !== text) swap.restart()

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
