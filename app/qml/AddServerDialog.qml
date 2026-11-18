import QtQuick
import com.skradle.app
import QtQuick.Controls
import QtQuick.Layouts

Popup {
    id: dlg
    parent: Overlay.overlay
    anchors.centerIn: parent
    modal: true
    focus: true
    padding: 0
    width: Math.min(460, (parent ? parent.width : 460) - 48)
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
    property bool shown: false
    signal accepted(string name, string address)

    onAboutToShow: { shown = true; nameField.text = ""; addrField.text = "" }
    onAboutToHide: shown = false
    onOpened: nameField.forceActiveFocus()

    // Animated scrim: opacity only.
    Overlay.modal: Rectangle {
        color: Theme.scrim
    }
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

    readonly property bool valid: nameField.text.trim().length > 0 && addrField.text.trim().length > 2
    function submit() {
        if (!valid) return
        accepted(nameField.text.trim(), addrField.text.trim())
        close()
    }

    contentItem: ColumnLayout {
        spacing: Theme.s4
        Accessible.role: Accessible.Dialog
        Accessible.name: qsTr("Add server")
        Item { Layout.preferredHeight: 0 }
        ColumnLayout {
            Layout.fillWidth: true
            Layout.leftMargin: Theme.s6; Layout.rightMargin: Theme.s6
            spacing: 4
            Txt { Layout.fillWidth: true; text: qsTr("Add server"); kind: "title"; Accessible.role: Accessible.Heading }
            Txt { Layout.fillWidth: true; text: qsTr("Enter the name and address the server owner gave you."); color: Theme.textSecondary }
        }
        ColumnLayout {
            Layout.fillWidth: true
            Layout.leftMargin: Theme.s6; Layout.rightMargin: Theme.s6
            spacing: Theme.s2
            Txt { text: qsTr("Name"); kind: "caption"; font.weight: Font.Medium }
            STextField {
                id: nameField
                Layout.fillWidth: true
                placeholderText: qsTr("My server")
                Accessible.name: qsTr("Name")
                onAccepted: addrField.forceActiveFocus()
            }
            Txt { Layout.topMargin: Theme.s2; text: qsTr("Address"); kind: "caption"; font.weight: Font.Medium }
            STextField {
                id: addrField
                Layout.fillWidth: true
                placeholderText: "host.example.com:47011"
                Accessible.name: qsTr("Address")
                onAccepted: dlg.submit()
            }
        }
        Divider { Layout.topMargin: Theme.s2 }
        RowLayout {
            Layout.fillWidth: true
            Layout.leftMargin: Theme.s6; Layout.rightMargin: Theme.s6
            Layout.bottomMargin: Theme.s5
            spacing: Theme.s2
            Item { Layout.fillWidth: true }
            SButton { text: qsTr("Cancel"); onClicked: dlg.close() }
            SButton { text: qsTr("Add server"); highlighted: true; enabled: dlg.valid; onClicked: dlg.submit() }
        }
    }
}
