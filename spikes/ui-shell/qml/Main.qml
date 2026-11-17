import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

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

    property int current: AppEnv.initialPage
    property int dir: 1
    property bool collapsed: AppEnv.initialCollapsed
    readonly property var navItems: [
        { label: L.t("Home"), icon: "home" },
        { label: L.t("Servers"), icon: "servers" },
        { label: L.t("Mods"), icon: "mods" },
        { label: L.t("Graphics"), icon: "graphics" },
        { label: L.t("About"), icon: "info" }
    ]
    function go(i) {
        if (i === current || i < 0 || i >= navItems.length) return
        dir = i > current ? 1 : -1
        current = i
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
            model: 5
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
                    sourceComponent: [homeC, serversC, modsC, graphicsC, aboutC][host.index]
                }
            }
        }
        Component { id: homeC; HomePage {} }
        Component { id: serversC; ServersPage {} }
        Component { id: modsC; ModsPage {} }
        Component { id: graphicsC; GraphicsPage {} }
        Component { id: aboutC; AboutPage {} }
    }

    Toast {
        id: toast
        z: 5
        x: content.x + (content.width - width) / 2
        y: win.height - height - 28
    }
    Connections {
        target: Data
        function onToast(text) { toast.show(text) }
        function onNavigate(page) { win.go(page) }
    }

    // ---- screenshot driver -----------------------------------------------------------------------------------
    readonly property var shotNames: ["home-ready", "home-updating", "home-launching", "home-running", "home-firstrun",
        "home-collapsed", "home-scrolled", "servers", "servers-dialog", "mods", "mods-search", "mods-drop", "graphics", "graphics-running",
        "about", "focus", "focus-controls", "servers-menu", "graphics-combo", "mods-toggle"]
    function setupShot(name) {
        Data.gameFound = true
        win.collapsed = false
        LaunchModel.force("ready", 0)
        const hp = pageItem(0); if (hp) hp.scroller.contentY = 0
        const sv = pageItem(1); if (sv) sv.closeAll()
        const gp = pageItem(3); if (gp) gp.closeCombo()
        const md = pageItem(2); if (md) { md.dropForced = false; md.setSearch(""); md.filter = "all" }
        switch (name) {
        case "home-ready": go(0); break
        case "home-updating": go(0); LaunchModel.force("updating", 0.58); break
        case "home-launching": go(0); LaunchModel.force("launching", 1); break
        case "home-running": go(0); LaunchModel.force("running", 1); break
        case "home-firstrun": go(0); Data.gameFound = false; break
        case "home-collapsed": go(0); win.collapsed = true; break
        case "home-scrolled": go(0); Qt.callLater(() => { const h = pageItem(0); if (h) h.scroller.contentY = Math.max(0, h.scroller.contentHeight - h.scroller.height) }); break
        case "servers": go(1); break
        case "servers-dialog": go(1); Qt.callLater(() => { const p = pageItem(1); if (p) p.openAdd() }); break
        case "mods": go(2); break
        case "mods-search": go(2); Qt.callLater(() => { const p = pageItem(2); if (p) p.setSearch("hud") }); break
        case "mods-drop": go(2); Qt.callLater(() => { const p = pageItem(2); if (p) p.dropForced = true }); break
        case "graphics": go(3); break
        case "graphics-running": go(3); LaunchModel.force("running", 1); break
        case "about": go(4); break
        case "servers-menu": go(1); Qt.callLater(() => { const p = pageItem(1); if (p) p.openMenu() }); break
        case "graphics-combo": go(3); Qt.callLater(() => { const p = pageItem(3); if (p) p.openCombo() }); break
        case "focus-controls": go(3); Qt.callLater(() => { const p = pageItem(3); if (p) p.focusSlider() }); break
        case "mods-toggle": go(2); Qt.callLater(() => { const p = pageItem(2); if (p) { p.setOn(4, true); p.setOn(0, false) } }); break
        case "focus": go(1); Qt.callLater(() => side.focusItem(1)); break
        }
    }

    // ---- benchmark driver ------------------------------------------------------------------------------------
    Rectangle {   // keeps frames flowing so every measured interval is a real frame
        visible: AppEnv.bench
        width: 2; height: 2; x: 0; y: 0; color: Theme.bg
        RotationAnimator on rotation { from: 0; to: 360; duration: 700; loops: Animation.Infinite; running: AppEnv.bench }
    }
    property int benchStep: 0
    Timer {
        id: benchTimer
        interval: 450
        repeat: true
        onTriggered: {
            win.benchStep++
            if (win.benchStep <= 9) win.go(win.benchStep % 5)
            else if (win.benchStep <= 14) win.collapsed = !win.collapsed
            else {
                stop()
                win.collapsed = false
                win.go(0)
                AppEnv.benchPhase("launch-updating")
                LaunchModel.start()
            }
        }
    }
    Connections {
        target: LaunchModel
        enabled: AppEnv.bench
        function onStateChanged() {
            if (LaunchModel.state === "launching") AppEnv.benchPhase("launch-indeterminate")
            else if (LaunchModel.state === "running") AppEnv.benchFinish()
        }
    }
    Timer {
        interval: 1200
        running: AppEnv.bench
        onTriggered: {
            win.go(0)
            AppEnv.benchPhase("warmup")
            benchTimer.start()
            AppEnv.benchPhase("page-transitions")
            // phase boundaries inside the timer chain: pages (steps 1-9), then sidebar collapse (10-14)
            collapsePhase.start()
        }
    }
    Timer { id: collapsePhase; interval: 450 * 9.5; onTriggered: AppEnv.benchPhase("sidebar-collapse") }
}
