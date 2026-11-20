import QtQuick
import com.skradle.app
import QtQuick.Controls
import QtQuick.Layouts

// Ask before something that cannot be undone. Focus starts on Cancel; Escape cancels; focus goes back to where it was.
Popup {
    id: dlg
    property string title
    property string message
    property string confirmText: qsTr("Confirm")
    property bool danger: false
    signal confirmed()

    parent: Overlay.overlay
    anchors.centerIn: parent
    modal: true
    focus: true
    padding: 0
    width: Math.min(440, (parent ? parent.width : 440) - 48)
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
    onOpened: cancelBtn.forceActiveFocus()

    Overlay.modal: Rectangle { color: Theme.scrim }
    enter: Transition {
        NumberAnimation { property: "opacity"; from: 0; to: 1; duration: Motion.base; easing.type: Easing.BezierSpline; easing.bezierCurve: Motion.decel }
    }
    exit: Transition {
        NumberAnimation { property: "opacity"; to: 0; duration: Motion.fast; easing.type: Easing.BezierSpline; easing.bezierCurve: Motion.accel }
    }
    background: Card { radius: Theme.rXl; fill: Theme.card }

    contentItem: ColumnLayout {
        spacing: Theme.s4
        Accessible.role: Accessible.Dialog
        Accessible.name: dlg.title
        Item { Layout.preferredHeight: 0 }
        ColumnLayout {
            Layout.fillWidth: true
            Layout.leftMargin: Theme.s6; Layout.rightMargin: Theme.s6
            spacing: Theme.s2
            Txt { Layout.fillWidth: true; text: dlg.title; kind: "title"; Accessible.role: Accessible.Heading }
            Txt { Layout.fillWidth: true; text: dlg.message; color: Theme.textSecondary }
        }
        Divider { Layout.topMargin: Theme.s2 }
        RowLayout {
            Layout.fillWidth: true
            Layout.leftMargin: Theme.s6; Layout.rightMargin: Theme.s6
            Layout.bottomMargin: Theme.s5
            spacing: Theme.s2
            Item { Layout.fillWidth: true }
            SButton { id: cancelBtn; text: qsTr("Cancel"); onClicked: dlg.close() }
            SButton {
                text: dlg.confirmText
                highlighted: !dlg.danger
                variant: dlg.danger ? "danger" : "primary"
                onClicked: { dlg.close(); dlg.confirmed() }
            }
        }
    }
}
