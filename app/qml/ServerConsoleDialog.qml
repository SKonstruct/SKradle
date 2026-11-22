import QtQuick
import com.skradle.app
import QtQuick.Controls
import QtQuick.Layouts

// The hosted server's console: its live output (bounded scrollback, follows the end until the player scrolls up)
// and an input line that sends commands to the server's own console. Polls while open, never blocks the UI.
ServerDialog {
    id: dlg
    property int cursor: 0
    readonly property int cap: 2000
    readonly property bool live: ServerHost.state === "running" || ServerHost.state === "starting"
    preferredWidth: 760
    title: qsTr("Server console")
    subtitle: qsTr("What the server prints, and a line to send it commands. Type help to see what it understands.")
    onFocusFirst: input.forceActiveFocus()

    ListModel { id: lines }
    ClipboardHelper { id: clip }

    function add(list) {
        if (list.length === 0) return
        const stick = view.atYEnd
        for (const l of list) lines.append({ line: l })
        if (lines.count > cap) lines.remove(0, lines.count - cap)
        if (stick) view.positionViewAtEnd()
    }
    function poll() {
        let r
        try { r = JSON.parse(ServerHost.consoleSince(cursor)) } catch (e) { return }
        if (r.live && r.cursor < cursor) { lines.clear(); add([qsTr("\u2014 the server restarted \u2014")]) }
        add(r.lines)
        if (r.live) cursor = r.cursor
    }
    function send() {
        const t = input.text
        if (t.trim().length === 0) return
        let r
        try { r = JSON.parse(ServerHost.consoleSend(t)) } catch (e) { return }
        if (r.ok) { add([r.echo]); input.text = ""; view.positionViewAtEnd() }
        else Data.toast(qsTr("The server isn't running."), "error")
    }
    function allText() { let t = []; for (let i = 0; i < lines.count; ++i) t.push(lines.get(i).line); return t.join("\n") }

    onAboutToShow: { lines.clear(); cursor = 0; input.text = ""; poll() }
    Timer { interval: 400; repeat: true; running: dlg.opened; onTriggered: dlg.poll() }

    Rectangle {
        Layout.fillWidth: true
        Layout.preferredHeight: Math.max(200, Math.min(380, (dlg.parent ? dlg.parent.height : 600) - 300))
        radius: Theme.rSm
        color: Theme.field
        border.width: 1; border.color: Theme.border
        Accessible.role: Accessible.StaticText
        Accessible.name: qsTr("Server output")

        ListView {
            id: view
            anchors.fill: parent; anchors.margins: Theme.s2
            model: lines
            clip: true
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: SScrollBar {}
            delegate: Text {
                required property string line
                width: view.width - Theme.s4
                text: line
                wrapMode: Text.WrapAnywhere
                textFormat: Text.PlainText
                font.family: Theme.monoFamily
                font.pixelSize: Theme.fsCaption
                color: line.startsWith("> ") ? Theme.accent : Theme.text
            }
            Txt {
                anchors.centerIn: parent
                visible: lines.count === 0
                text: dlg.live ? qsTr("Waiting for output\u2026") : qsTr("The server isn't running.")
                color: Theme.textSecondary
            }
        }
        SButton {
            visible: !view.atYEnd
            anchors.right: parent.right; anchors.bottom: parent.bottom; anchors.margins: Theme.s3
            size: "sm"; glyph: "chevron-down"; text: qsTr("Latest")
            onClicked: view.positionViewAtEnd()
        }
    }
    STextField {
        id: input
        Layout.fillWidth: true
        label: qsTr("Command for the server")
        placeholderText: dlg.live ? qsTr("Type a command and press Enter") : qsTr("The server isn't running")
        enabled: dlg.live
        font.family: Theme.monoFamily
        onAccepted: dlg.send()
        Keys.onUpPressed: (e) => { text = ServerHost.consoleRecall(true, text); cursorPosition = text.length }
        Keys.onDownPressed: (e) => { text = ServerHost.consoleRecall(false, text); cursorPosition = text.length }
    }
    buttons: [
        SButton { text: qsTr("Copy"); glyph: "copy"; variant: "ghost"; onClicked: { clip.copy(dlg.allText()); Data.toast(qsTr("Console copied")) } },
        SButton { text: qsTr("Clear"); variant: "ghost"; onClicked: lines.clear() },
        Item { Layout.fillWidth: true },
        SButton { text: qsTr("Close"); highlighted: true; onClicked: dlg.close() }
    ]
}
