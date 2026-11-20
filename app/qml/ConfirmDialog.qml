import QtQuick
import com.skradle.app
import QtQuick.Controls
import QtQuick.Layouts

// A modal question with a cancel and a confirm button. The confirm button is red when `danger` is set.
Popup {
    id: dlg
    parent: Overlay.overlay
    anchors.centerIn: parent
    modal: true
    focus: true
    padding: 0
    width: Math.min(480, (parent ? parent.width : 480) - 48)
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
    property string title
    property string message
    property string confirmText: qsTr("Confirm")
    property bool danger: false
    signal confirmed()

    Overlay.modal: Rectangle { color: Theme.scrim }
    enter: Transition {
        ParallelAnimation {
            NumberAnimation { property: "opacity"; from: 0; to: 1; duration: Motion.base; easing.type: Easing.BezierSpline; easing.bezierCurve: Motion.decel }
            NumberAnimation { property: "scale"; from: 0.95; to: 1; duration: Motion.slow * 0.85; easing.type: Easing.BezierSpline; easing.bezierCurve: Motion.decel }
        }
    }
    exit: Transition {
        ParallelAnimation {
            NumberAnimation { property: "opacity"; to: 0; duration: Motion.fast; easing.type: Easing.BezierSpline; easing.bezierCurve: Motion.accel }
            NumberAnimation { property: "scale"; to: 0.97; duration: Motion.fast; easing.type: Easing.BezierSpline; easing.bezierCurve: Motion.accel }
        }
    }
    background: Card { radius: Theme.rXl; fill: Theme.card }

    contentItem: ColumnLayout {
        spacing: Theme.s4
        Accessible.role: Accessible.AlertMessage
        Accessible.name: dlg.title + ". " + dlg.message
        Item { Layout.preferredHeight: 0 }
        ColumnLayout {
            Layout.fillWidth: true
            Layout.leftMargin: Theme.s6; Layout.rightMargin: Theme.s6
            spacing: Theme.s2
            Txt { Layout.fillWidth: true; text: dlg.title; kind: "title"; Accessible.role: Accessible.Heading }
            Txt { Layout.fillWidth: true; text: dlg.message; color: Theme.textSecondary }
        }
        RowLayout {
            Layout.fillWidth: true
            Layout.leftMargin: Theme.s6; Layout.rightMargin: Theme.s6; Layout.bottomMargin: Theme.s5
            spacing: Theme.s2
            Item { Layout.fillWidth: true }
            SButton { text: qsTr("Cancel"); onClicked: dlg.close() }
            SButton {
                text: dlg.confirmText
                variant: dlg.danger ? "danger" : "primary"
                onClicked: { dlg.close(); dlg.confirmed() }
            }
        }
    }
}
