pragma Singleton
import QtQuick

// Fake launch state machine: ready -> updating -> launching -> running. Real work arrives later from the core.
QtObject {
    id: m
    property string state: "ready"
    property real target: 0            // fake download progress, advances in lumps like a real one
    property real shown: 0             // what the UI draws, eased toward target
    property bool snap: false
    readonly property real totalMB: 412
    readonly property real doneMB: shown * totalMB
    property real speed: 14.2
    property bool fast: false          // bench: shorter run

    Behavior on shown { enabled: !m.snap; NumberAnimation { duration: Motion.progress; easing.type: Easing.OutQuad } }

    function start() {
        if (state !== "ready") return
        snap = true; target = 0; shown = 0; snap = false
        state = "updating"
        tick.restart()
    }
    function cancel() { tick.stop(); launchTimer.stop(); state = "ready"; snap = true; target = 0; shown = 0; snap = false }
    function stop() { state = "ready" }
    function force(s, v) {   // screenshots
        tick.stop(); launchTimer.stop()
        snap = true; target = v; shown = v; snap = false
        state = s
    }
    onTargetChanged: if (!snap) shown = target
    onShownChanged: if (state === "updating" && shown >= 0.995) { tick.stop(); state = "launching"; launchTimer.restart() }

    property Timer tick: Timer {
        interval: 320
        repeat: true
        onTriggered: {
            m.target = Math.min(1, m.target + 0.025 + Math.random() * 0.055)
            m.speed = 11 + Math.random() * 7
            if (m.target >= 1) tick.stop()
        }
    }
    property Timer launchTimer: Timer {
        interval: 2400
        onTriggered: m.state = "running"
    }
}
