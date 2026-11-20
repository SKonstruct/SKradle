import QtQuick
import com.skradle.app
import QtQuick.Controls

// A jump tab of the Settings page: the current section has bold text and an underline.
AbstractButton {
    id: root
    property bool current: false
    property int radius: Theme.rSm
    hoverEnabled: true
    activeFocusOnTab: true
    focusPolicy: Qt.StrongFocus
    implicitHeight: Theme.hSm
    implicitWidth: label.implicitWidth + 2 * Theme.s3
    Accessible.name: text
    Accessible.role: Accessible.PageTab
    Accessible.selected: current
    scale: down ? 0.96 : 1
    Behavior on scale { NumberAnimation { duration: Motion.instant } }

    background: Rectangle {
        radius: root.radius
        color: root.hovered ? Theme.stateHover : "transparent"
        Behavior on color { ColorAnimation { duration: Motion.fast } }
        Rectangle {   // the current section: bold text plus a 2 px underline
            x: Theme.s3; width: parent.width - 2 * Theme.s3
            y: parent.height - 2; height: 2
            radius: 1
            color: Theme.accentFg
            opacity: root.current ? 1 : 0
            Behavior on opacity { NumberAnimation { duration: Motion.fast } }
        }
    }
    contentItem: Item {
        Txt {
            id: label
            anchors.centerIn: parent
            text: root.text
            kind: "caption"
            wrapMode: Text.NoWrap
            font.weight: root.current ? Font.DemiBold : Font.Medium
            color: root.current ? Theme.text : Theme.textSecondary
        }
    }
    FocusRing {}
}
