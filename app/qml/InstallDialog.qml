import QtQuick
import com.skradle.app
import QtQuick.Layouts

// The licence the player must accept before the server engine is downloaded. Nothing is downloaded until Install is
// pressed with the box ticked.
ServerDialog {
    id: dlg
    property var terms: ({})
    property bool update: false
    signal accepted()
    preferredWidth: 520
    title: update ? qsTr("Update the server engine") : qsTr("Install the server")
    subtitle: qsTr("The server is a separate program by its own author. SKradle downloads it for you.")
    onFocusFirst: accept.forceActiveFocus()
    Component.onCompleted: { try { terms = JSON.parse(ServerHost.termsJson()) } catch (e) { terms = ({}) } }
    onAboutToShow: accept.checked = false

    Txt { Layout.fillWidth: true; text: dlg.terms.summary || ""; color: Theme.text }
    RowLayout {
        Layout.fillWidth: true; Layout.topMargin: Theme.s2
        spacing: Theme.s2
        SButton {
            text: qsTr("Read the licence"); variant: "ghost"; glyph: "globe"
            onClicked: Qt.openUrlExternally(dlg.terms.licenceUrl)
            Accessible.name: qsTr("Read the licence online: %1").arg(dlg.terms.licence || "")
        }
        SButton {
            text: qsTr("About the engine"); variant: "ghost"; glyph: "globe"
            onClicked: Qt.openUrlExternally(dlg.terms.engineUrl)
        }
    }
    SCheckBox {
        id: accept
        Layout.fillWidth: true; Layout.topMargin: Theme.s2
        text: qsTr("I have read the licence (%1) and accept it for my use.").arg(dlg.terms.licence || "")
        Accessible.name: text
    }
    buttons: [
        Item { Layout.fillWidth: true },
        SButton { text: qsTr("Cancel"); onClicked: dlg.close() },
        SButton {
            text: dlg.update ? qsTr("Accept and update") : qsTr("Accept and install")
            highlighted: true
            enabled: accept.checked
            onClicked: { dlg.close(); dlg.accepted() }
        }
    ]
}
