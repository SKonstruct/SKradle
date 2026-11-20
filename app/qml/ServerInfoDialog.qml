import QtQuick
import com.skradle.app
import QtQuick.Layouts

// The details of one server, and what the game is pointed at when it is selected.
ServerDialog {
    id: dlg
    property var entry: ({})
    property var summary: []
    size: "form"
    title: entry.name ? entry.name : qsTr("Server")
    subtitle: entry.kind === "official" ? qsTr("The official service. It is always in the list and cannot be changed.")
            : entry.kind === "local" ? qsTr("Your own server. It is managed from Your server, below the list.")
            : qsTr("A server you saved.")
    ClipboardHelper { id: clip }
    onFocusFirst: closeBtn.forceActiveFocus()

    function show(item, opener) {
        entry = item
        let list = []
        try { list = JSON.parse(ServerBook.summaryOf(item.sid)) } catch (e) { list = [] }
        summary = list
        openFrom(opener)
    }
    function labelOf(key) {
        switch (key) {
        case "server_host": return qsTr("Server")
        case "server_ports": return qsTr("Port")
        case "key.public": return qsTr("Public key")
        case "client_root_url": return qsTr("Game files from")
        case "content_url": return qsTr("Content from")
        }
        return key
    }

    Repeater {
        model: dlg.summary
        RowLayout {
            required property var modelData
            Layout.fillWidth: true
            spacing: Theme.gapStack
            Txt { Layout.preferredWidth: 120; Layout.alignment: Qt.AlignTop; text: dlg.labelOf(modelData.key); kind: "caption" }
            Txt {
                Layout.fillWidth: true
                text: modelData.value
                wrapMode: Text.WrapAnywhere
                maximumLineCount: 3; elide: Text.ElideRight
                mono: true; numeric: true
            }
        }
    }
    Txt {
        Layout.fillWidth: true; Layout.topMargin: Theme.s2
        text: qsTr("These are the connection settings the game gets when you launch with this server selected.")
        kind: "caption"
    }
    buttons: [
        SButton { text: qsTr("Copy address"); onClicked: { clip.copy(dlg.entry.address); Data.toast(qsTr("Address copied")) } },
        Item { Layout.fillWidth: true },
        SButton { id: closeBtn; text: qsTr("Close"); highlighted: true; onClicked: dlg.close() }
    ]
}
