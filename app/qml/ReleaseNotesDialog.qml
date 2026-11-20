import QtQuick
import com.skradle.app
import QtQuick.Layouts

// The release notes: the changelog bundled into the app (nothing is downloaded), one block per version, newest first.
// The newest block is open when the dialog opens. Clicking a version header folds that block; one button opens or
// closes all of them.
ServerDialog {
    id: dlg
    size: "reading"
    title: qsTr("Release notes")
    subtitle: qsTr("What changed in SKradle. You have version %1.").arg(AppEnv.version)

    readonly property var versions: { try { return JSON.parse(ReleaseNotes.json) } catch (e) { return [] } }
    property var openSet: []                       // by index: is the block open
    readonly property bool allOpen: versions.length > 0 && versions.every((v, i) => openSet[i] === true)
    onFocusFirst: closeBtn.forceActiveFocus()
    onAboutToShow: setAll(false, true)

    function show(opener) { openFrom(opener) }
    function toggle(i) { const a = openSet.slice(); a[i] = !(a[i] === true); openSet = a }
    // Opens or closes every block; `keepNewest` leaves only the newest open (the state the dialog starts in).
    function setAll(open, keepNewest) {
        const a = []
        for (let i = 0; i < versions.length; i++) a.push(keepNewest ? i === 0 : open)
        openSet = a
    }

    SButton {
        Layout.alignment: Qt.AlignRight
        visible: dlg.versions.length > 1
        variant: "ghost"
        size: "sm"
        text: dlg.allOpen ? qsTr("Collapse all") : qsTr("Expand all")
        onClicked: dlg.setAll(!dlg.allOpen, false)
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
            first: index === 0
            expanded: dlg.openSet[index] === true
            onToggled: dlg.toggle(index)
        }
    }
    buttons: [
        Item { Layout.fillWidth: true },
        SButton { id: closeBtn; text: qsTr("Close"); variant: "primary"; onClicked: dlg.close() }
    ]
}
