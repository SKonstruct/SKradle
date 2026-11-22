import QtQuick
import com.skradle.app
import QtQuick.Controls
import QtQuick.Layouts

// One row of the server list. The whole row chooses the server (one action). The choice is shown once: a 3 px
// accent bar on the left edge and a ticked circle. Status is shown once, in one right-aligned column: the ping
// when the server answers, a word only when it does not.
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
    focusPolicy: Qt.StrongFocus
    padding: Theme.padCompact
    leftPadding: Theme.s5
    property int radius: Theme.rMd
    readonly property bool editable: kind === "saved"
    // The hosted server that is not running cannot answer: say that instead of "unreachable".
    readonly property string live: kind === "local" && ServerHost.state !== "running" ? "stopped" : probeState
    readonly property color statusColor: live === "up" ? Theme.ok : live === "down" ? Theme.danger : Theme.textTertiary
    // The ping when it answers; a word only for the states that need one.
    readonly property string statusText: live === "up" ? qsTr("%1 ms").arg(ping) : live === "down" ? qsTr("Unreachable")
        : live === "stopped" ? qsTr("Not running") : live === "checking" ? qsTr("Checking\u2026") : "\u2014"

    Accessible.role: Accessible.RadioButton
    Accessible.name: name + ", " + address + ", " + (live === "up" ? qsTr("reachable, %1 milliseconds").arg(ping) : statusText)
    Accessible.checkable: true
    Accessible.checked: selected
    Accessible.onPressAction: root.select()
    Keys.onReturnPressed: root.select()
    Keys.onSpacePressed: root.select()

    background: Item {
        Card {
            anchors.fill: parent
            raised: root.hovered && !root.selected
            fill: root.hovered ? Theme.cardHover : Theme.card
            radius: root.radius
            Rectangle {   // the chosen row's cue
                visible: root.selected
                x: 0; y: Theme.s3
                width: 3; height: parent.height - 2 * Theme.s3
                radius: Theme.rXs
                color: Theme.accent
            }
        }
        MouseArea {
            anchors.fill: parent
            cursorShape: Qt.PointingHandCursor
            onClicked: { root.forceActiveFocus(); root.select() }
        }
        FocusRing { grow: 3 }
    }

    contentItem: RowLayout {
        spacing: Theme.s4

        // Choice circle: ticked when this is the server the game will use.
        Item {
            Layout.preferredWidth: 22; Layout.preferredHeight: 22
            Layout.alignment: Qt.AlignVCenter
            Accessible.ignored: true
            Rectangle {
                anchors.fill: parent; radius: width / 2
                color: root.selected ? Theme.accent : "transparent"
                border.width: root.selected ? 0 : 2
                border.color: Theme.fieldBorder
                Icon { anchors.centerIn: parent; name: "check"; size: 14; stroke: 2.6; color: Theme.accentText; visible: root.selected }
            }
        }

        ColumnLayout {
            Layout.fillWidth: true
            Layout.preferredWidth: 0
            Layout.minimumWidth: 120
            spacing: Theme.s0
            RowLayout {
                spacing: Theme.gapInline
                Txt {
                    id: nameTxt
                    Layout.fillWidth: true
                    Layout.maximumWidth: nameMetrics.advanceWidth + 2
                    text: root.name; kind: "heading"; maximumLineCount: 1; wrapMode: Text.NoWrap; elide: Text.ElideRight
                    TextMetrics { id: nameMetrics; font: nameTxt.font; text: root.name }
                }
                Tag { visible: root.kind === "local"; text: qsTr("This computer") }
            }
            Txt {
                id: addrTxt
                Layout.fillWidth: true
                text: root.address; kind: "caption"; mono: true; numeric: true
                wrapMode: Text.NoWrap; elide: Text.ElideMiddle
                HoverHandler { id: addrHover }
                Tip { text: root.address; wanted: addrHover.hovered && addrTxt.truncated }
            }
            Txt {
                Layout.fillWidth: true
                visible: root.kind === "local" && root.selected && root.live === "stopped"
                text: qsTr("Start your server before you launch.")
                kind: "caption"; color: Theme.amber
                wrapMode: Text.NoWrap; elide: Text.ElideRight
            }
        }

        // Status: a dot and one value, right aligned so the numbers line up from row to row.
        RowLayout {
            Layout.preferredWidth: 112
            Layout.alignment: Qt.AlignVCenter
            spacing: Theme.gapInline
            Accessible.ignored: true
            Item { Layout.fillWidth: true }
            StatusDot { color: root.statusColor; pulse: root.live === "up"; Layout.alignment: Qt.AlignVCenter }
            Txt {
                text: root.statusText
                numeric: true; kind: "caption"
                color: root.live === "up" ? Theme.text : root.live === "down" ? Theme.danger : Theme.textSecondary
                wrapMode: Text.NoWrap
                horizontalAlignment: Text.AlignRight
                Layout.preferredWidth: 80
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
    function openMenu() { menu.popup(moreBtn, moreBtn.width - menu.width, moreBtn.height + Theme.s1) }
    function focusAction() { moreBtn.forceActiveFocus() }
    readonly property Item moreButton: moreBtn

    SMenu {
        id: menu
        SMenuItem { text: qsTr("Server info"); onTriggered: root.info() }
        SMenuItem { text: qsTr("Copy address"); onTriggered: root.copyAddress() }
        SMenuItem { text: qsTr("Edit\u2026"); visible: root.editable; height: visible ? implicitHeight : 0; onTriggered: root.edit() }
        SMenuSeparator { visible: root.editable }
        SMenuItem { danger: true; text: qsTr("Delete\u2026"); visible: root.editable; height: visible ? implicitHeight : 0; onTriggered: root.remove() }
    }
    TapHandler { acceptedButtons: Qt.RightButton; onTapped: menu.popup() }
    Keys.onMenuPressed: root.openMenu()
}
