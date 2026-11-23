import QtQuick
import com.skradle.app
import QtQuick.Controls
import QtQuick.Layouts

// "Your server": a compact summary of the hosted server under the list. One line says what it is doing and
// offers the one action that fits (Install, Start; while running: console, web settings, Stop). Everything else (restart,
// backups, engine update, address, web password reset, network access, ports) is under "Manage". Problems show as a banner below the
// line, with a plain sentence; the engine's own text goes to the log.
Card {
    id: root
    signal openWebUi()
    property var s: ({ state: "checking", notice: null, progress: -1, step: "", task: "", java: "", engineVersion: "",
                       gamePort: 47624, webPort: 8080, lan: false, adminUser: "", join: null })
    function parse() { try { s = JSON.parse(ServerHost.status) } catch (e) { /* keep the last */ } }
    Component.onCompleted: parse()
    Connections { target: ServerHost; function onStatusChanged() { root.parse() } }

    property bool manage: false
    readonly property string st: s.state
    readonly property bool installed: st !== "notinstalled" && st !== "checking"
    readonly property bool idle: st === "stopped" || st === "failed"
    readonly property bool canChangeSettings: idle
    readonly property bool needsJava: installed && s.java === ""
    readonly property color dotColor: st === "running" ? Theme.ok : st === "failed" ? Theme.danger
        : st === "starting" || st === "busy" || st === "stopping" ? Theme.amber : Theme.textTertiary
    ClipboardHelper { id: clip }

    // Every message is a sentence that says what happened and what to do. The engine's own text is never shown.
    function noticeText(n) {
        const d = n.detail
        switch (n.code) {
        case "not_installed": return qsTr("The server engine isn't installed yet. Choose Install server first.")
        case "no_java": return qsTr("The server needs Java 25 or newer to run, and none was found on this computer. Install it, then try again.")
        case "not_configured": return qsTr("The server's settings file isn't in the form SKradle uses. Apply the ports under Manage, then start again.")
        case "port_game_busy": return n.suggestion > 0 ? qsTr("The game port %1 is already used by another program. Close that program, or switch to port %2.").arg(n.port).arg(n.suggestion)
                                                      : qsTr("The game port %1 is already used by another program. Close it or choose another port.").arg(n.port)
        case "port_web_busy": return n.suggestion > 0 ? qsTr("The web port %1 is already used by another program. Close that program, or switch to port %2.").arg(n.port).arg(n.suggestion)
                                                      : qsTr("The web port %1 is already used by another program. Close it or choose another port.").arg(n.port)
        case "port_same": return qsTr("The game port and the web port must be different.")
        case "port_range": return qsTr("Ports must be numbers from 1 to 65535.")
        case "spawn": return qsTr("The server couldn't be started. Check that Java 25 or newer is installed, then try again.")
        case "early_exit": return qsTr("The server stopped while starting. Try again; its log is in the server folder under data/logs.")
        case "start_timeout": return qsTr("The server didn't finish starting in time. Try again; its log is in the server folder under data/logs.")
        case "not_running": return qsTr("The server isn't running.")
        case "console": return qsTr("The server didn't accept the request. Try again in a moment.")
        case "config": return qsTr("The server's settings couldn't be saved. Check that the server folder is writable.")
        case "io": return qsTr("A file couldn't be read or written. Check that the server folder is writable and the disk isn't full.")
        case "data_locked": return qsTr("Another server is already using this server's data folder. Close it and try again.")
        case "startup_failed": return qsTr("The server failed to start. Try again; its log is in the server folder under data/logs.")
        case "ended": return qsTr("The server stopped unexpectedly. Start it again; its log is in the server folder under data/logs.")
        case "offline": return qsTr("Couldn't reach GitHub, where the server engine is downloaded from. Check your internet connection and try again.")
        case "rate_limited": return qsTr("GitHub is limiting requests right now (HTTP %1). Try again in a few minutes.").arg(d)
        case "download_failed": return d.length > 0 ? qsTr("The download failed (%1). Try again.").arg(d)
                                                    : qsTr("The download failed. Check your internet connection and try again.")
        case "no_release": return qsTr("No downloadable server release was found. Try again later.")
        case "checksum": return qsTr("The download didn't match its checksum, so it was thrown away. Try again.")
        case "bad_archive": return qsTr("The download wasn't a usable server release. Try again later.")
        case "no_previous": return qsTr("There is no earlier engine to go back to.")
        case "terms": return qsTr("Accept the licence to install the server engine.")
        case "up_to_date": return qsTr("The engine is already up to date (%1).").arg(d)
        case "installed": return qsTr("Installed engine %1.").arg(d)
        case "backup_done": return qsTr("Backup made: %1.").arg(d)
        case "restored": return qsTr("Restored %1.").arg(d)
        case "password_reset": return qsTr("A new web interface password was made. The settings page signs in with it by itself.")
        case "stop_first": return qsTr("Stop the server before changing these settings.")
        case "register_duplicate": return qsTr("A server you saved is already named \u201CLocal Server\u201D, so your own server isn't in the list. Rename or delete it.")
        case "register_failed": return qsTr("Your server couldn't be added to the list. Check that the settings folder is writable.")
        }
        return qsTr("Something went wrong with the server. Try again.")
    }
    function sentence() {
        switch (st) {
        case "checking": return qsTr("Checking the server's files\u2026")
        case "notinstalled": return qsTr("Not installed. SKradle downloads the server engine (Tortodrone) from its author's GitHub releases.")
        case "stopped": return qsTr("Stopped. Engine %1.").arg(s.engineVersion)
        case "starting": return qsTr("Starting\u2026")
        case "running": return qsTr("Running on 127.0.0.1:%1").arg(s.gamePort)
        case "stopping": return qsTr("Stopping\u2026")
        case "failed": return qsTr("Stopped with a problem.")
        case "busy":
            switch (s.task) {
            case "install": return qsTr("Installing the server engine\u2026")
            case "update": return qsTr("Updating the server engine\u2026")
            case "backup": return qsTr("Backing up. The server stops for a moment\u2026")
            case "restore": return qsTr("Restoring a backup. The server stops for a moment\u2026")
            }
        }
        return ""
    }
    function stepText() {
        switch (s.step) {
        case "downloading": return s.progress >= 0 ? qsTr("Downloading\u2026 %1%").arg(Math.round(s.progress * 100)) : qsTr("Downloading\u2026")
        case "verifying": return qsTr("Checking the download\u2026")
        case "unpacking": return qsTr("Unpacking\u2026")
        case "installing": return qsTr("Installing\u2026")
        }
        return ""
    }
    function closeDialogs() { installDlg.close(); backupDlg.close(); consoleDlg.close() }
    // Screenshots only.
    function demo(name) {
        if (name === "install") { installDlg.update = false; installDlg.openFrom(null) }
        else if (name === "backup") backupDlg.openFrom(null)
        else if (name === "console") consoleDlg.openFrom(null)
    }
    function install() { if (s.termsAccepted) ServerHost.install(true); else { installDlg.update = false; installDlg.openFrom(installBtn) } }
    function updateEngine() { if (s.termsAccepted) ServerHost.updateEngine(); else { installDlg.update = true; installDlg.openFrom(updateBtn) } }
    function applyPorts() { ServerHost.setPorts(parseInt(gamePort.text), parseInt(webPort.text)) }
    readonly property bool portsValid: { const g = parseInt(gamePort.text), w = parseInt(webPort.text); return g >= 1 && g <= 65535 && w >= 1 && w <= 65535 }

    implicitHeight: col.implicitHeight + 2 * Theme.padCompact
    Accessible.role: Accessible.Grouping
    Accessible.name: qsTr("Your server")

    ColumnLayout {
        id: col
        x: Theme.padCompact; y: Theme.padCompact
        width: root.width - 2 * Theme.padCompact
        spacing: Theme.gapStack

        // The summary line.
        RowLayout {
            Layout.fillWidth: true
            spacing: Theme.gapStack
            StatusDot { color: root.dotColor; pulse: root.st === "running"; Layout.alignment: Qt.AlignVCenter; Accessible.ignored: true }
            ColumnLayout {
                Layout.fillWidth: true
                Layout.preferredWidth: 0
                Layout.minimumWidth: 120
                spacing: Theme.s0
                Txt { Layout.fillWidth: true; text: qsTr("Your server"); font.weight: Font.DemiBold; Accessible.role: Accessible.Heading }
                Txt { id: stateText; Layout.fillWidth: true; text: root.sentence(); kind: "caption"; Accessible.role: Accessible.StaticText }
            }
            SButton {
                id: installBtn
                visible: root.st === "notinstalled"
                text: qsTr("Install server"); glyph: "download"; highlighted: true
                onClicked: root.install()
            }
            SButton {
                visible: root.idle
                text: root.st === "failed" ? qsTr("Start again") : qsTr("Start"); glyph: "play"; highlighted: true
                enabled: !root.needsJava
                onClicked: ServerHost.start()
            }
            IconButton {
                visible: root.st === "running"
                glyph: "terminal"; tip: qsTr("Server console")
                onClicked: consoleDlg.openFrom(this)
            }
            IconButton {
                visible: root.st === "running"
                glyph: "gear"; tip: qsTr("Server settings (web UI)")
                enabled: ServerHost.url.length > 0
                onClicked: root.openWebUi()
            }
            ServerStopButton { visible: root.st === "running"; onClicked: ServerHost.stop() }
            SButton { visible: root.st === "starting"; text: qsTr("Cancel"); glyph: "stop"; onClicked: ServerHost.stop() }
            IconButton {
                id: manageBtn
                visible: root.installed && root.st !== "busy"
                glyph: root.manage ? "chevron-up" : "chevron-down"
                toggledOn: root.manage
                tip: root.manage ? qsTr("Hide the server options") : qsTr("Manage the server: backups, ports, network access")
                onClicked: root.manage = !root.manage
            }
        }

        // Busy: what is happening, and how far.
        ColumnLayout {
            Layout.fillWidth: true
            visible: root.st === "busy"
            spacing: Theme.s1
            // The fill is clipped to the track and positioned from its own left edge, so it starts and ends exactly
            // on the track's edges. (A moving fill left at an offset by the indeterminate animation once shifted it.)
            Rectangle {
                id: track
                Layout.fillWidth: true; implicitHeight: 6; radius: height / 2; color: Theme.track
                clip: true
                readonly property bool known: root.s.progress >= 0
                Accessible.role: Accessible.ProgressBar
                Accessible.name: root.stepText()
                Rectangle {
                    id: fill
                    height: parent.height; radius: height / 2; color: Theme.accent
                    // Smooth the value, not the width: a layout change of the track must never animate the fill.
                    property real shown: Math.max(0, Math.min(1, root.s.progress))
                    Behavior on shown { NumberAnimation { duration: Motion.progress } }
                    width: track.known ? Math.round(track.width * shown) : Math.round(track.width * 0.3)
                    x: track.known ? 0 : slide
                    property real slide: 0
                    SequentialAnimation on slide {
                        running: root.st === "busy" && !track.known
                        loops: Animation.Infinite
                        NumberAnimation { from: -fill.width; to: track.width; duration: Motion.loop }
                    }
                }
            }
            Txt { text: root.stepText(); kind: "caption" }
        }

        // The one thing the player should know, with the fix when there is one.
        Banner {
            id: noticeBar
            visible: root.s.notice !== null && root.s.notice !== undefined
            kind: visible ? (root.s.notice.kind === "error" ? "danger" : root.s.notice.kind === "warn" ? "warn" : "info") : "info"
            text: visible ? root.noticeText(root.s.notice) : ""
            actionText: visible && root.s.notice.suggestion > 0 && root.idle ? qsTr("Use port %1").arg(root.s.notice.suggestion) : ""
            dismissible: visible && !(root.st === "failed" && root.s.notice.code === "ended")
            onAction: {
                const n = root.s.notice
                if (n.code === "port_game_busy") ServerHost.setPorts(n.suggestion, root.s.webPort)
                else ServerHost.setPorts(root.s.gamePort, n.suggestion)
            }
            onDismissed: ServerHost.dismiss()
        }

        // Java is a precondition for running; say so before the player presses Start.
        Banner {
            visible: root.needsJava && root.idle
            kind: "warn"
            text: qsTr("Java 25 or newer wasn't found on this computer. The server can't start without it.")
            actionText: qsTr("Get Java")
            onAction: Qt.openUrlExternally("https://adoptium.net/temurin/releases/?version=25")
        }

        // Manage: everything that is not the one action above.
        ColumnLayout {
            Layout.fillWidth: true
            visible: root.manage && root.installed && root.st !== "busy"
            spacing: Theme.gapStack
            Divider { Layout.fillWidth: true }

            Flow {
                Layout.fillWidth: true
                spacing: Theme.gapInline
                visible: root.idle || root.st === "running"
                SButton { visible: root.st === "running"; text: qsTr("Restart"); glyph: "refresh"; onClicked: ServerHost.restart() }
                SButton {
                    id: backupBtn
                    text: qsTr("Back up and restore"); glyph: "download"
                    onClicked: backupDlg.openFrom(backupBtn)
                }
                SButton {
                    text: qsTr("Open folder"); glyph: "folder"
                    Accessible.name: qsTr("Open the server's folder")
                    onClicked: ServerHost.openFolder()
                }
                SButton {
                    id: updateBtn
                    text: qsTr("Update engine"); glyph: "upload"
                    onClicked: root.updateEngine()
                }
                SButton {
                    visible: root.st === "running" && root.s.join !== null && root.s.join.reachableByOthers
                    text: qsTr("Copy address"); glyph: "copy"
                    onClicked: {
                        const j = root.s.join
                        clip.copy(qsTr("Server address: %1\nPort: %2\nPublic key: %3\nContent URL: %4").arg(j.host).arg(j.port).arg(j.publicKey).arg(j.contentUrl))
                        Data.toast(qsTr("Server address copied"))
                    }
                }
                SButton {
                    visible: root.st === "running" && root.s.adminUser.length > 0
                    text: qsTr("Reset web password"); variant: "ghost"
                    Accessible.name: qsTr("Reset the web interface password. Only needed if the server's page won't sign in.")
                    onClicked: ServerHost.resetAdminPassword()
                }
            }

            Divider { Layout.fillWidth: true }

            // Network access and ports. Only while the server is stopped, because the engine reads them once.
            SSwitch {
                id: lanSwitch
                text: qsTr("Let other players on my network join")
                enabled: root.canChangeSettings
                checked: root.s.lan
                onToggled: ServerHost.setLan(checked)
                Accessible.name: text
            }
            Flow {
                Layout.fillWidth: true
                spacing: Theme.gapStack
                Row {
                    height: Theme.hMd
                    spacing: Theme.gapInline
                    Txt { anchors.verticalCenter: parent.verticalCenter; text: qsTr("Game port"); kind: "caption" }
                    STextField {
                        id: gamePort
                        width: 84
                        label: qsTr("Game port")
                        enabled: root.canChangeSettings
                        text: String(root.s.gamePort)
                        invalid: !(parseInt(text) >= 1)
                        validator: IntValidator { bottom: 1; top: 65535 }
                        inputMethodHints: Qt.ImhDigitsOnly
                        font.features: { "tnum": 1 }
                        onAccepted: if (root.portsValid) root.applyPorts()
                    }
                }
                Row {
                    height: Theme.hMd
                    spacing: Theme.gapInline
                    Txt { anchors.verticalCenter: parent.verticalCenter; text: qsTr("Web port"); kind: "caption" }
                    STextField {
                        id: webPort
                        width: 84
                        label: qsTr("Web port, used by the server's web page and news")
                        enabled: root.canChangeSettings
                        text: String(root.s.webPort)
                        invalid: !(parseInt(text) >= 1)
                        validator: IntValidator { bottom: 1; top: 65535 }
                        inputMethodHints: Qt.ImhDigitsOnly
                        font.features: { "tnum": 1 }
                        onAccepted: if (root.portsValid) root.applyPorts()
                    }
                }
                SButton {
                    text: qsTr("Apply")
                    enabled: root.canChangeSettings && root.portsValid && (parseInt(gamePort.text) !== root.s.gamePort || parseInt(webPort.text) !== root.s.webPort)
                    onClicked: root.applyPorts()
                }
            }
            Txt {
                Layout.fillWidth: true
                kind: "caption"
                color: !root.portsValid && root.canChangeSettings ? Theme.danger : Theme.textSecondary
                text: !root.canChangeSettings ? qsTr("Stop the server to change these settings.")
                    : !root.portsValid ? qsTr("Use ports from 1 to 65535.")
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
    ServerConsoleDialog { id: consoleDlg }
}
