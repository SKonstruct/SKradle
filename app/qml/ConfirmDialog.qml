import QtQuick
import com.skradle.app
import QtQuick.Layouts

// "Are you sure?" for something that cannot be undone. The safe button (Cancel) has the focus.
ServerDialog {
    id: dlg
    property string message
    property string confirmText: qsTr("Confirm")
    property bool danger: false
    signal confirmed()

    onFocusFirst: cancelBtn.forceActiveFocus()
    Txt { Layout.fillWidth: true; text: dlg.message; color: Theme.textSecondary }
    buttons: [
        Item { Layout.fillWidth: true },
        SButton { id: cancelBtn; text: qsTr("Cancel"); onClicked: dlg.close() },
        SButton {
            text: dlg.confirmText
            variant: dlg.danger ? "danger" : "primary"
            onClicked: { dlg.close(); dlg.confirmed() }
        }
    ]
}
