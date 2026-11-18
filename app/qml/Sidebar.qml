import QtQuick
import com.skradle.app
import QtQuick.Controls
import QtQuick.Layouts

// Custom-drawn navigation rail. The selection pill is one item whose top and bottom edges animate with different
// durations, so it stretches toward the target and then settles (a "morph"), never a cross-fade.
Rectangle {
    id: root
    property int current: 0
    property bool collapsed: false
    readonly property int expandedWidth: 224
    readonly property int collapsedWidth: 72
    readonly property int targetWidth: collapsed ? collapsedWidth : expandedWidth
    property var items: []            // [{ label, icon }]
    property var itemRefs: []
    signal activated(int index)
    signal toggleCollapsed()

    width: targetWidth
    color: Theme.sidebar
    clip: true
    Behavior on width { NumberAnimation { duration: Motion.slow; easing.type: Easing.BezierSpline; easing.bezierCurve: Motion.standard } }

    Rectangle {  // hairline on the content side
        anchors.right: parent.right
        width: 1; height: parent.height
        color: Theme.border
    }

    // brand row
    Item {
        id: brand
        x: 12; y: 18
        width: root.targetWidth - 24; height: 44
        Logo { x: 8; anchors.verticalCenter: parent.verticalCenter; size: 32 }
        Txt {
            x: 8 + 32 + 12
            anchors.verticalCenter: parent.verticalCenter
            text: "SKradle"
            kind: "heading"
            wrapMode: Text.NoWrap
            opacity: root.collapsed ? 0 : 1
            Behavior on opacity { NumberAnimation { duration: root.collapsed ? Motion.instant : Motion.base } }
        }
    }

    // pill
    property real pillTop: 0
    property real pillBottom: 0
    property int topDur: 0
    property int botDur: 0
    property bool pillReady: false
    Behavior on pillTop { enabled: root.pillReady; NumberAnimation { duration: root.topDur; easing.type: Easing.BezierSpline; easing.bezierCurve: Motion.standard } }
    Behavior on pillBottom { enabled: root.pillReady; NumberAnimation { duration: root.botDur; easing.type: Easing.BezierSpline; easing.bezierCurve: Motion.standard } }

    function syncPill() {
        const it = rep.itemAt(current)
        if (!it) return
        const top = navCol.y + it.y
        const bottom = top + it.height
        const down = top > pillTop
        topDur = down ? Motion.pillTrail : Motion.pillLead
        botDur = down ? Motion.pillLead : Motion.pillTrail
        pillTop = top
        pillBottom = bottom
    }
    onCurrentChanged: syncPill()
    Component.onCompleted: { syncPill(); pillReady = true }

    Item {
        id: pill
        x: navCol.x
        y: root.pillTop
        width: navCol.width
        height: root.pillBottom - root.pillTop
        Rectangle {
            anchors.fill: parent
            radius: Theme.rMd
            color: Theme.accentTint
        }
        Rectangle {   // accent bar rides along with the pill
            x: 0
            width: 3; radius: 1.5
            height: Math.min(parent.height - 16, 20)
            anchors.verticalCenter: parent.verticalCenter
            color: Theme.accent
        }
    }

    ColumnLayout {
        id: navCol
        x: 12; y: 88
        width: root.targetWidth - 24
        spacing: 4
        Repeater {
            id: rep
            model: root.items
            NavItem {
                required property int index
                required property var modelData
                Layout.fillWidth: true
                text: modelData.label
                glyph: modelData.icon
                selected: root.current === index
                collapsed: root.collapsed
                onClicked: root.activated(index)
                Keys.onUpPressed: root.focusItem(index - 1)
                Keys.onDownPressed: root.focusItem(index + 1)
                Keys.onReturnPressed: root.activated(index)
            }
        }
    }

    function focusItem(i) {
        const n = items.length
        const idx = (i + n) % n
        const it = rep.itemAt(idx)
        if (it) { it.forceActiveFocus(Qt.TabFocusReason); activated(idx) }
    }

    IconButton {
        x: root.collapsed ? (root.collapsedWidth - width) / 2 : 12 + 6
        anchors.bottom: parent.bottom
        anchors.bottomMargin: 16
        glyph: "panel"
        tip: root.collapsed ? L.t("Expand sidebar") : L.t("Collapse sidebar")
        onClicked: root.toggleCollapsed()
        Behavior on x { NumberAnimation { duration: Motion.slow; easing.type: Easing.BezierSpline; easing.bezierCurve: Motion.standard } }
    }
}
