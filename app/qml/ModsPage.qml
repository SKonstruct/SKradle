import QtQuick
import com.skradle.app
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Layouts

// The player's mods folder: list, switches, tags found in the mods themselves, adding by drop or dialog, bulk actions
// on a selection, and SirDank's Modpack. Everything shown comes from ModsModel (the core's scan).
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
    readonly property int tagsCol: width < 760 ? 132 : 148

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
    // The order can be changed while nothing is filtered (positions in the shown list are positions in the whole one).
    readonly property bool canReorder: !filtering && all.length > 1 && gameOk && !busy
    function moveMod(index, steps) {
        const to = Math.max(0, Math.min(all.length - 1, index + steps))
        if (to !== index && index >= 0 && index < shown.length) ModsModel.moveMod(shown[index].file, to)
    }
    readonly property bool filtering: activeStatus !== "all" || activeTag !== "" || needle.length > 0
    // What the two filters offer: the status segments (Warnings only when there are some) and the types.
    readonly property var statusModel: {
        const m = [{ id: "all", label: qsTr("All"), badge: String(statusCount("all")) },
                   { id: "enabled", label: qsTr("Enabled"), badge: String(statusCount("enabled")) },
                   { id: "disabled", label: qsTr("Disabled"), badge: String(statusCount("disabled")) }]
        if (warnCount > 0) m.push({ id: "warning", label: qsTr("Warnings"), badge: String(statusCount("warning")) })
        return m
    }
    readonly property var typeModel: {
        const m = [{ id: "", label: qsTr("All types") }]
        for (const t of tags) m.push({ id: t.id, label: ModsText.tagLabel(t.id) + " (" + tagCount(t.id) + ")" })
        return m
    }
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
        help.close()
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
        case "help": Qt.callLater(() => help.open()); break
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
                  : page.all.length === 0 ? qsTr("Add mods, or install SirDank\u2019s Modpack.")
                  : page.filtering ? qsTr("Showing %1 of %2 mods").arg(page.shown.length).arg(page.all.length)
                  : qsTr("%1 of %2 enabled").arg(page.enabledCount).arg(page.all.length)
        actions: [
            PackButton {
                id: packBtn
                visible: page.gameOk || page.all.length > 0
                usable: page.gameOk
                onCheckRequested: ModsModel.checkPack()
            },
            SButton {
                visible: page.all.length > 0
                glyph: "folder"
                text: qsTr("Open folder")
                Accessible.name: qsTr("Open the mods folder")
                onClicked: page.openFolder()
            },
            SButton {
                visible: page.all.length > 0
                variant: "primary"
                glyph: "plus"
                text: qsTr("Add mods\u2026")
                enabled: page.gameOk && !page.busy
                onClicked: fileDialog.open()
            }
        ]

        ColumnLayout {
            Layout.fillWidth: true
            spacing: Theme.s3

            Banner {   // no game, but there are mods to look at
                visible: !page.gameOk && page.all.length > 0
                kind: "warn"
                text: GameModel.state === "searching" ? qsTr("Looking for Spiral Knights\u2026")
                      : qsTr("Spiral Knights was not found, so mods are switched off here. Set up the game on the Home page first.")
                actionText: GameModel.state === "searching" ? "" : qsTr("Open Home")
                onAction: Data.navigate(0)
            }

            // What the last modpack sync did, or why it failed: only when there is something to say.
            Banner {
                id: packNote
                readonly property var report: { try { return ModsModel.packReport === "" ? null : JSON.parse(ModsModel.packReport) } catch (e) { return null } }
                readonly property bool failed: ModsModel.packState === "error"
                readonly property bool bad: failed || (report !== null && report.failed.length > 0)
                visible: (failed || (report !== null && ModsModel.packState !== "syncing")) && (page.gameOk || page.all.length > 0)
                kind: bad ? "warn" : "info"
                title: qsTr("SirDank\u2019s Modpack")
                text: {
                    if (failed) return ModsModel.packMessage
                    if (!report) return ""
                    const parts = [qsTr("%n downloaded", "", report.downloaded.length)]
                    if (report.skipped > 0) parts.push(qsTr("%n already current", "", report.skipped))
                    if (report.deleted.length > 0) parts.push(qsTr("%n removed", "", report.deleted.length))
                    if (report.failed.length > 0) parts.push(qsTr("%n failed", "", report.failed.length))
                    const lines = [(report.cancelled ? qsTr("Stopped. ") : "") + parts.join(", ")]
                    for (const f of report.failed) lines.push(f[0] + ": " + f[1])
                    for (const d of report.deleted) lines.push(qsTr("Removed %1 (dropped from SirDank\u2019s Modpack)").arg(d))
                    return lines.join("\n")
                }
                dismissible: report !== null && !failed
                onDismissed: ModsModel.dismissReport()
            }

            Banner {   // what the last action did, when it needs a word
                visible: page.issues.length > 0
                kind: page.issues.some(i => i.bad) ? "warn" : "info"
                title: page.issues.some(i => i.bad) ? qsTr("Some files need attention") : qsTr("Done")
                text: page.issues.map(i => i.file + ": " + i.text).join("\n")
                dismissible: true
                onDismissed: page.issues = []
            }

            Item {   // search, or the bulk bar in its place (the list never moves)
                Layout.fillWidth: true
                visible: page.all.length > 0
                implicitHeight: Theme.hMd
                RowLayout {
                    anchors.fill: parent
                    visible: page.selectedFiles.length === 0
                    spacing: Theme.s2
                    STextField {
                        id: search
                        glyph: "search"
                        Layout.fillWidth: true
                        Layout.maximumWidth: 360
                        placeholderText: qsTr("Search mods")
                        label: qsTr("Search mods")
                        onTextChanged: page.query = text
                        Keys.onEscapePressed: (event) => { if (text.length > 0) text = ""; else event.accepted = false }
                    }
                    Item { Layout.fillWidth: true }
                    IconButton {
                        id: helpBtn
                        glyph: "question"
                        tip: qsTr("Keyboard shortcuts")
                        onClicked: help.opened ? help.close() : help.open()
                    }
                }
                SelectionBar {
                    anchors.fill: parent
                    visible: page.selectedFiles.length > 0
                    count: page.selectedFiles.length
                    canAct: page.gameOk && !page.busy
                    onEnable: page.setEnabled(page.selectedFiles, true)
                    onDisable: page.setEnabled(page.selectedFiles, false)
                    onRemove: page.askRemove(page.selectedFiles)
                    onClear: page.clearSelection()
                }
            }

            ModFilters {
                Layout.fillWidth: true
                Layout.topMargin: Theme.gapSection - Theme.s3   // the page's section spacing above the filter row
                visible: page.all.length > 0
                statusModel: page.statusModel
                typeModel: page.typeModel
                status: page.activeStatus
                tag: page.activeTag
                onStatusPicked: (id) => page.status = id
                onTagPicked: (id) => page.tag = id
            }

            Txt {
                Layout.fillWidth: true
                visible: page.all.length > 1
                kind: "caption"
                color: Theme.textSecondary
                text: page.filtering ? qsTr("Clear the search and filters to change the order.")
                    : qsTr("Drag the handle (or press Alt with the arrow keys) to change the order. A mod lower in the list wins when two change the same file.")
            }

            FocusScope {   // the rows: one tab stop, arrow keys inside
                id: rowsScope
                Layout.fillWidth: true
                visible: page.shown.length > 0
                activeFocusOnTab: true
                implicitHeight: rowCol.implicitHeight
                Accessible.role: Accessible.List
                Accessible.name: qsTr("Installed mods")
                onActiveFocusChanged: if (activeFocus && page.cur < 0 && page.shown.length > 0) page.cur = 0

                Keys.onPressed: (event) => {
                    if (event.matches(StandardKey.SelectAll)) { page.selectAll(); event.accepted = true; return }
                    switch (event.key) {
                    case Qt.Key_Up:
                        if ((event.modifiers & Qt.AltModifier) && page.canReorder && page.cur >= 0) { page.moveMod(page.cur, -1); page.cur = Math.max(0, page.cur - 1) }
                        else page.moveFocus(-1, event.modifiers & Qt.ShiftModifier)
                        event.accepted = true; break
                    case Qt.Key_Down:
                        if ((event.modifiers & Qt.AltModifier) && page.canReorder && page.cur >= 0) { page.moveMod(page.cur, 1); page.cur = Math.min(page.shown.length - 1, page.cur + 1) }
                        else page.moveFocus(1, event.modifiers & Qt.ShiftModifier)
                        event.accepted = true; break
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
                    spacing: Theme.s2
                    Repeater {
                        id: rows
                        model: page.shown
                        delegate: ModRow {
                            required property var modelData
                            required property int index
                            width: rowCol.width
                            item: modelData
                            tagsWidth: page.tagsCol
                            selected: page.sel[modelData.file] === true
                            canToggle: page.gameOk && !page.busy
                            reorderable: page.canReorder
                            rowStep: height + rowCol.spacing
                            onMoveRequested: (steps) => page.moveMod(index, steps)
                            showFocus: rowsScope.activeFocus && page.keyboardNav && page.cur === index
                            onClicked: (mods) => page.clickRow(index, mods)
                            onToggled: (on) => page.setEnabled([modelData.file], on)
                        }
                    }
                }
            }

            EmptyState {   // search or filter with no result
                visible: page.all.length > 0 && page.shown.length === 0
                glyph: "search"
                title: qsTr("No mods match")
                text: qsTr("Try a different search or filter.")
                actionText: qsTr("Show all mods")
                onAction: page.clearFilters()
            }
            EmptyState {   // nothing in the folder
                visible: page.all.length === 0 && ModsModel.error === "" && page.gameOk
                glyph: "mods"
                title: qsTr("No mods yet")
                text: qsTr("Drop .zip, .jar or .modpack files anywhere on this page, or choose them. Each file is checked before it is copied in. You can also install SirDank\u2019s Modpack above.")
                actionText: qsTr("Add mods\u2026")
                actionGlyph: "plus"
                onAction: fileDialog.open()
            }
            EmptyState {   // no game
                visible: page.all.length === 0 && ModsModel.error === "" && !page.gameOk
                glyph: "warning"
                title: GameModel.state === "searching" ? qsTr("Looking for Spiral Knights\u2026") : qsTr("Spiral Knights was not found")
                text: qsTr("Mods are applied when the game starts, so the game has to be set up first.")
                actionText: GameModel.state === "searching" ? "" : qsTr("Open Home")
                onAction: Data.navigate(0)
            }
            EmptyState {   // the folder could not be read
                visible: ModsModel.error !== ""
                glyph: "warning"
                title: qsTr("The mods folder could not be read")
                text: ModsModel.error
                actionText: qsTr("Try again")
                onAction: page.load()
            }
        }
    }

    // The whole page accepts a drop; the zone only shows it.
    DropArea {
        id: drops
        anchors.fill: parent
        enabled: page.gameOk && !page.busy
        onDropped: (drop) => page.addUrls(drop.urls)
    }
    DropZone {
        anchors.fill: parent
        anchors.margins: Theme.s4
        enabled: page.gameOk && !page.busy
        dragging: drops.containsDrag
        forceActive: page.dropForced
    }

    Popup {   // keyboard help, opened by the "?" button
        id: help
        parent: helpBtn
        x: helpBtn.width - width
        y: helpBtn.height + Theme.s1
        width: 320
        padding: Theme.s4
        modal: false
        closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
        background: Card { fill: Theme.overlay; elevation: 2; radius: Theme.rMd }
        contentItem: ColumnLayout {
            spacing: Theme.s2
            Txt { text: qsTr("Selecting and switching"); kind: "heading"; Accessible.role: Accessible.Heading }
            Repeater {
                model: [[qsTr("Click"), qsTr("Select one mod")],
                        [page.cmd + " + " + qsTr("click"), qsTr("Add or remove one")],
                        [qsTr("Shift + click"), qsTr("Select a range")],
                        [page.cmd + " + A", qsTr("Select all")],
                        [qsTr("Up, Down"), qsTr("Move")],
                        [qsTr("Space"), qsTr("Switch on or off")],
                        [qsTr("Delete"), qsTr("Remove")],
                        [qsTr("Esc"), qsTr("Clear the selection")]]
                delegate: RowLayout {
                    required property var modelData
                    Layout.fillWidth: true
                    spacing: Theme.s3
                    Txt { Layout.preferredWidth: 112; text: modelData[0]; font.weight: Font.Medium; wrapMode: Text.NoWrap }
                    Txt { Layout.fillWidth: true; text: modelData[1]; color: Theme.textSecondary }
                }
            }
        }
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
