import QtQuick
import com.skradle.app
import QtQuick.Layouts

// SirDank's Modpack as one compact button next to the page's other actions: it says what the next step is (install,
// update, check again, or the progress while it downloads). What the last sync did is shown by the page in a banner,
// only when there is something to say. State comes from ModsModel.
RowLayout {
    id: root
    property bool usable: true
    readonly property string st: ModsModel.packState
    signal checkRequested()
    spacing: Theme.s1

    readonly property string status: {
        switch (st) {
        case "checking": return qsTr("Checking for changes…")
        case "notinstalled": return qsTr("Not installed. %n mod(s) to download.", "", ModsModel.packTotal)
        case "update": return qsTr("Update available. %n file(s) changed.", "", ModsModel.packChanges)
        case "current": return qsTr("Up to date. Click to check again.")
        case "syncing": return qsTr("Downloading %1 of %2: %3").arg(ModsModel.packCurrent).arg(ModsModel.packTotal).arg(ModsModel.packName)
        case "error": return ModsModel.packMessage
        }
        return qsTr("A maintained collection of mods. Mods you added are never touched.")
    }
    readonly property bool installable: st === "unknown" || st === "notinstalled" || st === "update"

    SButton {
        id: button
        glyph: root.st === "error" ? "warning"
             : root.st === "current" ? "check"
             : root.st === "update" || root.st === "checking" ? "refresh"
             : "download"
        text: root.st === "update" ? qsTr("Update modpack")
            : root.st === "current" ? qsTr("Modpack up to date")
            : root.st === "checking" ? qsTr("Checking modpack…")
            : root.st === "syncing" ? qsTr("Downloading %1 of %2").arg(ModsModel.packCurrent).arg(ModsModel.packTotal)
            : root.st === "error" ? qsTr("Try modpack again")
            : qsTr("Install modpack")
        highlighted: root.st === "update"
        enabled: root.usable && !ModsModel.busy && root.st !== "checking" && root.st !== "syncing"
        Accessible.name: qsTr("SirDank’s Modpack: %1").arg(root.status)
        onClicked: root.installable ? ModsModel.syncPack() : root.checkRequested()
        Tip { text: qsTr("SirDank’s Modpack: %1").arg(root.status); wanted: button.hovered }
    }
    IconButton {
        visible: root.st === "syncing"
        glyph: "close"
        tip: qsTr("Cancel the download of SirDank’s Modpack")
        onClicked: ModsModel.cancelSync()
    }
}
