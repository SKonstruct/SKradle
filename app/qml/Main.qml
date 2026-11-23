import QtQuick
import com.skradle.app
import QtQuick.Controls

ApplicationWindow {
    id: win
    visible: true
    width: AppEnv.initialWidth
    height: AppEnv.initialHeight
    minimumWidth: 900
    minimumHeight: 600
    title: "SKradle"
    color: Theme.bg

    // Palette: native styles ignore these (verified); Fusion/Basic follow them. The highlight stays the system one.
    palette.window: Theme.bg
    palette.windowText: Theme.text
    palette.base: Theme.card
    palette.text: Theme.text
    palette.button: Theme.raised
    palette.buttonText: Theme.text
    palette.toolTipBase: Theme.raised
    palette.toolTipText: Theme.text
    palette.placeholderText: Theme.textTertiary

    // The settings file was unreadable and reset to the defaults: say so once, instead of silently forgetting choices.
    Component.onCompleted: if (Settings.loadWarning.length > 0) Qt.callLater(() => Data.toast(Settings.loadWarning, "error"))

    property int current: AppEnv.initialPage
    property int dir: 1
    property bool collapsed: AppEnv.initialCollapsed
    readonly property var navItems: [
        { label: qsTr("Home"), icon: "home" },
        { label: qsTr("Servers"), icon: "servers" },
        { label: qsTr("Mods"), icon: "mods" },
        { label: qsTr("Tools"), icon: "tools" },
        { label: qsTr("Settings"), icon: "graphics" }
    ]
    function go(i) {
        if (i === current || i < 0 || i >= navItems.length) return
        dir = i > current ? 1 : -1
        current = i
    }
    // Go to a page section (Settings): the page may still be loading, so scrolling waits until it exists.
    property int pendingSection: -1
    Timer {
        id: sectionTimer
        interval: 60
        repeat: true
        onTriggered: {
            const p = win.pageItem(win.current)
            if (p && p.scrollTo && win.pendingSection >= 0) { p.scrollTo(win.pendingSection); win.pendingSection = -1; stop() }
            else if (++count > 30) { win.pendingSection = -1; stop() }
        }
        property int count: 0
        onRunningChanged: if (running) count = 0
    }
    function pageItem(i) { return pages.itemAt(i) ? pages.itemAt(i).item : null }

    Shortcut { sequence: "Ctrl+1"; onActivated: win.go(0) }
    Shortcut { sequence: "Ctrl+2"; onActivated: win.go(1) }
    Shortcut { sequence: "Ctrl+3"; onActivated: win.go(2) }
    Shortcut { sequence: "Ctrl+4"; onActivated: win.go(3) }
    Shortcut { sequence: "Ctrl+5"; onActivated: win.go(4) }
    Shortcut { sequence: "Ctrl+B"; onActivated: win.collapsed = !win.collapsed }

    Sidebar {
        id: side
        z: 2
        height: parent.height
        items: win.navItems
        current: win.current
        collapsed: win.collapsed
        onActivated: (i) => win.go(i)
        onToggleCollapsed: win.collapsed = !win.collapsed
    }

    // Content area. Its width is the FINAL width immediately (no layout work per frame while the sidebar animates);
    // only its x follows the sidebar edge. Anything past the window edge is simply clipped by the window.
    Item {
        id: content
        x: side.width
        width: win.width - side.targetWidth
        height: parent.height

        Repeater {
            id: pages
            model: win.navItems.length
            delegate: Item {
                id: host
                required property int index
                readonly property bool active: index === win.current
                property bool visited: active
                property alias item: loader.item
                width: content.width
                height: content.height
                opacity: 0
                visible: opacity > 0.001
                enabled: active
                onActiveChanged: {
                    if (active) { visited = true; exitAnim.stop(); enterAnim.restart() }
                    else { enterAnim.stop(); exitAnim.restart() }
                }
                Component.onCompleted: opacity = active ? 1 : 0

                ParallelAnimation {
                    id: enterAnim
                    SequentialAnimation {
                        PauseAnimation { duration: Motion.pageOut * 0.4 }
                        ParallelAnimation {
                            NumberAnimation { target: host; property: "opacity"; from: 0; to: 1; duration: Motion.pageIn; easing.type: Easing.BezierSpline; easing.bezierCurve: Motion.decel }
                            NumberAnimation { target: host; property: "y"; from: win.dir * Motion.pageShift; to: 0; duration: Motion.pageIn; easing.type: Easing.BezierSpline; easing.bezierCurve: Motion.decel }
                        }
                    }
                }
                ParallelAnimation {
                    id: exitAnim
                    NumberAnimation { target: host; property: "opacity"; to: 0; duration: Motion.pageOut; easing.type: Easing.BezierSpline; easing.bezierCurve: Motion.accel }
                    NumberAnimation { target: host; property: "y"; to: -win.dir * Motion.pageShift * 0.5; duration: Motion.pageOut; easing.type: Easing.BezierSpline; easing.bezierCurve: Motion.accel }
                }
                Loader {
                    id: loader
                    anchors.fill: parent
                    active: host.visited
                    sourceComponent: [homeC, serversC, modsC, toolsC, settingsC][host.index]
                }
            }
        }
        Component { id: homeC; HomePage {} }
        Component { id: serversC; ServersPage {} }
        Component { id: modsC; ModsPage {} }
        Component { id: toolsC; ToolsPage {} }
        Component { id: settingsC; SettingsPage {} }
    }

    // "Update ready": a calm banner once an update has been downloaded (automatically or by hand).
    Banner {
        id: updateReady
        z: 4
        property bool dismissed: false
        visible: UpdateModel.state === "ready" && !dismissed
        kind: "info"
        title: qsTr("Update ready")
        text: qsTr("SKradle %1 is downloaded.").arg(UpdateModel.version)
        actionText: qsTr("Install and restart")
        dismissible: true
        width: Math.min(content.width - 2 * Theme.s6, 560)
        x: content.x + (content.width - width) / 2
        y: Theme.s3
        onAction: UpdateModel.install()
        onDismissed: dismissed = true
    }
    Toast {
        id: toast
        z: 5
        x: content.x + (content.width - width) / 2
        y: win.height - height - Theme.s6
    }
    Connections {
        target: Data
        function onToastRequested(text, kind) { toast.show(text, kind) }
        function onNavigate(page) { win.go(page) }
        function onNavigateSection(page, section) { win.go(page); win.pendingSection = section; sectionTimer.restart() }
        function onReleaseNotes() { notes.show(null) }
    }
    ReleaseNotesDialog { id: notes }

    // ---- minimise while the game runs, restore when it ends ------------------------------------------------
    property bool minimizedByUs: false
    // How the window was before we minimised it, so it comes back the same (`show()` keeps a minimised window minimised).
    property int visibilityBeforeMinimize: Window.Windowed
    Connections {
        target: LaunchModel
        function onStateChanged() {
            const st = LaunchModel.state
            if (st === "running" && Settings.minimizeWhilePlaying && !AppEnv.selftest && AppEnv.shotDir === "") {
                win.visibilityBeforeMinimize = win.visibility === Window.Maximized ? Window.Maximized : Window.Windowed
                win.minimizedByUs = true
                win.showMinimized()
            } else if (win.minimizedByUs && st !== "running" && st !== "launching") {
                win.minimizedByUs = false
                if (win.visibilityBeforeMinimize === Window.Maximized) win.showMaximized()
                else win.showNormal()
                win.raise()
                win.requestActivate()
            }
        }
    }
    Connections {
        target: GameModel
        function onErrorChanged() { if (GameModel.error !== "") toast.show(GameModel.error, "error") }
    }

    // ---- screenshot driver (skradle --shots <dir> [--prefix name]) -----------------------------------------------
    readonly property var shotNames: ["home-ready", "home-updating", "home-launching", "home-running", "home-error", "home-firstrun",
        "home-collapsed", "home-scrolled", "home-offline", "home-nonews", "home-noevents", "home-events-offline", "home-players-down",
        "home-notes", "home-notes-all", "home-notes-min", "servers", "servers-dialog", "mods", "mods-search", "mods-drop",
        "settings-general", "settings-game", "settings-files", "settings-graphics", "settings-graphics-running", "settings-integrations",
        "tools", "tools-web", "settings-about", "settings-reset", "toast-error", "toast-info", "focus", "focus-controls", "servers-menu", "settings-combo", "servers-myserver",
        "mods-selected", "mods-focus", "mods-tag", "mods-empty", "mods-nogame", "mods-confirm", "mods-issues", "mods-pack-update",
        "mods-pack-syncing", "mods-pack-report", "mods-pack-error", "mods-help",
        "mods-enabled", "mods-disabled", "mods-warning", "mods-combo", "mods-long", "mods-nomatch", "mods-scroll1", "mods-scroll2",
        "srv-notinstalled", "srv-offline", "srv-install", "srv-busy", "srv-nojava", "srv-stopped", "srv-starting", "srv-running", "srv-running-lan",
        "srv-failed", "srv-manage", "srv-portbusy", "srv-backup", "srv-console", "srv-add", "srv-add-errors", "srv-edit", "srv-info", "srv-delete", "srv-selected-menu"]
    property int shotIndex: 0
    property int shotsFailed: 0
    function nextShot() {
        if (shotIndex > 0) {
            const path = AppEnv.shotDir + "/" + AppEnv.shotPrefix + "-" + shotNames[shotIndex - 1] + ".png"
            if (!AppEnv.grab(path)) { shotsFailed++; AppEnv.report("failed to save " + path) }
        }
        if (shotIndex >= shotNames.length) {
            AppEnv.report("shots: " + (shotNames.length - shotsFailed) + " of " + shotNames.length + " saved to " + AppEnv.shotDir)
            Qt.exit(shotsFailed > 0 ? 1 : 0)
            return
        }
        setupShot(shotNames[shotIndex])
        shotIndex++
        shotTimer.restart()
    }
    Timer { id: shotTimer; interval: 850; onTriggered: win.nextShot() }
    Timer { running: AppEnv.shotDir !== ""; interval: 900; onTriggered: win.nextShot() }

    // The automatic update check: once, a moment after start, never in the self-test or screenshot runs.
    Timer {
        id: autoUpdate
        interval: 2500
        running: Settings.autoUpdate && !AppEnv.selftest && AppEnv.shotDir === "" && !autoUpdate.done
        property bool done: false
        onTriggered: { done = true; UpdateModel.checkAutomatically() }
    }

    function setupShot(name) {
        // Shots never depend on this machine: pretend a game was found and nothing is running.
        GameModel.force("ready")
        win.collapsed = false
        LaunchModel.force("ready", 0)
        const hp = pageItem(0); if (hp) hp.scroller.contentY = 0
        const sv = pageItem(1); if (sv) sv.closeAll()
        const gp = pageItem(4); if (gp) { gp.closeCombo(); gp.closeReset(); gp.scroller.contentY = 0 }
        const tp = pageItem(3); if (tp) tp.shot("")
        HomeFeed.force("ok"); HomeFeed.forceEvents("present"); HomeFeed.forcePlayers("ok")
        notes.close()
        const md = pageItem(2); if (md) md.resetShot()
        // The Servers page shows a fixed list and a fixed server state, never this computer's.
        ServerBook.force("list"); ServerHost.force("stopped")
        if (name.indexOf("srv-") === 0) { go(1); Qt.callLater(() => { const p = pageItem(1); if (p) p.shot(name.substring(4)) }); return }
        switch (name) {
        case "home-ready": go(0); break
        case "home-updating": go(0); LaunchModel.force("updating", 0); break
        case "home-launching": go(0); LaunchModel.force("launching", 0); break
        case "home-running": go(0); LaunchModel.force("running", 0); break
        case "home-error": go(0); LaunchModel.force("error", 0); break
        case "home-firstrun": go(0); GameModel.force("notfound"); break
        case "home-collapsed": go(0); win.collapsed = true; break
        case "home-scrolled": go(0); Qt.callLater(() => { const h = pageItem(0); if (h) h.scroller.contentY = Math.max(0, h.scroller.contentHeight - h.scroller.height) }); break
        case "servers": go(1); break
        case "servers-dialog": go(1); Qt.callLater(() => { const p = pageItem(1); if (p) p.openAdd() }); break
        case "mods": go(2); break
        case "mods-search": go(2); Qt.callLater(() => { const p = pageItem(2); if (p) p.setSearch("hud") }); break
        case "mods-drop": go(2); Qt.callLater(() => { const p = pageItem(2); if (p) p.dropForced = true }); break
        case "home-offline": go(0); HomeFeed.force("offline"); break
        case "home-nonews": go(0); HomeFeed.force("empty"); break
        case "home-noevents": go(0); HomeFeed.forceEvents("none"); break
        case "home-events-offline": go(0); HomeFeed.forceEvents("offline"); break
        case "home-players-down": go(0); HomeFeed.forcePlayers("unavailable"); break
        case "home-notes": go(0); Qt.callLater(() => notes.show(null)); break
        case "home-notes-all": go(0); Qt.callLater(() => { notes.show(null); notes.setAll(true, false) }); break
        case "home-notes-min": go(0); Qt.callLater(() => { notes.show(null); notes.setAll(false, false) }); break
        case "settings-general": go(4); break
        case "settings-game": go(4); Qt.callLater(() => pageItem(4).scrollTo(1)); break
        case "settings-files": go(4); Qt.callLater(() => pageItem(4).scrollTo(2)); break
        case "settings-graphics": go(4); Qt.callLater(() => pageItem(4).scrollTo(3)); break
        case "settings-graphics-running": go(4); LaunchModel.force("running", 0); Qt.callLater(() => pageItem(4).scrollTo(3)); break
        case "settings-integrations": go(4); Qt.callLater(() => pageItem(4).scrollTo(4)); break
        case "tools": go(3); Qt.callLater(() => { const p = pageItem(3); if (p) p.shot("") }); break
        case "tools-web": go(3); Qt.callLater(() => { const p = pageItem(3); if (p) p.shot("web") }); break
        case "settings-about": go(4); Qt.callLater(() => pageItem(4).scrollTo(5)); break
        case "toast-error": go(4); Qt.callLater(() => Data.toast(qsTr("Could not open the folder. Check that it still exists."), "error")); break
        case "toast-info": go(0); Qt.callLater(() => Data.toast(qsTr("Address copied"))); break
        case "settings-reset": go(4); Qt.callLater(() => pageItem(4).scrollTo(2)); Qt.callLater(() => pageItem(4).openReset()); break
        case "servers-menu": go(1); Qt.callLater(() => { const p = pageItem(1); if (p) p.openMenu() }); break
        case "settings-combo": go(4); Qt.callLater(() => pageItem(4).scrollTo(1)); Qt.callLater(() => { const p = pageItem(4); if (p) p.openCombo() }); break
        case "focus-controls": go(4); Qt.callLater(() => pageItem(4).scrollTo(1)); Qt.callLater(() => { const p = pageItem(4); if (p) p.focusSlider() }); break
        case "mods-selected": case "mods-focus": case "mods-tag": case "mods-empty": case "mods-confirm": case "mods-issues":
        case "mods-pack-update": case "mods-pack-syncing": case "mods-pack-report": case "mods-pack-error": case "mods-help":
        case "mods-enabled": case "mods-disabled": case "mods-warning": case "mods-combo": case "mods-long": case "mods-nomatch": case "mods-scroll1": case "mods-scroll2":
            go(2); Qt.callLater(() => { const p = pageItem(2); if (p) p.shotState(name.substring(5)) }); break
        case "mods-nogame": go(2); GameModel.force("notfound"); break
        case "servers-myserver": go(1); ServerHost.force("running"); Qt.callLater(() => { const p = pageItem(1); if (p) p.showHost = true }); break
        case "focus": go(1); Qt.callLater(() => side.focusItem(1)); break
        }
    }

    // ---- self-test driver (skradle --selftest): visit every page, check each one loaded, report, exit ------------
    property int selftestStep: 0
    property int selftestWait: 0
    property int notesStep: 0
    Timer {
        interval: 250
        repeat: true
        running: AppEnv.selftest
        onTriggered: {
            const i = win.selftestStep
            if (i === 0 && LaunchModel.state !== "ready") {
                AppEnv.finishSelftest(false, "LaunchModel starts in state '" + LaunchModel.state + "', not 'ready'")
                return
            }
            if (i === 0 && ["searching", "ready", "notfound"].indexOf(GameModel.state) < 0) {
                AppEnv.finishSelftest(false, "GameModel starts in state '" + GameModel.state + "'")
                return
            }
            if (i > 0 && !win.pageItem(i - 1)) {
                AppEnv.finishSelftest(false, "page " + (i - 1) + " did not load")
                return
            }
            if (i >= win.navItems.length) {
                // The bundled release notes open and have at least one version.
                if (win.notesStep === 0) { win.notesStep = 1; notes.show(null); return }
                if (win.notesStep === 1) {
                    win.notesStep = 2
                    const ok = notes.versions.length > 0 && notes.visible
                    notes.close()
                    if (!ok) { AppEnv.finishSelftest(false, "the release notes did not open or are empty"); return }
                    return
                }
                // Last: open "My server" and see whether it embeds a web view or falls back to the browser.
                const servers = win.pageItem(1)
                win.go(1)
                servers.showHost = true
                win.selftestWait++
                if ((servers.hostMode === "loading" || (servers.hostMode === "embedded" && servers.hostTitle === "")) && win.selftestWait < 80) return
                const mode = servers.hostMode
                const title = servers.hostTitle
                servers.showHost = false
                AppEnv.finishSelftest(mode !== "loading" && (mode !== "embedded" || title === "script ran"), "window and " + win.navItems.length + " pages loaded, Qt " + AppEnv.qtVersion + ", web view backend: " + AppEnv.webViewAvailable + ", my-server view: " + mode + (mode === "embedded" ? " (page title: " + title + ")" : ""))
                return
            }
            win.go(i)
            win.selftestStep = i + 1
        }
    }
}
