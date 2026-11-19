import QtQuick
import com.skradle.app
import QtQuick.Controls
import QtQuick.Layouts

Item {
    id: page
    property string filter: "all"
    property var all: Data.mods.map(m => Object.assign({}, m))
    property int enabledCount: 0
    property bool dropForced: false
    property string query: ""
    readonly property real cw: Math.min(Theme.pageMax, lv.width - 2 * Theme.s8)

    function matches(m) {
        const q = query.trim().toLowerCase()
        if (filter === "enabled" && !m.on) return false
        if (filter === "disabled" && m.on) return false
        if (q.length && (m.name + " " + m.desc + " " + m.tag).toLowerCase().indexOf(q) < 0) return false
        return true
    }
    // Diff the visible model instead of resetting it, so ListView add/remove/displaced transitions run.
    function apply() {
        let pos = 0
        for (let i = 0; i < all.length; ++i) {
            const m = all[i]
            const present = pos < vm.count && vm.get(pos).mid === m.id
            const want = matches(m)
            if (want && !present) vm.insert(pos, { mid: m.id, name: m.name, tag: m.tag, desc: m.desc, ver: m.ver, enabledState: m.on })
            else if (!want && present) { vm.remove(pos); continue }
            if (want) pos++
        }
        let c = 0
        for (const m of all) if (m.on) c++
        enabledCount = c
    }
    function setOn(id, on) {
        for (let i = 0; i < all.length; ++i) if (all[i].id === id) all[i].on = on
        for (let j = 0; j < vm.count; ++j) if (vm.get(j).mid === id) vm.setProperty(j, "enabledState", on)
        let c = 0
        for (const m of all) if (m.on) c++
        enabledCount = c
        if (filter !== "all") settle.restart()   // let the row finish its animation, then drop it from the filter
    }
    function setSearch(t) { if (lv.headerItem) lv.headerItem.searchField.text = t; query = t }
    Timer { id: settle; interval: 650; onTriggered: page.apply() }
    Component.onCompleted: { apply(); Qt.callLater(() => ready = true) }
    onFilterChanged: apply()
    ListModel { id: vm }
    property bool ready: false   // no add-transition for the initial fill (the page is still invisible then)
    Transition {
        id: addT
            ParallelAnimation {
                NumberAnimation { property: "opacity"; from: 0; to: 1; duration: Motion.slow; easing.type: Easing.BezierSpline; easing.bezierCurve: Motion.decel }
                // Not "y": `from` is an absolute position in the list, which sent new rows flying down from the header.
                NumberAnimation { property: "scale"; from: 0.97; to: 1; duration: Motion.slow; easing.type: Easing.BezierSpline; easing.bezierCurve: Motion.decel }
            }
    }


    ListView {
        id: lv
        anchors.fill: parent
        model: vm
        clip: true
        spacing: Theme.s3
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: SScrollBar {}

        header: Item {
            property alias searchField: search
            width: lv.width
            height: hcol.implicitHeight + Theme.s8 + Theme.s5
            ColumnLayout {
                id: hcol
                x: (parent.width - page.cw) / 2; y: Theme.s8
                width: page.cw
                spacing: Theme.s5
                PageHeader {
                    Layout.fillWidth: true
                    title: qsTr("Mods")
                    subtitle: qsTr("%1 of %2 enabled").arg(page.enabledCount).arg(page.all.length)
                }
                DropZone { forceActive: page.dropForced; Layout.fillWidth: true; onDropped: (n) => Data.toast(qsTr("%1 files added").arg(n)) }
                RowLayout {
                    Layout.fillWidth: true
                    spacing: Theme.s3
                    STextField {
                        id: search
                        glyph: "search"
                        Layout.fillWidth: true
                        Layout.maximumWidth: 340
                        placeholderText: qsTr("Search mods")
                        Accessible.name: qsTr("Search mods")
                        onTextChanged: { page.query = text; page.apply() }
                    }
                    Item { Layout.fillWidth: true }
                    RowLayout {
                        spacing: Theme.s2
                        Chip { text: qsTr("All"); badge: page.all.length; selected: page.filter === "all"; onClicked: page.filter = "all" }
                        Chip { text: qsTr("Enabled"); badge: page.enabledCount; selected: page.filter === "enabled"; onClicked: page.filter = "enabled" }
                        Chip { text: qsTr("Disabled"); badge: page.all.length - page.enabledCount; selected: page.filter === "disabled"; onClicked: page.filter = "disabled" }
                    }
                }
            }
        }
        footer: Item {
            width: lv.width
            height: vm.count === 0 ? 190 : Theme.s10
            ColumnLayout {
                visible: vm.count === 0
                x: (parent.width - width) / 2
                y: 24
                width: Math.min(360, parent.width - 64)
                spacing: Theme.s2
                Icon { Layout.alignment: Qt.AlignHCenter; name: "search"; size: 32; color: Theme.textTertiary }
                Txt { Layout.fillWidth: true; horizontalAlignment: Text.AlignHCenter; text: qsTr("No mods match"); kind: "heading" }
                Txt { Layout.fillWidth: true; horizontalAlignment: Text.AlignHCenter; text: qsTr("Try a different search or filter."); color: Theme.textSecondary }
            }
        }

        delegate: Item {
            id: del
            required property int index
            required property int mid
            required property string name
            required property string tag
            required property string desc
            required property string ver
            required property bool enabledState
            width: lv.width
            height: row.implicitHeight
            ModRow {
                id: row
                x: (parent.width - width) / 2
                width: page.cw
                index: del.index; mid: del.mid; name: del.name; tag: del.tag; desc: del.desc; ver: del.ver
                enabledState: del.enabledState
                onToggled: (on) => page.setOn(del.mid, on)
            }
        }

        add: page.ready ? addT : null
        remove: Transition {
            NumberAnimation { property: "opacity"; to: 0; duration: Motion.fast; easing.type: Easing.BezierSpline; easing.bezierCurve: Motion.accel }
        }
        displaced: Transition {
            NumberAnimation { property: "y"; duration: Motion.slow; easing.type: Easing.BezierSpline; easing.bezierCurve: Motion.standard }
        }
    }
}
