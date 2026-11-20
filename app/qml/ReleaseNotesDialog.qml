import QtQuick
import com.skradle.app
import QtQuick.Layouts

// The release notes: the changelog bundled into the app (nothing is downloaded), one block per version. The newest
// block is open when the dialog opens; each block can be minimised or maximised, and the two buttons do all of them.
ServerDialog {
    id: dlg
    preferredWidth: 760
    title: qsTr("Release notes")
    subtitle: qsTr("What changed in SKradle. You have version %1.").arg(AppEnv.version)

    readonly property var versions: { try { return JSON.parse(ReleaseNotes.json) } catch (e) { return [] } }
    onFocusFirst: closeBtn.forceActiveFocus()
    onAboutToShow: setAll(false, true)

    function show(opener) { openFrom(opener) }
    // Opens or closes every block; `keepNewest` leaves the newest open (the state the dialog starts in).
    function setAll(open, keepNewest) {
        for (let i = 0; i < blocks.count; i++) {
            const b = blocks.itemAt(i)
            if (b) b.expanded = keepNewest ? i === 0 : open
        }
    }

    RowLayout {
        Layout.fillWidth: true
        visible: dlg.versions.length > 0
        spacing: Theme.s2
        Item { Layout.fillWidth: true }
        SButton { text: qsTr("Expand all"); variant: "ghost"; onClicked: dlg.setAll(true, false) }
        SButton { text: qsTr("Collapse all"); variant: "ghost"; onClicked: dlg.setAll(false, false) }
    }
    Txt {
        Layout.fillWidth: true
        visible: dlg.versions.length === 0
        text: qsTr("There are no release notes in this build.")
        color: Theme.textSecondary
    }
    Repeater {
        id: blocks
        model: dlg.versions
        VersionBlock {
            required property int index
            required property var modelData
            version: modelData
            expanded: index === 0
        }
    }
    buttons: [
        Item { Layout.fillWidth: true },
        SButton { id: closeBtn; text: qsTr("Close"); highlighted: true; onClicked: dlg.close() }
    ]
}
