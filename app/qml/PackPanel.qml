import QtQuick
import com.skradle.app
import QtQuick.Controls
import QtQuick.Layouts

// SirDank's Modpack as a compact card: name, one line of status, the button for the next step. What the last
// sync did opens under it on demand. State comes from ModsModel.
Card {
    id: root
    property bool usable: true
    property bool open: false
    readonly property string st: ModsModel.packState
    readonly property var report: { try { return ModsModel.packReport === "" ? null : JSON.parse(ModsModel.packReport) } catch (e) { return null } }
    signal checkRequested()
    elevation: 0
    implicitHeight: col.implicitHeight + 2 * Theme.s3

    function reportSummary(r) {
        const parts = []
        parts.push(qsTr("%n downloaded", "", r.downloaded.length))
        if (r.skipped > 0) parts.push(qsTr("%n already current", "", r.skipped))
        if (r.deleted.length > 0) parts.push(qsTr("%n removed", "", r.deleted.length))
        if (r.failed.length > 0) parts.push(qsTr("%n failed", "", r.failed.length))
        return (r.cancelled ? qsTr("Stopped. ") : "") + parts.join(", ")
    }
    readonly property string status: {
        switch (st) {
        case "checking": return qsTr("Checking for changes…")
        case "notinstalled": return qsTr("Not installed. %n mod(s) to download.", "", ModsModel.packTotal)
        case "update": return qsTr("Update available. %n file(s) changed.", "", ModsModel.packChanges)
        case "current": return qsTr("Up to date")
        case "syncing": return qsTr("Downloading %1 of %2: %3").arg(ModsModel.packCurrent).arg(ModsModel.packTotal).arg(ModsModel.packName)
        case "error": return ModsModel.packMessage
        }
        return qsTr("A maintained collection of mods. Mods you added are never touched.")
    }
    readonly property bool bad: st === "error" || (report !== null && report.failed.length > 0)

    ColumnLayout {
        id: col
        anchors { left: parent.left; right: parent.right; top: parent.top; margins: Theme.s3 }
        spacing: Theme.s3

        RowLayout {
            Layout.fillWidth: true
            spacing: Theme.s3
            Rectangle {
                Layout.preferredWidth: Theme.hMd; Layout.preferredHeight: Theme.hMd; radius: Theme.rSm
                color: root.st === "error" ? Theme.warnTint : Theme.raised
                Icon {
                    anchors.centerIn: parent; size: 18
                    name: root.st === "error" ? "warning" : root.st === "current" ? "check" : "download"
                    color: root.st === "error" ? Theme.amberText : Theme.textSecondary
                }
            }
            ColumnLayout {
                Layout.fillWidth: true
                spacing: 0
                Txt { Layout.fillWidth: true; text: qsTr("SirDank\u2019s Modpack"); font.weight: Font.Medium; wrapMode: Text.NoWrap; Accessible.role: Accessible.Heading }
                Txt {
                    Layout.fillWidth: true
                    text: root.status + (root.report && root.st !== "syncing" && root.st !== "error" ? " · " + root.reportSummary(root.report) : "")
                    kind: "caption"
                    color: root.st === "error" ? Theme.amberText : Theme.textSecondary
                    wrapMode: Text.NoWrap
                    elide: Text.ElideRight
                    HoverHandler { id: statusHover }
                    Tip { text: parent.text; wanted: statusHover.hovered && parent.truncated }
                }
            }
            SButton {
                visible: root.report !== null && root.st !== "syncing"
                variant: "ghost"
                size: "sm"
                text: root.open ? qsTr("Hide details") : qsTr("Details")
                Accessible.name: root.open ? qsTr("Hide what the last sync did") : qsTr("Show what the last sync did")
                onClicked: root.open = !root.open
            }
            SButton {
                visible: root.st === "syncing"
                text: qsTr("Cancel")
                Accessible.name: qsTr("Cancel the download of SirDank\u2019s Modpack")
                onClicked: ModsModel.cancelSync()
            }
            SButton {
                visible: root.st === "unknown" || root.st === "notinstalled" || root.st === "update"
                enabled: root.usable && !ModsModel.busy
                glyph: root.st === "update" ? "refresh" : "download"
                text: root.st === "update" ? qsTr("Update SirDank\u2019s Modpack") : qsTr("Install SirDank\u2019s Modpack")
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
            Layout.topMargin: -Theme.s1
            visible: root.st === "syncing"
            implicitHeight: Theme.s1; radius: Theme.rXs
            color: Theme.track
            Accessible.role: Accessible.ProgressBar
            Accessible.name: qsTr("SirDank\u2019s Modpack download progress")
            Rectangle {
                height: parent.height; radius: Theme.rXs
                color: Theme.accent
                width: ModsModel.packTotal > 0 ? parent.width * Math.max(0, ModsModel.packCurrent - 1) / ModsModel.packTotal : 0
                Behavior on width { NumberAnimation { duration: Motion.base } }
            }
        }

        ColumnLayout {   // what the last sync did
            Layout.fillWidth: true
            visible: root.open && root.report !== null && root.st !== "syncing"
            spacing: Theme.s1
            Divider {}
            Repeater {
                model: root.report ? root.report.failed : []
                delegate: RowLayout {
                    required property var modelData
                    Layout.fillWidth: true
                    Layout.topMargin: Theme.s1
                    spacing: Theme.s2
                    Icon { Layout.alignment: Qt.AlignTop; name: "error"; size: 14; color: Theme.danger }
                    Txt { Layout.fillWidth: true; text: modelData[0] + ": " + modelData[1]; kind: "caption"; color: Theme.danger }
                }
            }
            Repeater {
                model: root.report ? root.report.deleted : []
                delegate: Txt {
                    required property var modelData
                    Layout.fillWidth: true
                    text: qsTr("Removed %1 (dropped from SirDank\u2019s Modpack)").arg(modelData)
                    kind: "caption"
                }
            }
            Txt {
                Layout.fillWidth: true
                visible: root.report && root.report.failed.length === 0 && root.report.deleted.length === 0
                text: qsTr("Nothing needed attention.")
                kind: "caption"
            }
            SButton {
                Layout.alignment: Qt.AlignRight
                variant: "ghost"; size: "sm"
                text: qsTr("Dismiss")
                Accessible.name: qsTr("Dismiss the report on SirDank\u2019s Modpack")
                onClicked: { root.open = false; ModsModel.dismissReport() }
            }
        }
    }
}
