import QtQuick
import com.skradle.app
import QtQuick.Controls
import QtQuick.Layouts

// Backups of the server's data and settings: make one now, or restore an earlier one. The server is stopped while
// either runs (a database copied under a running server can be torn) and started again afterwards.
ServerDialog {
    id: dlg
    property var items: []
    property string pending: ""
    size: "form"
    title: qsTr("Back up and restore")
    subtitle: qsTr("A backup holds the server's data and settings. The server stops while a backup or a restore runs, and starts again afterwards if it was running.")
    onFocusFirst: backupBtn.forceActiveFocus()
    onAboutToShow: load()
    function load() {
        try { items = JSON.parse(ServerHost.backupsJson()) } catch (e) { items = [] }
    }
    // A backup that finishes while the dialog is open shows up in the list.
    Connections { target: ServerHost; function onStateChanged() { if (dlg.opened) dlg.load() } }

    function whenText(secs) { return new Date(secs * 1000).toLocaleString(Qt.locale(), Locale.ShortFormat) }
    function sizeText(bytes) { return qsTr("%1 MB").arg(Math.max(1, Math.round(bytes / 1048576))) }

    EmptyState {
        visible: dlg.items.length === 0
        glyph: "folder"
        title: qsTr("No backups yet")
        text: qsTr("Back up now saves the server's data and settings so you can come back to them.")
    }
    Repeater {
        model: dlg.items
        RowLayout {
            required property var modelData
            Layout.fillWidth: true
            spacing: Theme.gapStack
            ColumnLayout {
                Layout.fillWidth: true; spacing: 0
                Txt { Layout.fillWidth: true; text: dlg.whenText(modelData.createdAt) + (modelData.label ? "  \u00B7  " + modelData.label : ""); elide: Text.ElideRight; wrapMode: Text.NoWrap }
                Txt { text: dlg.sizeText(modelData.bytes); kind: "caption"; numeric: true }
            }
            SButton {
                text: qsTr("Restore")
                enabled: ServerHost.state === "stopped" || ServerHost.state === "running" || ServerHost.state === "failed"
                Accessible.name: qsTr("Restore the backup from %1").arg(dlg.whenText(modelData.createdAt))
                onClicked: { dlg.pending = modelData.name; confirm.openFrom(this) }
            }
        }
    }
    ConfirmDialog {
        id: confirm
        title: qsTr("Restore this backup?")
        message: qsTr("The server's current data and settings are replaced by the backup. A backup of the current state is made first, so you can go back.")
        confirmText: qsTr("Restore")
        danger: true
        onConfirmed: { ServerHost.restore(dlg.pending); dlg.close() }
    }
    buttons: [
        Item { Layout.fillWidth: true },
        SButton { text: qsTr("Close"); onClicked: dlg.close() },
        SButton {
            id: backupBtn
            text: qsTr("Back up now"); glyph: "download"; highlighted: true
            enabled: ServerHost.state === "stopped" || ServerHost.state === "running" || ServerHost.state === "failed"
            onClicked: { ServerHost.backup(); dlg.close() }
        }
    ]
}
