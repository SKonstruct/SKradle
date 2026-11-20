import QtQuick
import com.skradle.app
import QtQuick.Controls
import QtQuick.Layouts

Item {
    id: page
    property bool showHost: false
    // For the self-test: what the web interface panel ended up showing.
    readonly property string hostMode: panel.mode
    readonly property string hostTitle: panel.pageTitle
    readonly property int pageProgress: panel.pageProgress
    function openAdd() { dlg.openAdd(lv) }
    function closeAll() {
        page.showHost = false
        dlg.close(); infoDlg.close(); deleteDlg.close()
        if (lv.footerItem) lv.footerItem.stripItem.closeDialogs()
        const it = lv.itemAtIndex(0); if (it) it.card.closeMenu()
    }
    // Screenshots only: a named state of the page.
    function shot(name) {
        closeAll()
        const strip = lv.footerItem ? lv.footerItem.stripItem : null
        strip.manage = false
        lv.positionViewAtBeginning()
        switch (name) {
        case "add": dlg.openAdd(lv); break
        case "add-errors": dlg.openDemo(lv); break
        case "edit": dlg.openEdit("a", lv); break
        case "info": infoDlg.show(rows.get(1), lv); break
        case "delete": deleteDlg.sid = "c"; deleteDlg.name = rows.get(3).name; deleteDlg.openFrom(lv); break
        case "install": ServerHost.force("notinstalled"); lv.positionViewAtEnd(); strip.demo("install"); break
        case "backup": ServerHost.force("stopped"); lv.positionViewAtEnd(); strip.demo("backup"); break
        case "selected-menu": { const it = lv.itemAtIndex(1); if (it) it.card.openMenu(); break }
        case "manage": ServerHost.force("running"); lv.positionViewAtEnd(); strip.manage = true; break
        default: ServerHost.force(name); lv.positionViewAtEnd(); strip.manage = false
        }
    }
    function openMenu() { const it = lv.itemAtIndex(0); if (it) it.card.openMenu() }
    readonly property real cw: Math.min(Theme.pageMax, lv.width - 2 * Theme.s8)
    readonly property bool active: visible && !!Window.window && Window.window.visible && !showHost

    // The probe results, by server id: {state: "checking" | "up" | "down", ms}.
    readonly property var probes: { try { return JSON.parse(ServerBook.probes) } catch (e) { return ({}) } }
    readonly property bool officialDown: probes["official"] !== undefined && probes["official"].state === "down"

    ClipboardHelper { id: clip }

    // The rows, kept in step with the stored list by id: a changed probe never touches this model, and a changed
    // list only changes the rows that differ, so an open menu or a focused button stays where it is.
    ListModel { id: rows }
    function syncRows() {
        let list = []
        try { list = JSON.parse(ServerBook.entries) } catch (e) { list = [] }
        for (let i = rows.count - 1; i >= 0; --i)
            if (!list.some(e => e.id === rows.get(i).sid)) rows.remove(i)
        for (let i = 0; i < list.length; ++i) {
            const e = list[i]
            const item = { sid: e.id, name: e.name, host: e.host, port: e.port, address: e.address, kind: e.kind,
                           selected: e.selected, publicKey: e.publicKey, getdownUrl: e.getdownUrl, contentUrl: e.contentUrl }
            let at = -1
            for (let j = i; j < rows.count; ++j) if (rows.get(j).sid === e.id) { at = j; break }
            if (at < 0) { rows.insert(i, item); continue }
            if (at !== i) rows.move(at, i, 1)
            for (const k in item) if (rows.get(i)[k] !== item[k]) rows.setProperty(i, k, item[k])
        }
    }
    Component.onCompleted: syncRows()
    Connections { target: ServerBook; function onEntriesChanged() { page.syncRows() } }
    // The hosted server registers itself in the list when it has a key: read the list again then.
    property int seenRevision: 0
    Connections {
        target: ServerHost
        function onStatusChanged() {
            let r = 0
            try { r = JSON.parse(ServerHost.status).revision } catch (e) { r = 0 }
            if (r !== page.seenRevision) { page.seenRevision = r; ServerBook.reload(); ServerBook.probeAll() }
        }
    }

    // Probe while the page is showing, and again when the player asks; never in the background.
    Timer {
        interval: 15000; repeat: true
        running: page.active; triggeredOnStart: true
        onTriggered: ServerBook.probeAll()
    }
    onActiveChanged: if (active) ServerBook.reload()

    function toastError() { if (ServerBook.error !== "") Data.toast(qsTr("The list could not be saved. Check that the settings folder is writable."), "error") }
    Connections { target: ServerBook; function onErrorChanged() { page.toastError() } }

    ListView {
        id: lv
        visible: !page.showHost
        anchors.fill: parent
        model: rows
        clip: true
        spacing: Theme.s2
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: SScrollBar {}
        topMargin: 0
        // Every row has the same height, so a refresh of one cannot move the others.
        reuseItems: false

        header: Item {
            width: lv.width
            height: head.implicitHeight + sectionHead.implicitHeight + Theme.s8 + Theme.s5 + Theme.s3
            PageHeader {
                id: head
                x: (parent.width - page.cw) / 2; y: Theme.s8
                width: page.cw
                title: qsTr("Servers")
                subtitle: qsTr("Choose the server the game connects to when you launch it.")
                SButton {
                    text: qsTr("Check again"); glyph: "refresh"; variant: "ghost"
                    onClicked: ServerBook.probeAll()
                    Accessible.name: qsTr("Check which servers can be reached again")
                }
                SButton {
                    id: addBtn
                    text: qsTr("Add server"); glyph: "plus"
                    onClicked: dlg.openAdd(addBtn)
                }
            }
            ColumnLayout {
                id: sectionHead
                x: (parent.width - page.cw) / 2
                y: head.y + head.implicitHeight + Theme.s5
                width: page.cw
                spacing: Theme.gapStack
                Txt { Layout.fillWidth: true; text: qsTr("Play on"); kind: "heading"; Accessible.role: Accessible.Heading }
                Banner {
                    visible: page.officialDown
                    kind: "warn"
                    text: qsTr("The official server can't be reached. Check your internet connection. Servers on your own network may still work.")
                    actionText: qsTr("Check again")
                    onAction: ServerBook.probeAll()
                }
            }
        }
        // The hosting summary comes after the list: choosing where to play is the first job.
        footer: Item {
            width: lv.width
            property alias stripItem: strip
            height: strip.implicitHeight + Theme.s6 + Theme.s10
            ServerControlStrip {
                id: strip
                x: (parent.width - page.cw) / 2
                y: Theme.s6
                width: page.cw
                onOpenWebUi: page.showHost = true
            }
        }

        delegate: Item {
            id: del
            required property int index
            required property string sid
            required property string name
            required property string address
            required property string kind
            required property bool selected
            width: lv.width
            height: card.implicitHeight
            property alias card: card
            ServerCard {
                id: card
                x: (parent.width - width) / 2
                width: page.cw
                sid: del.sid; name: del.name; address: del.address; kind: del.kind; selected: del.selected
                probeState: page.probes[del.sid] ? page.probes[del.sid].state : ""
                ping: page.probes[del.sid] ? page.probes[del.sid].ms : 0
                onSelect: { ServerBook.select(del.sid); Data.toast(qsTr("%1 will be used when you launch").arg(del.name)) }
                onEdit: dlg.openEdit(del.sid, card.moreButton)
                onRemove: { deleteDlg.sid = del.sid; deleteDlg.name = del.name; deleteDlg.openFrom(card.moreButton) }
                onInfo: infoDlg.show(rows.get(del.index), card.moreButton)
                onCopyAddress: { clip.copy(del.address); Data.toast(qsTr("Address copied")) }
            }
        }

        add: Transition {
            ParallelAnimation {
                NumberAnimation { property: "opacity"; from: 0; to: 1; duration: Motion.slow; easing.type: Easing.BezierSpline; easing.bezierCurve: Motion.decel }
                NumberAnimation { property: "scale"; from: 0.97; to: 1; duration: Motion.slow; easing.type: Easing.BezierSpline; easing.bezierCurve: Motion.decel }
            }
        }
        remove: Transition {
            ParallelAnimation {
                NumberAnimation { property: "opacity"; to: 0; duration: Motion.base; easing.type: Easing.BezierSpline; easing.bezierCurve: Motion.accel }
                NumberAnimation { property: "scale"; to: 0.97; duration: Motion.base }
            }
        }
        displaced: Transition {
            NumberAnimation { property: "y"; duration: Motion.slow; easing.type: Easing.BezierSpline; easing.bezierCurve: Motion.standard }
        }
    }

    MyServerPanel {
        id: panel
        anchors.fill: parent
        anchors.leftMargin: Theme.s8; anchors.rightMargin: Theme.s8
        anchors.topMargin: Theme.s8; anchors.bottomMargin: Theme.s8
        visible: page.showHost
        onBack: page.showHost = false
    }

    AddServerDialog {
        id: dlg
        fallback: lv
        onSaved: ServerBook.probeAll()
    }
    ServerInfoDialog { id: infoDlg; fallback: lv }
    ConfirmDialog {
        id: deleteDlg
        property string sid: ""
        property string name: ""
        fallback: lv
        title: qsTr("Delete this server?")
        message: qsTr("“%1” is removed from your list. If it is the selected server, the game will use Official again.").arg(name)
        confirmText: qsTr("Delete")
        danger: true
        onConfirmed: { ServerBook.remove(sid); Data.toast(qsTr("Server deleted")) }
    }
}
