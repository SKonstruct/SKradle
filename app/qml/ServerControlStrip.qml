import QtQuick
import com.skradle.app
import QtQuick.Controls
import QtQuick.Layouts

// "Your server": the control strip above the list. It shows what the hosted server is doing (not installed,
// stopped, starting, running, failed or busy) and the actions that make sense in that state. The server's own web
// page opens from here once it runs.
Card {
    id: root
    signal openWebUi()
    property var s: ({ state: "checking", notice: null, progress: -1, step: "", task: "", java: "", engineVersion: "",
                       gamePort: 47624, webPort: 8080, lan: false, adminUser: "", join: null })
    function parse() { try { s = JSON.parse(ServerHost.status) } catch (e) { /* keep the last */ } }
    Component.onCompleted: parse()
    Connections { target: ServerHost; function onStatusChanged() { root.parse() } }

    readonly property string st: s.state
    readonly property bool installed: st !== "notinstalled" && st !== "checking"
    readonly property bool idle: st === "stopped" || st === "failed"
    readonly property bool canChangeSettings: idle
    readonly property bool needsJava: installed && s.java === ""
    readonly property color dotColor: st === "running" ? Theme.ok : st === "failed" ? Theme.danger
        : st === "starting" || st === "busy" || st === "stopping" ? Theme.amber : Theme.textTertiary
    ClipboardHelper { id: clip }

    function noticeText(n) {
        const d = n.detail
        switch (n.code) {
        case "not_installed": return qsTr("The server engine isn't installed. Choose Install first.")
        case "no_java": return qsTr("The server needs Java 25 or newer to run, and none was found on this computer. Install it, then try again.")
        case "not_configured": return qsTr("The server's settings file isn't in the form SKradle uses. Apply the ports below, then start again.")
        case "port_game_busy": return n.suggestion > 0 ? qsTr("The game port %1 is already used by another program. Close that program, or switch to port %2.").arg(n.port).arg(n.suggestion)
                                                      : qsTr("The game port %1 is already used by another program. Close it or choose another port.").arg(n.port)
        case "port_web_busy": return n.suggestion > 0 ? qsTr("The web port %1 is already used by another program. Close that program, or switch to port %2.").arg(n.port).arg(n.suggestion)
                                                      : qsTr("The web port %1 is already used by another program. Close it or choose another port.").arg(n.port)
        case "port_same": return qsTr("The game port and the web port must be different.")
        case "port_range": return qsTr("Ports must be numbers from 1 to 65535.")
        case "spawn": return qsTr("The server process couldn't be started: %1").arg(d)
        case "early_exit": return qsTr("The server stopped while starting. %1").arg(d)
        case "start_timeout": return qsTr("The server didn't finish starting in time. Try again; its log is in the server folder under data/logs.")
        case "not_running": return qsTr("The server isn't running.")
        case "console": return qsTr("The server didn't accept the request: %1").arg(d)
        case "config": return qsTr("The server's settings couldn't be saved: %1").arg(d)
        case "io": return qsTr("A file operation failed: %1").arg(d)
        case "data_locked": return qsTr("Another server is already using this server's data folder. Close it and try again.")
        case "startup_failed": return qsTr("The server failed to start: %1").arg(d)
        case "ended": return qsTr("The server stopped unexpectedly. %1").arg(d)
        case "offline": return qsTr("Couldn't reach GitHub, where the server engine is downloaded from. Check your internet connection and try again.")
        case "rate_limited": return qsTr("GitHub is limiting requests right now (HTTP %1). Try again in a few minutes.").arg(d)
        case "download_failed": return qsTr("The download failed (%1). Try again.").arg(d)
        case "no_release": return qsTr("No downloadable server release was found. Try again later.")
        case "checksum": return qsTr("The download didn't match its checksum, so it was thrown away. Try again.")
        case "bad_archive": return qsTr("The download wasn't a usable server release. %1").arg(d)
        case "no_previous": return qsTr("There is no earlier engine to go back to.")
        case "terms": return qsTr("Accept the licence to install the server engine.")
        case "up_to_date": return qsTr("The engine is already up to date (%1).").arg(d)
        case "installed": return qsTr("Installed engine %1.").arg(d)
        case "backup_done": return qsTr("Backup made: %1.").arg(d)
        case "restored": return qsTr("Restored %1.").arg(d)
        case "password_reset": return qsTr("A new web interface password was made.")
        case "stop_first": return qsTr("Stop the server before changing these settings.")
        case "register_duplicate": return qsTr("A server you saved is already named “Local Server”, so your own server isn't in the list. Rename or delete it.")
        case "register_failed": return qsTr("Your server couldn't be added to the list: %1").arg(d)
        }
        return d
    }
    function sentence() {
        switch (st) {
        case "checking": return qsTr("Checking the server's files…")
        case "notinstalled": return qsTr("Not installed. SKradle downloads the server engine (Tortodrone) from its author's GitHub releases.")
        case "stopped": return qsTr("Stopped. Engine %1.").arg(s.engineVersion)
        case "starting": return qsTr("Starting…")
        case "running": return qsTr("Running. The game connects to 127.0.0.1:%1.").arg(s.gamePort)
        case "stopping": return qsTr("Stopping…")
        case "failed": return qsTr("Stopped with a problem.")
        case "busy":
            switch (s.task) {
            case "install": return qsTr("Installing the server engine…")
            case "update": return qsTr("Updating the server engine…")
            case "backup": return qsTr("Backing up. The server stops for a moment…")
            case "restore": return qsTr("Restoring a backup. The server stops for a moment…")
            }
        }
        return ""
    }
    function stepText() {
        switch (s.step) {
        case "downloading": return s.progress >= 0 ? qsTr("Downloading… %1%").arg(Math.round(s.progress * 100)) : qsTr("Downloading…")
        case "verifying": return qsTr("Checking the download…")
        case "unpacking": return qsTr("Unpacking…")
        case "installing": return qsTr("Installing…")
        }
        return ""
    }
    function closeDialogs() { installDlg.close(); backupDlg.close() }
    // Screenshots only.
    function demo(name) {
        if (name === "install") { installDlg.update = false; installDlg.openFrom(null) }
        else if (name === "backup") backupDlg.openFrom(null)
    }
    function install() { if (s.termsAccepted) ServerHost.install(true); else { installDlg.update = false; installDlg.openFrom(installBtn) } }
    function updateEngine() { if (s.termsAccepted) ServerHost.updateEngine(); else { installDlg.update = true; installDlg.openFrom(updateBtn) } }
    function applyPorts() { ServerHost.setPorts(parseInt(gamePort.text), parseInt(webPort.text)) }

    implicitHeight: col.implicitHeight + 2 * Theme.s5
    Accessible.role: Accessible.Grouping
    Accessible.name: qsTr("Your server")

    ColumnLayout {
        id: col
        x: Theme.s5; y: Theme.s5
        width: root.width - 2 * Theme.s5
        spacing: Theme.s3

        RowLayout {
            Layout.fillWidth: true
            spacing: Theme.s3
            StatusDot { color: root.dotColor; pulse: root.st === "running"; Layout.alignment: Qt.AlignTop; Layout.topMargin: 3; Accessible.name: root.st }
            ColumnLayout {
                Layout.fillWidth: true; spacing: 2
                Txt { Layout.fillWidth: true; text: qsTr("Your server"); kind: "heading"; Accessible.role: Accessible.Heading }
                Txt { id: stateText; Layout.fillWidth: true; text: root.sentence(); color: Theme.textSecondary; Accessible.role: Accessible.StaticText }
            }
        }

        // Busy: what is happening, and how far.
        ColumnLayout {
            Layout.fillWidth: true
            visible: root.st === "busy"
            spacing: 4
            Rectangle {
                Layout.fillWidth: true; implicitHeight: 6; radius: 3; color: Theme.track
                Accessible.role: Accessible.ProgressBar
                Accessible.name: root.stepText()
                Rectangle {
                    height: parent.height; radius: 3; color: Theme.accent
                    width: root.s.progress >= 0 ? parent.width * root.s.progress : parent.width * 0.3
                    Behavior on width { NumberAnimation { duration: Motion.progress } }
                    SequentialAnimation on x {
                        running: root.st === "busy" && root.s.progress < 0
                        loops: Animation.Infinite
                        NumberAnimation { from: 0; to: parent.parent.width * 0.7; duration: 1100 }
                    }
                }
            }
            Txt { text: root.stepText(); kind: "caption" }
        }

        // The one thing the player should know, with the fix when there is one.
        Rectangle {
            id: noticeBar
            Layout.fillWidth: true
            visible: root.s.notice !== null && root.s.notice !== undefined
            readonly property string kind: visible ? root.s.notice.kind : "info"
            implicitHeight: noticeRow.implicitHeight + 2 * Theme.s3
            radius: Theme.rMd
            color: kind === "error" ? Qt.rgba(Theme.danger.r, Theme.danger.g, Theme.danger.b, 0.14) : kind === "warn" ? Theme.amberTint : Theme.accentTint
            Accessible.role: Accessible.AlertMessage
            Accessible.name: visible ? root.noticeText(root.s.notice) : ""
            RowLayout {
                id: noticeRow
                anchors.fill: parent; anchors.margins: Theme.s3
                spacing: Theme.s3
                Icon {
                    name: noticeBar.kind === "info" ? "info" : "warning"; size: 18; Layout.alignment: Qt.AlignTop
                    color: noticeBar.kind === "error" ? Theme.danger : noticeBar.kind === "warn" ? Theme.amberText : Theme.accentFg
                }
                Txt { Layout.fillWidth: true; text: noticeBar.visible ? root.noticeText(root.s.notice) : ""; color: Theme.text }
                SButton {
                    visible: noticeBar.visible && root.s.notice.suggestion > 0 && root.idle
                    text: qsTr("Use port %1").arg(noticeBar.visible ? root.s.notice.suggestion : 0)
                    onClicked: {
                        const n = root.s.notice
                        if (n.code === "port_game_busy") ServerHost.setPorts(n.suggestion, root.s.webPort)
                        else ServerHost.setPorts(root.s.gamePort, n.suggestion)
                    }
                }
                SButton { variant: "ghost"; text: qsTr("Dismiss"); onClicked: ServerHost.dismiss(); visible: noticeBar.visible && !(root.st === "failed" && root.s.notice.code === "ended") }
            }
        }

        // Java is a precondition for running; say so before the player presses Start.
        RowLayout {
            Layout.fillWidth: true
            visible: root.needsJava && root.idle
            spacing: Theme.s3
            Icon { name: "warning"; size: 18; color: Theme.amberText; Layout.alignment: Qt.AlignTop }
            Txt { Layout.fillWidth: true; text: qsTr("Java 25 or newer wasn't found on this computer. The server can't start without it."); color: Theme.text }
            SButton { text: qsTr("Get Java"); glyph: "globe"; onClicked: Qt.openUrlExternally("https://adoptium.net/temurin/releases/?version=25") }
        }

        // Actions.
        Flow {
            Layout.fillWidth: true
            spacing: Theme.s2
            visible: root.st !== "checking" && root.st !== "busy" && root.st !== "stopping"
            SButton {
                id: installBtn
                visible: root.st === "notinstalled"
                text: qsTr("Install server"); glyph: "download"; highlighted: true
                onClicked: root.install()
            }
            SButton {
                visible: root.st === "running"
                text: qsTr("Open web UI"); glyph: "panel"; highlighted: true
                enabled: ServerHost.url.length > 0
                onClicked: root.openWebUi()
            }
            SButton {
                visible: root.idle
                text: root.st === "failed" ? qsTr("Start again") : qsTr("Start"); glyph: "play"; highlighted: true
                enabled: !root.needsJava
                onClicked: ServerHost.start()
            }
            SButton { visible: root.st === "running"; text: qsTr("Stop"); glyph: "stop"; onClicked: ServerHost.stop() }
            SButton { visible: root.st === "starting"; text: qsTr("Cancel"); glyph: "stop"; onClicked: ServerHost.stop() }
            SButton { visible: root.st === "running"; text: qsTr("Restart"); glyph: "refresh"; onClicked: ServerHost.restart() }
            SButton {
                id: backupBtn
                visible: root.installed && (root.idle || root.st === "running")
                text: qsTr("Back up / restore"); glyph: "folder"
                onClicked: backupDlg.openFrom(backupBtn)
            }
            SButton {
                id: updateBtn
                visible: root.installed && (root.idle || root.st === "running")
                text: qsTr("Update engine"); glyph: "upload"
                onClicked: root.updateEngine()
            }
        }

        // While running: where to connect and how to sign in to the web page.
        ColumnLayout {
            Layout.fillWidth: true
            visible: root.st === "running"
            spacing: Theme.s2
            Divider { Layout.fillWidth: true }
            Txt {
                Layout.fillWidth: true
                text: root.s.join && root.s.join.reachableByOthers
                    ? qsTr("Other players join at %1, port %2. They add it with Add server, using the public key and content address from the join details.").arg(root.s.join.host).arg(root.s.join.port)
                    : qsTr("Only this computer can join. To let others in, stop the server and turn on network access.")
                kind: "caption"
            }
            SButton {
                visible: root.s.join !== null && root.s.join.reachableByOthers
                text: qsTr("Copy join details"); glyph: "copy"
                onClicked: {
                    const j = root.s.join
                    clip.copy(qsTr("Server address: %1\nPort: %2\nPublic key: %3\nContent URL: %4").arg(j.host).arg(j.port).arg(j.publicKey).arg(j.contentUrl))
                    Data.toast(qsTr("Join details copied"))
                }
            }
            RowLayout {
                Layout.fillWidth: true
                visible: root.s.adminUser.length > 0
                spacing: Theme.s2
                Txt { text: qsTr("Web interface sign-in:"); kind: "caption" }
                Txt { text: root.s.adminUser; font.weight: Font.Medium; kind: "caption"; color: Theme.text; wrapMode: Text.NoWrap }
                Txt {
                    id: pwText
                    property bool shown: false
                    property string value: ""
                    text: shown ? value : "••••••••••"
                    kind: "caption"; color: Theme.text; wrapMode: Text.NoWrap
                    font.features: { "tnum": 1 }
                    Accessible.name: shown ? qsTr("Password: %1").arg(value) : qsTr("Password hidden")
                }
                SButton {
                    text: pwText.shown ? qsTr("Hide") : qsTr("Show"); variant: "ghost"
                    onClicked: { pwText.shown = !pwText.shown; pwText.value = pwText.shown ? ServerHost.adminPassword() : "" }
                }
                SButton { text: qsTr("Copy password"); variant: "ghost"; glyph: "copy"; onClicked: { clip.copy(ServerHost.adminPassword()); Data.toast(qsTr("Password copied")) } }
                SButton { text: qsTr("New password"); variant: "ghost"; onClicked: { pwText.shown = false; pwText.value = ""; ServerHost.resetAdminPassword() } }
                Item { Layout.fillWidth: true }
            }
        }

        // Settings: network access and ports. Only while the server is stopped, because the engine reads them once.
        ColumnLayout {
            Layout.fillWidth: true
            visible: root.installed && root.st !== "busy"
            spacing: Theme.s2
            Divider { Layout.fillWidth: true }
            RowLayout {
                Layout.fillWidth: true
                spacing: Theme.s4
                SSwitch {
                    id: lanSwitch
                    text: qsTr("Let other players on my network join")
                    enabled: root.canChangeSettings
                    checked: root.s.lan
                    onToggled: ServerHost.setLan(checked)
                    Accessible.name: text
                }
                Item { Layout.fillWidth: true }
                Txt { text: qsTr("Game port"); kind: "caption" }
                STextField {
                    id: gamePort
                    Layout.preferredWidth: 84
                    enabled: root.canChangeSettings
                    text: String(root.s.gamePort)
                    validator: IntValidator { bottom: 1; top: 65535 }
                    inputMethodHints: Qt.ImhDigitsOnly
                    Accessible.name: qsTr("Game port")
                    onAccepted: root.applyPorts()
                }
                Txt { text: qsTr("Web port"); kind: "caption" }
                STextField {
                    id: webPort
                    Layout.preferredWidth: 84
                    enabled: root.canChangeSettings
                    text: String(root.s.webPort)
                    validator: IntValidator { bottom: 1; top: 65535 }
                    inputMethodHints: Qt.ImhDigitsOnly
                    Accessible.name: qsTr("Web port, used by the server's web page and news")
                    onAccepted: root.applyPorts()
                }
                SButton {
                    text: qsTr("Apply")
                    enabled: root.canChangeSettings && (parseInt(gamePort.text) !== root.s.gamePort || parseInt(webPort.text) !== root.s.webPort)
                    onClicked: root.applyPorts()
                }
            }
            Txt {
                Layout.fillWidth: true
                kind: "caption"
                text: !root.canChangeSettings ? qsTr("Stop the server to change these settings.")
                    : root.s.lan ? qsTr("Network access also opens the server's web page to your network, protected only by its sign-in. Turn it on only on a network you trust.")
                    : qsTr("Only this computer can connect now.")
            }
        }
    }

    InstallDialog {
        id: installDlg
        onAccepted: if (update) ServerHost.updateEngine(); else ServerHost.install(true)
    }
    BackupDialog { id: backupDlg }
}
