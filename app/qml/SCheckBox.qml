import QtQuick
import com.skradle.app
import QtQuick.Templates as T

T.CheckBox {
    id: control
    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset, implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset, implicitContentHeight + topPadding + bottomPadding, implicitIndicatorHeight + topPadding + bottomPadding)
    padding: 4
    spacing: 10
    hoverEnabled: true
    focusPolicy: Qt.StrongFocus
    Accessible.name: text
    Accessible.checked: checked

    indicator: Rectangle {
        implicitWidth: 20; implicitHeight: 20
        x: control.leftPadding
        y: control.topPadding + (control.availableHeight - height) / 2
        radius: 6
        color: control.checkState !== Qt.Unchecked ? (control.down ? Theme.accentPressed : control.hovered ? Theme.accentHover : Theme.accent)
             : (control.hovered ? Theme.hoverTint : "transparent")
        border.width: control.checkState !== Qt.Unchecked ? 0 : 1.5
        border.color: Theme.fieldBorder
        Behavior on color { ColorAnimation { duration: Motion.fast } }
        Icon { anchors.centerIn: parent; name: "check"; size: 14; stroke: 2.6; color: Theme.accentText
               opacity: control.checkState !== Qt.Unchecked ? 1 : 0
               scale: control.checkState !== Qt.Unchecked ? 1 : 0.6
               Behavior on opacity { NumberAnimation { duration: Motion.fast } }
               Behavior on scale { NumberAnimation { duration: Motion.base; easing.type: Easing.BezierSpline; easing.bezierCurve: Motion.decel } } }
        FocusRing { active: control.visualFocus; grow: 3 }
    }
    contentItem: Text {
        leftPadding: control.indicator.width + control.spacing
        text: control.text
        font.pixelSize: Theme.fsBody
        color: control.enabled ? Theme.text : Theme.textTertiary
        verticalAlignment: Text.AlignVCenter
        wrapMode: Text.WordWrap
    }
}
