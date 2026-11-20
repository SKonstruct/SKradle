import QtQuick
import com.skradle.app
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Layouts

// The player's mods folder: list, switches, tags found in the mods themselves, adding by drop or dialog, bulk actions
// on a selection, and the maintained modpack. Everything shown comes from ModsModel (the core's scan).
Item {
    id: page

    // ---- state ---------------------------------------------------------------------------------------------------
    // Three filters, and a mod must pass all of them: one status ("all", "enabled", "disabled" or "warning"), one
    // type (a tag id, "" for any) and the search text.
    property string status: "all"
    property string tag: ""
    property string query: ""
    property bool dropForced: false        // screenshots
    property bool shotEmpty: false         // screenshots: show the empty state without touching the folder
    readonly property alias scroller: scaffold.scroller
    readonly property bool gameOk: GameModel.state === "ready"
    readonly property string cmd: ModsText.cmdKey
    readonly property int tagsCol: 140
    readonly property int switchCol: 52

    readonly property var all: {
        if (shotEmpty) return []
        try { return JSON.parse(ModsModel.items) } catch (e) { return [] }
    }
    readonly property var tags: {
        if (shotEmpty) return []
        try { return JSON.parse(ModsModel.tags) } catch (e) { return [] }
    }
    // A warning filter or a type whose mods are all gone falls back to "any".
    readonly property int warnCount: all.filter(m => m.warning).length
    readonly property string activeStatus: status === "warning" && warnCount === 0 ? "all" : status
    readonly property string activeTag: tags.some(t => t.id === tag) ? tag : ""
    readonly property string needle: query.trim().toLowerCase()
    function passStatus(m, st) {
        return st === "all" || (st === "enabled" ? m.enabled : st === "disabled" ? !m.enabled : m.warning)
    }
    function passTag(m, t) { return t === "" || m.tags.indexOf(t) >= 0 }
    function passText(m) { return needle.length === 0 || m.search.indexOf(needle) >= 0 }
    readonly property var shown: all.filter(m => passStatus(m, activeStatus) && passTag(m, activeTag) && passText(m))
    // Chip counts say what the chip would show given the other two filters, so a count always matches the list.
    function statusCount(st) { return all.filter(m => passStatus(m, st) && passTag(m, activeTag) && passText(m)).length }
    function tagCount(t) { return all.filter(m => passStatus(m, activeStatus) && passTag(m, t) && passText(m)).length }
    readonly property bool filtering: activeStatus !== "all" || activeTag !== "" || needle.length > 0
    function clearFilters() { search.text = ""; status = "all"; tag = "" }
    readonly property int enabledCount: all.filter(m => m.enabled).length

    property var sel: ({})                 // file name -> true
    property int anchorIndex: -1
    property int cur: -1                   // the focused row (index into shown)
    property bool keyboardNav: false
    property var issues: []                // [{ file, text, bad }] of the last action
    readonly property var selectedFiles: shown.filter(m => sel[m.file] === true).map(m => m.file)
    readonly property bool busy: ModsModel.busy

    function setSearch(t) { search.text = t; query = t }
    function resetShot() {
        dropForced = false; shotEmpty = false; setSearch(""); status = "all"; tag = ""
        sel = ({}); cur = -1; issues = []; confirm.close(); keyboardNav = false
        ModsModel.forcePack("")
    }
    function shotState(name) {
        switch (name) {
        case "selected": sel = ({ [all[0].file]: true, [all[1].file]: true }); cur = 1; anchorIndex = 0; break
        case "focus": cur = 2; keyboardNav = true; rowsScope.forceActiveFocus(); break
        case "tag": tag = "textures"; break
        case "enabled": status = "enabled"; break
        case "disabled": status = "disabled"; break
        case "warning": status = "warning"; break
        case "combo": status = "enabled"; tag = "textures"; setSearch("s"); break
        case "long": setSearch("texture"); break
        case "scroll1": Qt.callLater(() => scroller.contentY = 900); break
        case "scroll2": Qt.callLater(() => scroller.contentY = scroller.contentHeight - scroller.height); break
        case "nomatch": status = "warning"; tag = "audio"; break
        case "empty": shotEmpty = true; break
        case "confirm": sel = ({ [all[0].file]: true }); askRemove(selectedFiles); break
        case "issues": issues = [{ file: "notes.txt", text: qsTr("not a .zip, .jar or .modpack file"), bad: true }, { file: "Sharp HUD.zip", text: qsTr("added as Sharp HUD (2).zip"), bad: false }]; break
        case "pack-update": ModsModel.forcePack("update"); break
        case "pack-syncing": ModsModel.forcePack("syncing"); break
        case "pack-report": ModsModel.forcePack("report"); break
        case "pack-error": ModsModel.forcePack("error"); break
        }
    }

    function load() { ModsModel.refresh(GameModel.gameVersion) }
    Component.onCompleted: {
        load()
        if (!AppEnv.selftest && AppEnv.shotDir === "" && gameOk) ModsModel.checkPack()
    }
    Connections { target: GameModel; function onGameVersionChanged() { page.load() } }

    // Keep the selection and the focused row valid whenever the list or the filter changes.
    onShownChanged: {
        const keep = {}
        for (const m of shown) if (sel[m.file] === true) keep[m.file] = true
        sel = keep
        if (cur >= shown.length) cur = shown.length - 1
        if (anchorIndex >= shown.length) anchorIndex = -1
    }

    // ---- selection -----------------------------------------------------------------------------------------------
    function selectRange(a, b, add) {
        const next = add ? Object.assign({}, sel) : {}
        for (let i = Math.min(a, b); i <= Math.max(a, b); ++i) next[shown[i].file] = true
        sel = next
    }
    function clickRow(i, mods) {
        keyboardNav = false
        const f = shown[i].file
        const multi = (mods & (Qt.ControlModifier | Qt.MetaModifier)) !== 0
        if ((mods & Qt.ShiftModifier) && anchorIndex >= 0) {
            selectRange(anchorIndex, i, multi)
        } else if (multi) {
            const next = Object.assign({}, sel)
            if (next[f]) delete next[f]; else next[f] = true
            sel = next
            anchorIndex = i
        } else {
            sel = ({ [f]: true })
            anchorIndex = i
        }
        cur = i
        rowsScope.forceActiveFocus(Qt.MouseFocusReason)
    }
    function selectAll() { selectRange(0, shown.length - 1, false); if (cur < 0) cur = 0 }
    function clearSelection() { sel = ({}); anchorIndex = -1 }
    function moveFocus(delta, extend) {
        if (shown.length === 0) return
        const next = Math.max(0, Math.min(shown.length - 1, (cur < 0 ? (delta > 0 ? -1 : shown.length) : cur) + delta))
        if (extend) {
            if (anchorIndex < 0) anchorIndex = cur < 0 ? next : cur
            selectRange(anchorIndex, next, false)
        }
        cur = next
        keyboardNav = true
        ensureVisible(next)
    }
    function ensureVisible(i) {
        const it = rows.itemAt(i)
        if (!it) return
        const y = it.mapToItem(scroller.contentItem, 0, 0).y
        if (y < scroller.contentY + Theme.s2) scroller.contentY = Math.max(0, y - Theme.s2)
        else if (y + it.height > scroller.contentY + scroller.height - Theme.s2)
            scroller.contentY = y + it.height - scroller.height + Theme.s2
    }
    // What an action applies to: the selection, else the focused row.
    function targets() {
        if (selectedFiles.length > 0) return selectedFiles
        return cur >= 0 && cur < shown.length ? [shown[cur].file] : []
    }

    // ---- actions -------------------------------------------------------------------------------------------------
    function setEnabled(files, on) {
        if (files.length === 0 || busy || !gameOk) return
        ModsModel.setEnabled(JSON.stringify(files), on)
    }
    function toggleTargets() {
        const t = targets()
        if (t.length === 0) return
        const anyOff = all.some(m => t.indexOf(m.file) >= 0 && !m.enabled)
        setEnabled(t, anyOff)
    }
    property var pendingRemove: []
    function askRemove(files) {
        if (files.length === 0 || busy || !gameOk) return
        pendingRemove = files
        confirm.open()
    }
    function addUrls(urls) {
        if (!gameOk || busy || urls.length === 0) return
        ModsModel.add(JSON.stringify(Array.from(urls, u => u.toString())))
    }
    function openFolder() {
        const url = ModsModel.ensureFolder()
        if (url === "" || !Qt.openUrlExternally(url)) Data.toast(qsTr("Could not open the mods folder."))
    }

    Connections {
        target: ModsModel
        function onResultChanged() {
            let r
            try { r = JSON.parse(ModsModel.result) } catch (e) { return }
            const out = []
            for (const p of r.renamed) out.push({ file: p[0], text: qsTr("added as %1 because that name was taken").arg(p[1]), bad: false })
            for (const f of r.failed) out.push({ file: f[0], text: r.kind === "add" ? qsTr("not added: %1").arg(f[1]) : f[1], bad: true })
            page.issues = out
            let msg
            switch (r.kind) {
            case "add":
                msg = r.ok > 0 ? qsTr("%n mod(s) added", "", r.ok) : qsTr("No mods were added")
                if (r.failed.length > 0 && r.ok > 0) msg += qsTr(", %n not added", "", r.failed.length)
                else if (r.failed.length > 0) msg += qsTr(" (%n file(s) refused)", "", r.failed.length)
                break
            case "enable": msg = qsTr("%n mod(s) enabled", "", r.ok); break
            case "disable": msg = qsTr("%n mod(s) disabled", "", r.ok); break
            default: msg = qsTr("%n mod(s) removed", "", r.ok)
            }
            if (r.kind !== "add" && r.failed.length > 0) msg += qsTr(", %n failed", "", r.failed.length)
            Data.toast(msg)
        }
    }

    // ---- the page ------------------------------------------------------------------------------------------------
    PageScaffold {
        id: scaffold
        anchors.fill: parent
        title: qsTr("Mods")
        subtitle: !page.gameOk ? qsTr("Mods are applied when the game starts.")
                  : page.all.length === 0 ? qsTr("Add mods, or install the modpack.")
                  : qsTr("%1 of %2 enabled").arg(page.enabledCount).arg(page.all.length)

        Card {   // no game
            Layout.fillWidth: true
            visible: !page.gameOk
            fill: Theme.amberTint
            implicitHeight: noGame.implicitHeight + 2 * Theme.s4
            RowLayout {
                id: noGame
                anchors { left: parent.left; right: parent.right; verticalCenter: parent.verticalCenter; margins: Theme.s4 }
                spacing: Theme.s3
                Icon { name: "warning"; size: 20; color: Theme.amberText }
                Txt { Layout.fillWidth: true; color: Theme.text
                      text: GameModel.state === "searching" ? qsTr("Looking for Spiral Knights…")
                            : qsTr("Spiral Knights was not found, so mods are switched off here. Set up the game on the Home page first.") }
            }
        }

        DropZone {
            Layout.fillWidth: true
            enabled: page.gameOk && !page.busy
            roomy: page.all.length === 0
            dragging: drops.containsDrag
            forceActive: page.dropForced
            onBrowse: fileDialog.open()
        }

        Card {   // what the last action did, when it needs a word
            Layout.fillWidth: true
            visible: page.issues.length > 0
            implicitHeight: issueCol.implicitHeight + 2 * Theme.s4
            ColumnLayout {
                id: issueCol
                anchors { left: parent.left; right: parent.right; top: parent.top; margins: Theme.s4 }
                spacing: Theme.s2
                RowLayout {
                    Layout.fillWidth: true
                    Txt { Layout.fillWidth: true; text: qsTr("Details"); font.weight: Font.Medium; Accessible.role: Accessible.Heading }
                    SButton { variant: "ghost"; text: qsTr("Dismiss"); Accessible.name: qsTr("Dismiss the details"); onClicked: page.issues = [] }
                }
                Repeater {
                    model: page.issues
                    delegate: Txt {
                        required property var modelData
                        Layout.fillWidth: true
                        text: modelData.file + ": " + modelData.text
                        kind: "caption"
                        color: modelData.bad ? Theme.danger : Theme.textSecondary
                    }
                }
            }
        }

        PackPanel {
            Layout.fillWidth: true
            usable: page.gameOk
            onCheckRequested: ModsModel.checkPack()
        }

        RowLayout {   // search and folder buttons
            Layout.fillWidth: true
            spacing: Theme.s3
            STextField {
                id: search
                glyph: "search"
                Layout.fillWidth: true
                Layout.maximumWidth: 340
                placeholderText: qsTr("Search mods")
                Accessible.name: qsTr("Search mods")
                onTextChanged: page.query = text
                Keys.onEscapePressed: (event) => { if (text.length > 0) text = ""; else event.accepted = false }
            }
            Item { Layout.fillWidth: true }
            SButton {
                glyph: "folder"
                text: qsTr("Open mods folder")
                onClicked: page.openFolder()
            }
            SButton {
                glyph: "plus"
                text: qsTr("Add mods…")
                enabled: page.gameOk && !page.busy
                onClicked: fileDialog.open()
            }
        }

        ColumnLayout {   // the filters and a line on how they combine
            Layout.fillWidth: true
            visible: page.all.length > 0
            spacing: Theme.s2
        Flow {   // filters: one status, one type; only types the installed mods have
            Layout.fillWidth: true
            visible: page.all.length > 0
            spacing: Theme.s2
            Chip {
                text: qsTr("All"); badge: page.statusCount("all")
                selected: page.activeStatus === "all"
                onClicked: page.status = "all"
            }
            Chip {
                text: qsTr("Enabled"); badge: page.statusCount("enabled")
                selected: page.activeStatus === "enabled"
                onClicked: page.status = "enabled"
            }
            Chip {
                text: qsTr("Disabled"); badge: page.statusCount("disabled")
                selected: page.activeStatus === "disabled"
                onClicked: page.status = "disabled"
            }
            Chip {
                visible: page.warnCount > 0
                text: qsTr("Warnings"); badge: page.statusCount("warning")
                selected: page.activeStatus === "warning"
                onClicked: page.status = "warning"
            }
            Rectangle {   // between the status chips and the type chips
                visible: page.tags.length > 0
                width: 1; height: 20
                color: Theme.border
            }
            Repeater {
                model: page.tags
                delegate: Chip {
                    required property var modelData
                    text: ModsText.tagLabel(modelData.id); badge: page.tagCount(modelData.id)
                    selected: page.activeTag === modelData.id
                    Accessible.description: selected ? qsTr("Selected. Click to show every type.") : ""
                    onClicked: page.tag = selected ? "" : modelData.id
                }
            }
        }

        Txt {   // how the filters combine, and what they leave
            Layout.fillWidth: true
            kind: "micro"
            color: Theme.textTertiary
            text: (page.filtering ? qsTr("Showing %1 of %2 mods. ").arg(page.shown.length).arg(page.all.length) : "")
                  + qsTr("Pick one status and one type. A mod must match both and the search to be listed.")
        }
        }

        ColumnLayout {   // bulk bar and hints
            Layout.fillWidth: true
            visible: page.all.length > 0
            spacing: Theme.s2
            RowLayout {
                Layout.fillWidth: true
                visible: page.selectedFiles.length > 0
                spacing: Theme.s2
                Txt { text: qsTr("%n selected", "", page.selectedFiles.length); font.weight: Font.Medium; wrapMode: Text.NoWrap
                      Accessible.role: Accessible.StatusBar }
                Item { Layout.fillWidth: true }
                SButton { text: qsTr("Enable"); enabled: page.gameOk && !page.busy; onClicked: page.setEnabled(page.selectedFiles, true)
                          Accessible.name: qsTr("Enable the selected mods") }
                SButton { text: qsTr("Disable"); enabled: page.gameOk && !page.busy; onClicked: page.setEnabled(page.selectedFiles, false)
                          Accessible.name: qsTr("Disable the selected mods") }
                SButton { text: qsTr("Remove"); variant: "danger"; glyph: "trash"; enabled: page.gameOk && !page.busy; onClicked: page.askRemove(page.selectedFiles)
                          Accessible.name: qsTr("Remove the selected mods") }
                SButton { text: qsTr("Clear"); variant: "ghost"; onClicked: page.clearSelection()
                          Accessible.name: qsTr("Clear the selection") }
            }
            Txt {
                Layout.fillWidth: true
                visible: page.shown.length > 0
                kind: "micro"
                color: Theme.textTertiary
                text: qsTr("Click selects. %1+click adds or removes one, Shift+click selects a range, %1+A selects all, Esc clears. Up and Down move, Space switches, Delete removes.").arg(page.cmd)
            }
        }

        Item {   // column headings, lined up with the rows
            Layout.fillWidth: true
            visible: page.shown.length > 0
            implicitHeight: headRow.implicitHeight
            RowLayout {
                id: headRow
                anchors { left: parent.left; right: parent.right; leftMargin: Theme.s4; rightMargin: Theme.s4 }
                spacing: Theme.s4
                Txt { Layout.fillWidth: true; text: qsTr("Mod"); kind: "micro"; color: Theme.textTertiary }
                Txt { Layout.preferredWidth: page.tagsCol; text: qsTr("Type"); kind: "micro"; color: Theme.textTertiary }
                Txt { Layout.preferredWidth: page.switchCol; horizontalAlignment: Text.AlignRight; text: qsTr("On"); kind: "micro"; color: Theme.textTertiary }
            }
        }

        FocusScope {   // the rows: one tab stop, arrow keys inside
            id: rowsScope
            Layout.fillWidth: true
            Layout.topMargin: -Theme.s4
            visible: page.shown.length > 0
            activeFocusOnTab: true
            implicitHeight: rowCol.implicitHeight
            Accessible.role: Accessible.List
            Accessible.name: qsTr("Installed mods")
            onActiveFocusChanged: if (activeFocus && page.cur < 0 && page.shown.length > 0) page.cur = 0

            Keys.onPressed: (event) => {
                if (event.matches(StandardKey.SelectAll)) { page.selectAll(); event.accepted = true; return }
                switch (event.key) {
                case Qt.Key_Up: page.moveFocus(-1, event.modifiers & Qt.ShiftModifier); event.accepted = true; break
                case Qt.Key_Down: page.moveFocus(1, event.modifiers & Qt.ShiftModifier); event.accepted = true; break
                case Qt.Key_Home: page.moveFocus(-page.shown.length, event.modifiers & Qt.ShiftModifier); event.accepted = true; break
                case Qt.Key_End: page.moveFocus(page.shown.length, event.modifiers & Qt.ShiftModifier); event.accepted = true; break
                case Qt.Key_Space: page.keyboardNav = true; page.toggleTargets(); event.accepted = true; break
                case Qt.Key_Delete:
                case Qt.Key_Backspace: page.askRemove(page.targets()); event.accepted = true; break
                case Qt.Key_Escape:
                    if (page.selectedFiles.length > 0) { page.clearSelection(); event.accepted = true }
                    break
                }
            }

            Column {
                id: rowCol
                width: parent.width
                spacing: Theme.s3
                Repeater {
                    id: rows
                    model: page.shown
                    delegate: ModRow {
                        required property var modelData
                        required property int index
                        width: rowCol.width
                        item: modelData
                        tagsWidth: page.tagsCol
                        switchWidth: page.switchCol
                        selected: page.sel[modelData.file] === true
                        canToggle: page.gameOk && !page.busy
                        showFocus: rowsScope.activeFocus && page.keyboardNav && page.cur === index
                        onClicked: (mods) => page.clickRow(index, mods)
                        onToggled: (on) => page.setEnabled([modelData.file], on)
                    }
                }
            }
        }

        ColumnLayout {   // search or filter with no result
            Layout.fillWidth: true
            Layout.topMargin: Theme.s4
            visible: page.all.length > 0 && page.shown.length === 0
            spacing: Theme.s2
            Icon { Layout.alignment: Qt.AlignHCenter; name: "search"; size: 32; color: Theme.textTertiary }
            Txt { Layout.fillWidth: true; horizontalAlignment: Text.AlignHCenter; text: qsTr("No mods match"); kind: "heading" }
            Txt { Layout.fillWidth: true; horizontalAlignment: Text.AlignHCenter; text: qsTr("Try a different search or filter."); color: Theme.textSecondary }
            SButton { Layout.alignment: Qt.AlignHCenter; text: qsTr("Show all mods"); onClicked: page.clearFilters() }
        }

        ColumnLayout {   // the folder could not be read
            Layout.fillWidth: true
            visible: ModsModel.error !== ""
            spacing: Theme.s2
            Txt { Layout.fillWidth: true; horizontalAlignment: Text.AlignHCenter; text: qsTr("The mods folder could not be read"); kind: "heading" }
            Txt { Layout.fillWidth: true; horizontalAlignment: Text.AlignHCenter; text: ModsModel.error; color: Theme.danger }
        }
    }

    // The whole page accepts a drop; the zone only shows it.
    DropArea {
        id: drops
        anchors.fill: parent
        enabled: page.gameOk && !page.busy
        onDropped: (drop) => page.addUrls(drop.urls)
    }

    FileDialog {
        id: fileDialog
        title: qsTr("Choose mod files")
        fileMode: FileDialog.OpenFiles
        nameFilters: [qsTr("Mod files (*.zip *.jar *.modpack)"), qsTr("All files (*)")]
        onAccepted: page.addUrls(selectedFiles)
    }

    ConfirmDialog {
        id: confirm
        title: qsTr("Remove %n mod(s)?", "", page.pendingRemove.length)
        message: (page.pendingRemove.length <= 4 ? page.pendingRemove.join("\n") + "\n\n" : "")
                 + qsTr("The files are deleted from the mods folder. This cannot be undone.")
        confirmText: qsTr("Remove")
        danger: true
        onConfirmed: ModsModel.remove(JSON.stringify(page.pendingRemove))
        onClosed: rowsScope.forceActiveFocus()
    }
}
