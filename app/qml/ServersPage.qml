import QtQuick
import com.skradle.app
import QtQuick.Controls
import QtQuick.Layouts

Item {
    id: page
    function openAdd() { dlg.open() }
    function closeAll() { dlg.close(); const it = lv.itemAtIndex(0); if (it) it.card.closeMenu() }
    function openMenu() { const it = lv.itemAtIndex(0); if (it) it.card.openMenu() }
    readonly property real cw: Math.min(Theme.pageMax, lv.width - 2 * Theme.s8)

    ListView {
        id: lv
        anchors.fill: parent
        model: Data.servers
        clip: true
        spacing: Theme.s3
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: SScrollBar {}
        topMargin: 0

        header: Item {
            width: lv.width
            height: head.implicitHeight + Theme.s8 + Theme.s6
            PageHeader {
                id: head
                x: (parent.width - page.cw) / 2; y: Theme.s8
                width: page.cw
                title: L.t("Servers")
                subtitle: L.t("Choose where to play. Players and ping update live.")
                SButton {
                    text: L.t("Add server")
                    highlighted: true
                    onClicked: dlg.open()
                    Accessible.name: text
                }
            }
        }
        footer: Item { width: lv.width; height: Theme.s10 }

        delegate: Item {
            id: del
            required property int index
            required property string name
            required property string address
            required property string region
            required property string status
            required property int players
            required property int capacity
            required property int ping
            required property bool isDefault
            width: lv.width
            height: card.implicitHeight
            property alias card: card
            ServerCard {
                id: card
                x: (parent.width - width) / 2
                width: page.cw
                index: del.index; name: del.name; address: del.address; region: del.region; status: del.status
                players: del.players; capacity: del.capacity; ping: del.ping; isDefault: del.isDefault
                onRemove: Data.servers.remove(del.index)
                onMakeDefault: {
                    for (let i = 0; i < Data.servers.count; ++i) Data.servers.setProperty(i, "isDefault", i === del.index)
                    Data.toast(L.t("Default server changed"))
                }
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

    AddServerDialog {
        id: dlg
        onAccepted: (name, address) => {
            Data.servers.insert(0, { name: name, address: address, region: L.t("Custom"), status: "online",
                                     players: 0, capacity: 100, ping: 52, isDefault: false })
            Data.toast(L.t("Server added"))
        }
    }
}
