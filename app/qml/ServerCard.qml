import QtQuick
import com.skradle.app
import QtQuick.Controls
import QtQuick.Layouts

// One row of the server list. Columns are fixed so every row lines up: choice, name and address, reachability,
// ping, the Select action, more actions. The selected server is marked three ways: a filled choice circle, a
// tinted card with an accent edge, and the "Selected" label where the Select button would be.
Control {
    id: root
    required property string sid
    required property string name
    required property string address
    required property string kind        // official | saved | local
    required property bool selected
    // "" (not probed yet), "checking", "up" or "down"
    property string probeState: ""
    property int ping: 0
    signal select()
    signal edit()
    signal remove()
    signal info()
    signal copyAddress()

    hoverEnabled: true
    padding: Theme.s4
    leftPadding: Theme.s5
    rightPadding: Theme.s4
    property int radius: Theme.rLg
    readonly property bool editable: kind === "saved"
    // The hosted server that is not running cannot answer: say that instead of "unreachable".
    readonly property string live: kind === "local" && ServerHost.state !== "running" ? "stopped" : probeState
    readonly property color statusColor: live === "up" ? Theme.ok : live === "down" ? Theme.danger : Theme.textTertiary
    readonly property string statusText: live === "up" ? qsTr("Reachable") : live === "down" ? qsTr("Unreachable")
        : live === "stopped" ? qsTr("Not running") : live === "checking" ? qsTr("Checking…") : "—"
    readonly property int pingLevel: live !== "up" ? 0 : ping < 60 ? 4 : ping < 110 ? 3 : ping < 170 ? 2 : 1
    readonly property color pingColor: pingLevel >= 3 ? Theme.ok : pingLevel === 2 ? Theme.amber : Theme.danger

    Accessible.role: Accessible.ListItem
    Accessible.name: name + ", " + address + ", " + statusText + (live === "up" ? ", " + ping + " " + qsTr("milliseconds") : "")
                     + (selected ? ", " + qsTr("selected") : "")

    background: Item {
        Card {
            anchors.fill: parent
            raised: root.hovered
            fill: root.selected ? Theme.accentTint : root.hovered ? Theme.cardHover : Theme.card
            radius: root.radius
        }
        Rectangle {   // the selected row's edge
            visible: root.selected
            anchors.fill: parent
            radius: root.radius
            color: "transparent"
            border.width: 2
            border.color: Theme.accentFg
        }
    }

    contentItem: RowLayout {
        spacing: Theme.s4

        // Choice circle: filled and ticked when this is the server the game will use.
        Item {
            Layout.preferredWidth: 22; Layout.preferredHeight: 22
            Layout.alignment: Qt.AlignVCenter
            Rectangle {
                anchors.fill: parent; radius: 11
                color: root.selected ? Theme.accent : "transparent"
                border.width: root.selected ? 0 : 2
                border.color: Theme.fieldBorder
                Icon { anchors.centerIn: parent; name: "check"; size: 14; stroke: 2.6; color: Theme.accentText; visible: root.selected }
            }
        }

        ColumnLayout {
            Layout.fillWidth: true
            Layout.preferredWidth: 0
            Layout.minimumWidth: 140
            spacing: 2
            RowLayout {
                spacing: 8
                Txt {
                    id: nameTxt
                    Layout.fillWidth: true
                    Layout.maximumWidth: nameMetrics.advanceWidth + 2
                    text: root.name; kind: "heading"; maximumLineCount: 1; wrapMode: Text.NoWrap; elide: Text.ElideRight
                    TextMetrics { id: nameMetrics; font: nameTxt.font; text: root.name }
                }
                Rectangle {
                    visible: root.kind === "local"
                    implicitWidth: kindTxt.implicitWidth + 14; implicitHeight: 20; radius: 10
                    color: Theme.raised
                    Txt { id: kindTxt; anchors.centerIn: parent; text: qsTr("This computer"); kind: "micro"; wrapMode: Text.NoWrap; color: Theme.text }
                }
            }
            Txt { Layout.fillWidth: true; text: root.address; kind: "caption"; wrapMode: Text.NoWrap; elide: Text.ElideRight; font.features: { "tnum": 1 } }
        }

        RowLayout {   // reachability
            Layout.preferredWidth: 116
            Layout.alignment: Qt.AlignVCenter
            spacing: 8
            StatusDot { color: root.statusColor; pulse: root.live === "up"; Layout.alignment: Qt.AlignVCenter; Accessible.ignored: true }
            Txt { Layout.fillWidth: true; text: root.statusText; kind: "caption"; color: root.statusColor; font.weight: Font.Medium; wrapMode: Text.NoWrap; elide: Text.ElideRight }
        }

        RowLayout {   // ping: the time to open a connection to the game port
            Layout.preferredWidth: 78
            Layout.alignment: Qt.AlignVCenter
            spacing: 8
            SignalBars { level: root.pingLevel; color: root.pingColor; Layout.alignment: Qt.AlignVCenter }
            Txt { text: root.live === "up" ? root.ping + " ms" : "—"; kind: "caption"; wrapMode: Text.NoWrap; font.features: { "tnum": 1 }
                  Accessible.ignored: true }
        }

        Item {   // the action: Select, or the Selected label in its place; same width, so the rows line up
            Layout.preferredWidth: 104
            Layout.preferredHeight: 36
            Layout.alignment: Qt.AlignVCenter
            SButton {
                anchors.fill: parent
                visible: !root.selected
                text: qsTr("Select")
                Accessible.name: qsTr("Select %1: use this server when I launch").arg(root.name)
                leftPadding: 8; rightPadding: 8
                onClicked: root.select()
            }
            Rectangle {
                anchors.fill: parent
                visible: root.selected
                radius: 10
                color: Theme.accentTint
                Accessible.role: Accessible.StaticText
                Accessible.name: qsTr("%1 is selected").arg(root.name)
                Row {
                    anchors.centerIn: parent
                    spacing: 6
                    Icon { name: "check"; size: 16; color: Theme.accentFg; anchors.verticalCenter: parent.verticalCenter }
                    Txt { text: qsTr("Selected"); font.weight: Font.Medium; color: Theme.accentFg; wrapMode: Text.NoWrap; anchors.verticalCenter: parent.verticalCenter }
                }
            }
        }

        IconButton {
            id: moreBtn
            Layout.alignment: Qt.AlignVCenter
            glyph: "more"; tip: qsTr("More actions for %1").arg(root.name)
            onClicked: root.openMenu()
        }
    }

    function closeMenu() { menu.close() }
    function openMenu() { menu.popup(moreBtn, moreBtn.width - menu.width, moreBtn.height + 4) }
    function focusAction() { moreBtn.forceActiveFocus() }
    readonly property Item moreButton: moreBtn

    SMenu {
        id: menu
        SMenuItem { text: qsTr("Server info"); onTriggered: root.info() }
        SMenuItem { text: qsTr("Copy address"); onTriggered: root.copyAddress() }
        SMenuItem { text: qsTr("Edit…"); visible: root.editable; height: visible ? implicitHeight : 0; onTriggered: root.edit() }
        SMenuSeparator { visible: root.editable }
        SMenuItem { danger: true; text: qsTr("Delete…"); visible: root.editable; height: visible ? implicitHeight : 0; onTriggered: root.remove() }
    }
    TapHandler { acceptedButtons: Qt.RightButton; onTapped: menu.popup() }
    Keys.onMenuPressed: root.openMenu()
}
