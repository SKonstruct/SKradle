import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Control {
    id: root
    required property int index
    required property string name
    required property string address
    required property string region
    required property string status
    required property int players
    required property int capacity
    required property int ping
    required property bool isDefault
    signal remove()
    signal makeDefault()

    hoverEnabled: true
    activeFocusOnTab: true
    focusPolicy: Qt.StrongFocus
    padding: Theme.s4
    leftPadding: Theme.s5
    rightPadding: Theme.s4
    property int radius: Theme.rLg
    readonly property bool online: status !== "offline"
    readonly property color statusColor: status === "online" ? Theme.ok : status === "busy" ? Theme.amber : Theme.textTertiary
    readonly property string statusText: status === "online" ? L.t("Online") : status === "busy" ? L.t("Busy") : L.t("Offline")
    readonly property int pingLevel: !online ? 0 : ping < 60 ? 4 : ping < 110 ? 3 : ping < 170 ? 2 : 1
    readonly property color pingColor: pingLevel >= 3 ? Theme.ok : pingLevel === 2 ? Theme.amber : Theme.danger

    Accessible.role: Accessible.ListItem
    Accessible.name: name + ", " + statusText + (online ? ", " + players + " players, " + ping + " milliseconds" : "")

    background: Card { raised: root.hovered; fill: root.hovered ? Theme.cardHover : Theme.card; radius: root.radius }

    contentItem: RowLayout {
        spacing: Theme.s4
        StatusDot { color: root.statusColor; pulse: root.status === "online"; Layout.alignment: Qt.AlignVCenter; Accessible.name: root.statusText }

        ColumnLayout {
            Layout.fillWidth: true
            Layout.preferredWidth: 0     // all rows share the same split of the free space
            Layout.minimumWidth: 160
            spacing: 2
            RowLayout {
                spacing: 8
                Txt {
                    id: nameTxt
                    Layout.fillWidth: true
                    Layout.maximumWidth: nameMetrics.advanceWidth + 2
                    text: root.name; kind: "heading"; maximumLineCount: 2; elide: Text.ElideRight
                    TextMetrics { id: nameMetrics; font: nameTxt.font; text: root.name }
                }
                Rectangle {
                    visible: root.isDefault
                    implicitWidth: defTxt.implicitWidth + 14; implicitHeight: 20; radius: 10
                    color: Theme.accentTint
                    Txt { id: defTxt; anchors.centerIn: parent; text: L.t("Default"); kind: "micro"; wrapMode: Text.NoWrap; color: Theme.text }
                }
            }
            Txt { Layout.fillWidth: true; text: root.address + "  ·  " + root.region; kind: "caption"; wrapMode: Text.NoWrap; elide: Text.ElideRight }
        }

        ColumnLayout {   // status + players
            Layout.preferredWidth: 100
            Layout.alignment: Qt.AlignVCenter
            spacing: 2
            Txt { Layout.fillWidth: true; text: root.statusText; kind: "caption"; color: root.statusColor; font.weight: Font.Medium; wrapMode: Text.NoWrap; elide: Text.ElideRight }
            Txt {
                Layout.fillWidth: true
                text: root.online ? L.t("%1 players").arg(root.players.toLocaleString(Qt.locale(), "f", 0)) : "—"
                kind: "caption"; wrapMode: Text.NoWrap; elide: Text.ElideRight
                font.features: { "tnum": 1 }
            }
        }

        RowLayout {   // ping
            Layout.preferredWidth: 74
            Layout.alignment: Qt.AlignVCenter
            spacing: 8
            SignalBars { level: root.pingLevel; color: root.pingColor; Layout.alignment: Qt.AlignVCenter }
            Txt { text: root.online ? root.ping + " ms" : "—"; kind: "caption"; wrapMode: Text.NoWrap; font.features: { "tnum": 1 } }
        }

        RowLayout {
            Layout.alignment: Qt.AlignVCenter
            spacing: 4
            SButton {
                text: L.t("Join")
                enabled: root.online
                Accessible.name: L.t("Join") + " " + root.name
                onClicked: Data.toast(L.t("Joining %1…").arg(root.name))
            }
            IconButton { id: moreBtn; glyph: "more"; tip: L.t("More actions"); onClicked: menu.popup(moreBtn, moreBtn.width - menu.width, moreBtn.height + 4) }
        }
    }

    function closeMenu() { menu.close() }
    function openMenu() { menu.popup(moreBtn, moreBtn.width - menu.width, moreBtn.height + 4) }
    SMenu {
        id: menu
        SMenuItem { text: L.t("Join"); enabled: root.online; onTriggered: Data.toast(L.t("Joining %1…").arg(root.name)) }
        SMenuItem { text: L.t("Set as default"); enabled: !root.isDefault; onTriggered: root.makeDefault() }
        SMenuItem { text: L.t("Copy address"); onTriggered: Data.toast(L.t("Address copied")) }
        SMenuSeparator {}
        SMenuItem { danger: true; text: L.t("Remove"); onTriggered: root.remove() }
    }
    TapHandler { acceptedButtons: Qt.RightButton; onTapped: menu.popup() }
    Keys.onMenuPressed: menu.popup(root, root.width / 2, root.height / 2)
    Keys.onReturnPressed: menu.popup(root, root.width / 2, root.height / 2)

    FocusRing {}
}
