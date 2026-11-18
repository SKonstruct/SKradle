import QtQuick
import com.skradle.app
import QtQuick.Templates as T

// Thin overlay scroll bar: 6 px pill, widens to 10 px on hover, fades out when idle.
T.ScrollBar {
    id: control
    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset, implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset, implicitContentHeight + topPadding + bottomPadding)
    padding: 3
    visible: control.policy !== T.ScrollBar.AlwaysOff
    minimumSize: 0.08
    hoverEnabled: true
    interactive: true

    contentItem: Rectangle {
        implicitWidth: control.interactive ? 10 : 6
        implicitHeight: control.interactive ? 10 : 6
        radius: width / 2
        color: Theme.scrollThumb
        opacity: 0
        states: State {
            name: "active"
            when: control.policy === T.ScrollBar.AlwaysOn || (control.active && control.size < 1.0)
            PropertyChanges { target: control.contentItem; opacity: control.pressed || control.hovered ? 1 : 0.75 }
        }
        transitions: Transition {
            from: "active"
            SequentialAnimation {
                PauseAnimation { duration: 700 * Motion.k }
                NumberAnimation { target: control.contentItem; duration: Motion.slow; property: "opacity"; to: 0 }
            }
        }
    }
    background: Rectangle {
        implicitWidth: 12; implicitHeight: 12
        radius: width / 2
        color: Theme.hoverTint
        opacity: 0
        visible: control.size < 1.0
        states: State {
            name: "hot"
            when: control.hovered || control.pressed
            PropertyChanges { target: control.background; opacity: 1 }
        }
        Behavior on opacity { NumberAnimation { duration: Motion.fast } }
    }
}
