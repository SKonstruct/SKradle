import QtQuick
import com.skradle.app
import QtQuick.Controls

// The big Launch button. One item, four faces: fill colour, label and glyph morph between states.
AbstractButton {
    id: root
    // ready | updating | launching | running; a failed launch shows the Launch button again (the page shows why)
    property string phase: LaunchModel.state === "error" ? "ready" : LaunchModel.state
    implicitWidth: 236
    implicitHeight: Theme.hLaunch
    hoverEnabled: true
    activeFocusOnTab: true
    focusPolicy: Qt.StrongFocus
    property int radius: Theme.rLg
    // Not while the first look for the game is still going: there is no install to start yet.
    enabled: phase !== "launching" && !(phase === "ready" && GameModel.state !== "ready")
    Accessible.role: Accessible.Button
    Accessible.name: phase === "ready" ? (LaunchModel.state === "error" ? qsTr("Try again") : qsTr("Launch")) : phase === "updating" ? qsTr("Cancel update")
                   : phase === "launching" ? qsTr("Launching") : qsTr("Stop game")

    readonly property bool primary: phase === "ready"
    scale: down ? 0.97 : 1
    Behavior on scale { NumberAnimation { duration: Motion.instant; easing.type: Easing.OutQuad } }

    background: Item {
        Item {    // indeterminate bar inside the bottom edge (clipped, so the sliding segment never leaves the track)
            z: 2
            x: Theme.s5; y: parent.height - Theme.s3 - 1; width: parent.width - 2 * Theme.s5; height: 3
            clip: true
            opacity: root.phase === "launching" ? 1 : 0
            visible: opacity > 0.01
            Behavior on opacity { NumberAnimation { duration: Motion.base } }
            Rectangle { anchors.fill: parent; radius: Theme.rXs / 2; color: Qt.rgba(1, 1, 1, 0.28) }
            Rectangle {
                id: seg
                width: parent.width * 0.38; height: 3; radius: Theme.rXs / 2
                color: Theme.accentText
                SequentialAnimation on x {
                    loops: Animation.Infinite
                    running: root.phase === "launching" && root.visible
                    NumberAnimation { from: -seg.width; to: seg.parent.width; duration: Motion.loop; easing.type: Easing.InOutSine }
                }
            }
        }
        Rectangle {
            id: fillRect
            anchors.fill: parent
            radius: root.radius
            color: root.primary ? (root.down ? Theme.accentPressed : root.hovered ? Theme.accentHover : Theme.accent)
                 : root.phase === "launching" ? Theme.accentPressed   // opaque: white text is 7.7:1 (light) / 6.5:1 (dark), half-transparent pink was 2.1:1 on white
                 : (root.down ? Theme.pressTint : root.hovered ? Theme.hoverTint : Theme.raised)
            border.width: root.primary || root.phase === "launching" ? 0 : 1
            border.color: Theme.borderStrong
            Behavior on color { ColorAnimation { duration: Motion.base } }
        }
    }

    contentItem: Item {
        // READY
        Row {
            anchors.centerIn: parent
            spacing: Theme.s2
            opacity: root.phase === "ready" ? 1 : 0
            visible: opacity > 0.01
            Behavior on opacity { NumberAnimation { duration: Motion.base } }
            Icon { name: "play"; size: 22; color: Theme.accentText; anchors.verticalCenter: parent.verticalCenter }
            Txt { text: LaunchModel.state === "error" ? qsTr("Try again") : qsTr("Launch"); font.pixelSize: Theme.fsHeading + 2; font.weight: Font.DemiBold; color: Theme.accentText; wrapMode: Text.NoWrap; anchors.verticalCenter: parent.verticalCenter }
        }
        // UPDATING (cancel)
        Row {
            anchors.centerIn: parent
            spacing: Theme.s2
            opacity: root.phase === "updating" ? 1 : 0
            visible: opacity > 0.01
            Behavior on opacity { NumberAnimation { duration: Motion.base } }
            Txt { text: qsTr("Cancel"); font.pixelSize: Theme.fsHeading; font.weight: Font.DemiBold; wrapMode: Text.NoWrap; anchors.verticalCenter: parent.verticalCenter }
        }
        // LAUNCHING
        Item {
            anchors.fill: parent
            opacity: root.phase === "launching" ? 1 : 0
            visible: opacity > 0.01
            Behavior on opacity { NumberAnimation { duration: Motion.base } }
            Txt { anchors.centerIn: parent; text: qsTr("Launching"); font.pixelSize: Theme.fsHeading; font.weight: Font.DemiBold; color: Theme.accentText; wrapMode: Text.NoWrap }
        }
        // RUNNING (stop)
        Row {
            anchors.centerIn: parent
            spacing: Theme.s2
            opacity: root.phase === "running" ? 1 : 0
            visible: opacity > 0.01
            Behavior on opacity { NumberAnimation { duration: Motion.base } }
            Icon { name: "stop"; size: 20; color: Theme.danger; anchors.verticalCenter: parent.verticalCenter }
            Txt { text: qsTr("Stop game"); font.pixelSize: Theme.fsHeading; font.weight: Font.DemiBold; wrapMode: Text.NoWrap; anchors.verticalCenter: parent.verticalCenter }
        }
    }
    FocusRing { grow: 4 }

    onClicked: {
        if (phase === "ready") LaunchModel.start(GameModel.selectedPath)
        else if (phase === "updating") LaunchModel.cancel()
        else if (phase === "running") LaunchModel.stop()
    }
}
