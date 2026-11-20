import QtQuick
import com.skradle.app
import QtQuick.Templates as T

T.Slider {
    id: control
    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset, implicitHandleWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset, implicitHandleHeight + topPadding + bottomPadding)
    padding: Theme.s2
    hoverEnabled: true
    focusPolicy: Qt.StrongFocus
    snapMode: T.Slider.NoSnap

    background: Item {
        x: control.leftPadding
        y: control.topPadding + control.availableHeight / 2 - height / 2
        implicitWidth: 200
        implicitHeight: 28
        width: control.availableWidth
        height: implicitHeight
        Rectangle {   // rail: high enough contrast to stay visible on both surfaces
            x: 0; y: (parent.height - height) / 2
            width: parent.width; height: 6; radius: Theme.rXs
            color: control.enabled ? Theme.sliderRail : Theme.track
            Rectangle {
                width: control.visualPosition * parent.width; height: parent.height; radius: Theme.rXs
                color: control.enabled ? Theme.accent : Theme.textTertiary
            }
        }
    }
    handle: Item {
        x: control.leftPadding + control.visualPosition * (control.availableWidth - width)
        y: control.topPadding + control.availableHeight / 2 - height / 2
        implicitWidth: 22
        implicitHeight: 22
        Rectangle {   // press halo
            anchors.centerIn: parent
            width: 40; height: 40; radius: Theme.rPill
            color: Theme.accentTint
            opacity: control.pressed ? 1 : (control.hovered ? 0.55 : 0)
            scale: control.pressed ? 1 : 0.7
            Behavior on opacity { NumberAnimation { duration: Motion.fast } }
            Behavior on scale { NumberAnimation { duration: Motion.fast } }
        }
        Rectangle {
            anchors.fill: parent
            radius: Theme.rPill
            color: Theme.knob
            border.width: 2
            border.color: control.enabled ? Theme.accent : Theme.textTertiary
        }
        FocusRing { active: control.visualFocus; grow: 4 }
    }
}
