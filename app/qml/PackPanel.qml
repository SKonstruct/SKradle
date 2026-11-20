import QtQuick
import com.skradle.app
import QtQuick.Controls
import QtQuick.Layouts

// The maintained modpack: install it, keep it up to date, see what a sync did. State comes from ModsModel.
Card {
    id: root
    property bool usable: true
    readonly property string st: ModsModel.packState
    readonly property var report: { try { return ModsModel.packReport === "" ? null : JSON.parse(ModsModel.packReport) } catch (e) { return null } }
    signal checkRequested()
    implicitHeight: col.implicitHeight + 2 * Theme.s5

    function reportSummary(r) {
        const parts = []
        parts.push(qsTr("%n downloaded", "", r.downloaded.length))
        if (r.skipped > 0) parts.push(qsTr("%n already current", "", r.skipped))
        if (r.deleted.length > 0) parts.push(qsTr("%n removed", "", r.deleted.length))
        if (r.failed.length > 0) parts.push(qsTr("%n failed", "", r.failed.length))
        return (r.cancelled ? qsTr("Stopped. ") : "") + parts.join(", ")
    }
    readonly property string headline: {
        switch (st) {
        case "checking": return qsTr("Checking the modpack…")
        case "notinstalled": return qsTr("Not installed")
        case "update": return qsTr("Update available")
        case "current": return qsTr("Up to date")
        case "syncing": return qsTr("Downloading %1 of %2").arg(ModsModel.packCurrent).arg(ModsModel.packTotal)
        case "error": return qsTr("Could not reach the modpack")
        }
        return qsTr("Maintained modpack")
    }
    readonly property string detail: {
        switch (st) {
        case "notinstalled": return qsTr("%n mod(s) to download. Each file is checked against its published hash.", "", ModsModel.packTotal)
        case "update": return qsTr("%n file(s) changed upstream. Only those are downloaded.", "", ModsModel.packChanges)
        case "current": return qsTr("Every file matches the published modpack.")
        case "syncing": return ModsModel.packName
        case "error": return qsTr("Check your internet connection and try again. (%1)").arg(ModsModel.packMessage)
        case "checking": return ""
        }
        return qsTr("Installs the maintained collection of mods and keeps it up to date. Mods you added yourself are never touched.")
    }

    ColumnLayout {
        id: col
        anchors { left: parent.left; right: parent.right; top: parent.top; margins: Theme.s5 }
        spacing: Theme.s3

        RowLayout {
            Layout.fillWidth: true
            spacing: Theme.s4
            Rectangle {
                Layout.preferredWidth: 44; Layout.preferredHeight: 44; radius: 11
                color: root.st === "error" ? Theme.amberTint : Theme.accentTint
                Icon {
                    anchors.centerIn: parent; size: 22
                    name: root.st === "error" ? "warning" : root.st === "current" ? "check" : "download"
                    color: root.st === "error" ? Theme.amberText : Theme.accentFg
                }
            }
            ColumnLayout {
                Layout.fillWidth: true
                spacing: 2
                Txt { Layout.fillWidth: true; text: qsTr("Modpack"); kind: "heading"; Accessible.role: Accessible.Heading }
                Txt { Layout.fillWidth: true; text: root.headline; font.weight: Font.Medium; color: root.st === "error" ? Theme.amberText : Theme.text; elide: Text.ElideRight; wrapMode: Text.NoWrap }
                Txt { Layout.fillWidth: true; visible: root.detail.length > 0; text: root.detail; kind: "caption"; elide: root.st === "syncing" ? Text.ElideMiddle : Text.ElideNone; wrapMode: root.st === "syncing" ? Text.NoWrap : Text.WordWrap }
            }
            SButton {
                visible: root.st === "syncing"
                text: qsTr("Cancel")
                Accessible.name: qsTr("Cancel the modpack download")
                onClicked: ModsModel.cancelSync()
            }
            SButton {
                visible: root.st === "unknown" || root.st === "notinstalled" || root.st === "update"
                highlighted: true
                enabled: root.usable && !ModsModel.busy
                glyph: root.st === "update" ? "refresh" : "download"
                text: root.st === "update" ? qsTr("Update modpack") : qsTr("Install modpack")
                onClicked: ModsModel.syncPack()
            }
            SButton {
                visible: root.st === "current" || root.st === "error"
                enabled: root.usable
                glyph: "refresh"
                text: root.st === "error" ? qsTr("Try again") : qsTr("Check again")
                onClicked: root.checkRequested()
            }
        }

        Rectangle {   // determinate progress (file n of total)
            Layout.fillWidth: true
            visible: root.st === "syncing"
            implicitHeight: 6; radius: 3
            color: Theme.track
            Accessible.role: Accessible.ProgressBar
            Accessible.name: qsTr("Modpack download progress")
            Rectangle {
                height: parent.height; radius: 3
                color: Theme.accent
                width: ModsModel.packTotal > 0 ? parent.width * Math.max(0, ModsModel.packCurrent - 1) / ModsModel.packTotal : 0
                Behavior on width { NumberAnimation { duration: Motion.base } }
            }
        }

        ColumnLayout {   // what the last sync did
            Layout.fillWidth: true
            visible: root.report !== null
            spacing: Theme.s2
            Divider {}
            RowLayout {
                Layout.fillWidth: true
                Txt { Layout.fillWidth: true; text: root.report ? root.reportSummary(root.report) : ""; font.weight: Font.Medium }
                SButton { variant: "ghost"; text: qsTr("Dismiss"); Accessible.name: qsTr("Dismiss the modpack report"); onClicked: ModsModel.dismissReport() }
            }
            Repeater {
                model: root.report ? root.report.failed : []
                delegate: Txt {
                    required property var modelData
                    Layout.fillWidth: true
                    text: modelData[0] + ": " + modelData[1]
                    kind: "caption"
                    color: Theme.danger
                }
            }
            Repeater {
                model: root.report ? root.report.deleted : []
                delegate: Txt {
                    required property var modelData
                    Layout.fillWidth: true
                    text: qsTr("Removed %1 (dropped from the modpack)").arg(modelData)
                    kind: "caption"
                }
            }
        }
    }
}
