import QtQuick
import com.skradle.app
import QtQuick.Templates as T

T.Switch {
    id: control
    implicitWidth: implicitIndicatorWidth + leftPadding + rightPadding + (text.length > 0 ? implicitContentWidth + spacing : 0)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset, implicitIndicatorHeight + topPadding + bottomPadding)
    padding: 0
    spacing: Theme.gapInline
    hoverEnabled: true
    focusPolicy: Qt.StrongFocus
    Accessible.name: text
    Accessible.checked: checked

    indicator: Item {
        implicitWidth: 40
        implicitHeight: 24
        x: control.leftPadding
        y: control.topPadding + (control.availableHeight - height) / 2
        Rectangle {
            id: track
            anchors.fill: parent
            radius: Theme.rPill
            color: !control.enabled ? Theme.track : control.checked ? (control.down ? Theme.accentPressed : control.hovered ? Theme.accentHover : Theme.accent)
                 : (control.hovered ? Theme.switchOff : Theme.switchOff)
            Behavior on color { ColorAnimation { duration: Motion.base } }
        }
        Rectangle {
            id: knob
            width: 18; height: 18; radius: Theme.rPill
            y: 3
            x: control.checked ? parent.width - width - 3 : 3
            color: Theme.knob
            scale: control.down ? 1.12 : 1
            opacity: control.enabled ? 1 : 0.7
            Behavior on x { NumberAnimation { duration: Motion.base; easing.type: Easing.BezierSpline; easing.bezierCurve: Motion.standard } }
            Behavior on scale { NumberAnimation { duration: Motion.instant } }
            // tiny check mark makes on/off readable without colour
            Icon { anchors.centerIn: parent; name: "check"; size: 12; stroke: 2.6; color: Theme.accent; opacity: control.checked ? 1 : 0
                   Behavior on opacity { NumberAnimation { duration: Motion.base } } }
        }
        FocusRing { active: control.visualFocus; grow: 3 }
    }
    contentItem: Text {
        leftPadding: control.indicator.width + control.spacing
        text: control.text
        font.pixelSize: Theme.fsBody
        color: control.enabled ? Theme.text : Theme.textTertiary
        verticalAlignment: Text.AlignVCenter
        visible: control.text.length > 0
    }
}
