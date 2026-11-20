import QtQuick
import com.skradle.app
import QtQuick.Controls
import QtQuick.Layouts

// Add a server, or change one the player saved. Fields: name, address (host or host:port), port, public key,
// getdown URL, content URL. Every keystroke is checked by the core (`ServerBook.check`); a message shows under a
// field once the player has been in it or has tried to save. Enter saves, Escape cancels, Tab stays in the dialog.
ServerDialog {
    id: dlg
    property string editId: ""
    readonly property bool editing: editId.length > 0
    // The fields the player has left or changed, and whether Save was pressed: only those show their message.
    property var touched: ({})
    property bool attempted: false
    property var report: ({ ok: false, errors: ({}), changes: [] })
    property bool portEdited: false
    property string officialKey: ""
    signal saved(string sid)

    title: editing ? qsTr("Edit server") : qsTr("Add server")
    subtitle: qsTr("Enter the details the server's owner gave you. Only the name and the address are required.")
    preferredWidth: 540
    onFocusFirst: nameField.field.forceActiveFocus()

    function draft() {
        return { id: dlg.editId, name: nameField.text, address: addrField.text, port: portField.text,
                 key: keyField.text, getdown: getdownField.text, content: contentField.text }
    }
    function recheck() {
        try { report = JSON.parse(ServerBook.check(JSON.stringify(draft()))) } catch (e) { /* keep the last report */ }
    }
    function touch(field) { const t = touched; t[field] = true; touched = t }
    function reset(values) {
        const v = values || {}
        nameField.text = v.name || ""
        addrField.text = v.address || ""
        portField.text = v.port || "47624"
        keyField.text = v.key || ""
        getdownField.text = v.getdown || ""
        contentField.text = v.content || ""
        touched = ({})
        attempted = false
        portEdited = !!v.port
        try { officialKey = JSON.parse(ServerBook.entries)[0].publicKey } catch (e) { officialKey = "" }
        recheck()
    }
    // Screenshots only: a form with several problems, all shown.
    function openDemo(opener) {
        editId = ""
        reset({ name: "", address: "play.example.com:99999", port: "0", key: "xyz", getdown: "ftp://files", content: "" })
        attempted = true
        recheck()
        openFrom(opener)
    }
    function openAdd(opener) { editId = ""; reset(null); openFrom(opener) }
    function openEdit(sid, opener) {
        let values = null
        try { values = JSON.parse(ServerBook.draftOf(sid)) } catch (e) { values = null }
        if (!values) return
        editId = sid
        reset(values)
        openFrom(opener)
    }

    function messageOf(field) {
        if (!(attempted || touched[field])) return ""
        const e = report.errors ? report.errors[field] : undefined
        if (!e) return ""
        switch (e.code) {
        case "name_required": return qsTr("Enter a name.")
        case "name_long": return qsTr("Use 40 characters or fewer.")
        case "name_duplicate": return qsTr("A server with this name already exists.")
        case "address_required": return qsTr("Enter the server's address, like play.example.com or 192.168.1.20.")
        case "address_invalid": return qsTr("This isn't a valid host name or address. Use letters, digits, dots and dashes.")
        case "address_port": return qsTr("The port after the colon must be a number from 1 to 65535.")
        case "address_official": return qsTr("That is the official server, which is always in the list.")
        case "address_duplicate": return qsTr("This address is already saved as “%1”.").arg(e.detail)
        case "port_required": return qsTr("Enter a port from 1 to 65535.")
        case "port_range": return qsTr("The port must be a number from 1 to 65535.")
        case "port_mismatch": return qsTr("The address says port %1. Make the two match or leave one out.").arg(e.detail)
        case "key_invalid": return qsTr("A public key looks like 1a2b3c…#10001: hexadecimal digits, a #, more digits.")
        case "url_invalid": return qsTr("Use a web address that starts with http:// or https://.")
        case "managed": return qsTr("This server is managed from the controls above the list.")
        case "missing": return qsTr("This server no longer exists.")
        case "write": return qsTr("The list could not be saved. Check that the settings folder is writable.")
        }
        return qsTr("Something is not right with this server.")
    }
    readonly property var order: ["name", "address", "port", "key", "getdown", "content"]
    function firstInvalid() {
        for (const f of order) if (report.errors && report.errors[f]) return f
        return ""
    }
    function fieldItem(name) {
        return { name: nameField, address: addrField, port: portField, key: keyField, getdown: getdownField, content: contentField }[name]
    }
    function submit() {
        attempted = true
        recheck()
        if (!report.ok) {
            const f = firstInvalid()
            if (f) fieldItem(f).field.forceActiveFocus()
            else if (report.errors && report.errors.form) formMsg.visible = true
            return
        }
        const result = JSON.parse(ServerBook.save(JSON.stringify(draft())))
        if (result.ok) {
            const wasEdit = editing
            close()
            Data.toast(wasEdit ? qsTr("Server saved") : qsTr("Server added"))
            saved(result.id)
        } else {
            report = result
        }
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
    function shown(change) {
        if (change.key === "key.public" && change.value === officialKey) return qsTr("Official's key (not changed)")
        if (change.key === "client_root_url" && change.value === "http://gamemedia2.spiralknights.com") return qsTr("Official's address (not changed)")
        if (change.key === "content_url" && change.value === "http://content.spiralknights.com/") return qsTr("Official's address (not changed)")
        return change.value
    }

    component Field: ColumnLayout {
        id: f
        property string label
        property string name
        property alias field: input
        property alias text: input.text
        property alias placeholder: input.placeholderText
        property alias validator: input.validator
        property alias inputHint: input.inputMethodHints
        signal textEdited()
        Layout.fillWidth: true
        spacing: 4
        readonly property string message: dlg.messageOf(name)
        Txt { text: f.label; kind: "caption"; font.weight: Font.Medium; Layout.fillWidth: true }
        STextField {
            id: input
            Layout.fillWidth: true
            Accessible.name: f.label
            Accessible.description: f.message
            onTextChanged: dlg.recheck()
            onTextEdited: f.textEdited()
            onEditingFinished: dlg.touch(f.name)
            onActiveFocusChanged: if (activeFocus) dlg.reveal(f)
            onAccepted: dlg.submit()
            background: Rectangle {
                implicitWidth: 200; implicitHeight: 38; radius: 10
                color: Theme.field
                border.width: input.activeFocus || f.message.length > 0 ? 2 : 1
                border.color: f.message.length > 0 ? Theme.danger : input.activeFocus ? Theme.accentFg : (input.hovered ? Theme.borderStrong : Theme.fieldBorder)
            }
        }
        RowLayout {
            Layout.fillWidth: true
            visible: f.message.length > 0
            spacing: 6
            Icon { name: "warning"; size: 14; color: Theme.danger; Layout.alignment: Qt.AlignTop; Layout.topMargin: 2 }
            Txt { Layout.fillWidth: true; text: f.message; kind: "caption"; color: Theme.danger }
        }
    }

    Field { id: nameField; label: qsTr("Server name"); name: "name"; placeholder: qsTr("My friends' server") }
    RowLayout {
        Layout.fillWidth: true
        spacing: Theme.s3
        Field {
            id: addrField; label: qsTr("Server address"); name: "address"; placeholder: "play.example.com"
            Layout.alignment: Qt.AlignTop
            onTextEdited: {
                const p = ServerBook.portOf(text)
                if (p > 0 && !dlg.portEdited) portField.text = String(p)
            }
        }
        Field {
            id: portField; label: qsTr("Port"); name: "port"; placeholder: "47624"
            Layout.fillWidth: false; Layout.preferredWidth: 120; Layout.alignment: Qt.AlignTop
            validator: IntValidator { bottom: 0; top: 99999 }
            inputHint: Qt.ImhDigitsOnly
            onTextEdited: dlg.portEdited = true
        }
    }
    Field { id: keyField; label: qsTr("Public key (optional)"); name: "key"; placeholder: qsTr("Leave empty to use the official key") }
    Field { id: getdownField; label: qsTr("Getdown URL (optional)"); name: "getdown"; placeholder: "https://files.example.com/client" }
    Field { id: contentField; label: qsTr("Content URL (optional)"); name: "content"; placeholder: "https://content.example.com/" }

    Txt {
        id: formMsg
        Layout.fillWidth: true
        visible: false
        text: dlg.messageOf("form") || (dlg.report.errors && dlg.report.errors.form ? qsTr("This server can't be saved.") : "")
        color: Theme.danger; kind: "caption"
    }

    // What launching with this server does to the game's configuration.
    ColumnLayout {
        Layout.fillWidth: true
        Layout.topMargin: Theme.s2
        spacing: 4
        Txt { text: qsTr("What changes in the game"); kind: "caption"; font.weight: Font.Medium }
        Txt {
            Layout.fillWidth: true
            visible: !dlg.report.ok
            text: qsTr("Fill in the name and the address to see the settings the game would get.")
            kind: "caption"
        }
        Repeater {
            model: dlg.report.ok ? dlg.report.changes : []
            RowLayout {
                required property var modelData
                Layout.fillWidth: true
                spacing: Theme.s3
                Txt { Layout.preferredWidth: 110; Layout.alignment: Qt.AlignTop; text: dlg.labelOf(modelData.key); kind: "caption" }
                Txt {
                    Layout.fillWidth: true
                    text: dlg.shown(modelData)
                    kind: "caption"; color: Theme.text
                    wrapMode: Text.WrapAnywhere; maximumLineCount: 2; elide: Text.ElideRight
                }
            }
        }
    }

    Rectangle {
        Layout.fillWidth: true
        Layout.topMargin: Theme.s2
        implicitHeight: trust.implicitHeight + 2 * Theme.s3
        radius: Theme.rMd
        color: Theme.amberTint
        RowLayout {
            id: trust
            anchors.fill: parent; anchors.margins: Theme.s3
            spacing: Theme.s3
            Icon { name: "warning"; size: 18; color: Theme.amberText; Layout.alignment: Qt.AlignTop }
            Txt {
                Layout.fillWidth: true
                text: qsTr("Only add servers you trust. A server receives your sign-in, and these addresses decide where the game's files and content are loaded from.")
                kind: "caption"; color: Theme.text
            }
        }
    }

    buttons: [
        Item { Layout.fillWidth: true },
        SButton { text: qsTr("Cancel"); onClicked: dlg.close() },
        SButton { text: dlg.editing ? qsTr("Save changes") : qsTr("Add server"); highlighted: true; onClicked: dlg.submit() }
    ]
}
