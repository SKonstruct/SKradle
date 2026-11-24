import QtQuick
import com.skradle.app
import QtQuick.Layouts

// A small area chart of players over time, drawn on a Canvas in the theme's colours: a titled card with three grid
// lines, time labels along the bottom, and the busiest point marked. `points` is a list of [seconds since 1970, count].
// With fewer than three points it says there is not enough data yet instead of drawing nothing.
Card {
    id: root
    property string title
    property var points: []
    // "hour" (HH:mm), "weekday" (Mon) or "date" (23 Nov): how the time labels read.
    property string labels: "hour"
    // Seconds the chart spans, ending now; fixes the left edge so a young history sits at the right, as it should.
    property real span: 86400
    property real now: Date.now() / 1000
    readonly property bool enough: points.length >= 3
    readonly property var peak: {
        let best = null
        for (const p of points) if (best === null || p[1] > best[1]) best = p
        return best
    }
    // The plot area inside the canvas, in pixels (the canvas and the hover marker share it).
    readonly property real padLeft: 40
    readonly property real padRight: 6
    readonly property real padTop: 6
    readonly property real padBottom: 20
    // Index into `points` of the point under the pointer, -1 when none.
    property int hoverIndex: -1
    elevation: 0
    implicitHeight: col.implicitHeight + 2 * Theme.s3
    Accessible.role: Accessible.Graphic
    Accessible.name: enough ? qsTr("%1: busiest %2 players").arg(title).arg(peak ? peak[1] : 0) : qsTr("%1: not enough data yet").arg(title)

    function stamp(sec) {
        const d = new Date(sec * 1000)
        return labels === "weekday" ? Qt.formatDate(d, "ddd") : labels === "date" ? Qt.formatDate(d, "d MMM") : Qt.formatTime(d, "HH:mm")
    }
    function niceMax(v) {
        if (v <= 10) return 10
        const mag = Math.pow(10, Math.floor(Math.log10(v)))
        for (const m of [1, 1.5, 2, 2.5, 3, 4, 5, 6, 8, 10]) if (m * mag >= v) return m * mag
        return 10 * mag
    }
    readonly property real topN: {
        let max = 0
        for (const p of points) max = Math.max(max, p[1])
        return niceMax(max * 1.08)
    }
    function xAt(t, w) {
        const t0 = now - span
        return padLeft + Math.max(0, Math.min(1, (t - t0) / span)) * (w - padLeft - padRight)
    }
    function yAt(n, h) {
        const ph = h - padTop - padBottom
        return padTop + ph - (n / topN) * ph
    }
    // The point nearest in time to the pointer at x, or -1 outside the plot.
    function indexAt(x, w) {
        if (!enough || x < padLeft || x > w - padRight) return -1
        const t = (now - span) + (x - padLeft) / (w - padLeft - padRight) * span
        let best = -1, dist = Infinity
        for (let i = 0; i < points.length; i++) {
            const d = Math.abs(points[i][0] - t)
            if (d < dist) { dist = d; best = i }
        }
        return best
    }
    onPointsChanged: { hoverIndex = -1; canvas.requestPaint() }
    Connections { target: Theme; function onDarkChanged() { canvas.requestPaint() } }

    ColumnLayout {
        id: col
        anchors { left: parent.left; right: parent.right; top: parent.top; margins: Theme.s3 }
        spacing: Theme.s2
        RowLayout {
            Layout.fillWidth: true
            Txt { Layout.fillWidth: true; text: root.title; font.weight: Font.DemiBold; Accessible.role: Accessible.Heading }
            Txt {
                visible: root.enough && root.peak !== null
                kind: "caption"
                numeric: true
                text: root.peak ? qsTr("Busiest: %1 (%2)").arg(Number(root.peak[1]).toLocaleString(Qt.locale(), "f", 0)).arg(root.stamp(root.peak[0])) : ""
            }
        }
        Item {
            Layout.fillWidth: true
            Layout.preferredHeight: 150
            Canvas {
                id: canvas
                anchors.fill: parent
                visible: root.enough
                onWidthChanged: requestPaint()
                onHeightChanged: requestPaint()
                onPaint: {
                    const ctx = getContext("2d")
                    ctx.reset()
                    const w = width, h = height
                    const left = 40, right = 6, top = 6, bottom = 20
                    const pw = w - left - right, ph = h - top - bottom
                    if (pw <= 0 || ph <= 0 || !root.enough) return
                    const t1 = root.now
                    const t0 = t1 - root.span
                    let max = 0
                    for (const p of root.points) max = Math.max(max, p[1])
                    const topN = root.niceMax(max * 1.08)
                    const x = (t) => left + Math.max(0, Math.min(1, (t - t0) / (t1 - t0))) * pw
                    const y = (n) => top + ph - (n / topN) * ph
                    ctx.font = "11px sans-serif"
                    // grid and the count labels
                    ctx.lineWidth = 1
                    ctx.strokeStyle = Theme.border
                    ctx.fillStyle = Theme.textTertiary
                    ctx.textAlign = "right"
                    ctx.textBaseline = "middle"
                    for (let i = 0; i <= 2; i++) {
                        const n = topN * i / 2
                        const gy = Math.round(y(n)) + 0.5
                        ctx.beginPath(); ctx.moveTo(left, gy); ctx.lineTo(w - right, gy); ctx.stroke()
                        ctx.fillText(Math.round(n).toString(), left - 6, gy)
                    }
                    // time labels
                    ctx.textBaseline = "alphabetic"
                    const ticks = pw > 420 ? 5 : 4
                    for (let i = 0; i < ticks; i++) {
                        const f = i / (ticks - 1)
                        ctx.textAlign = i === 0 ? "left" : i === ticks - 1 ? "right" : "center"
                        ctx.fillText(root.stamp(t0 + f * (t1 - t0)), left + f * pw, h - 4)
                    }
                    // area and line
                    const pts = root.points
                    ctx.beginPath()
                    ctx.moveTo(x(pts[0][0]), y(0))
                    for (const p of pts) ctx.lineTo(x(p[0]), y(p[1]))
                    ctx.lineTo(x(pts[pts.length - 1][0]), y(0))
                    ctx.closePath()
                    ctx.fillStyle = Theme.accentTint
                    ctx.fill()
                    ctx.beginPath()
                    ctx.moveTo(x(pts[0][0]), y(pts[0][1]))
                    for (const p of pts) ctx.lineTo(x(p[0]), y(p[1]))
                    ctx.lineWidth = 2
                    ctx.lineJoin = "round"
                    ctx.strokeStyle = Theme.accentFg
                    ctx.stroke()
                    // the busiest point
                    if (root.peak) {
                        ctx.beginPath()
                        ctx.arc(x(root.peak[0]), y(root.peak[1]), 3.5, 0, Math.PI * 2)
                        ctx.fillStyle = Theme.accentFg
                        ctx.fill()
                    }
                }
            }
            HoverHandler {
                id: hover
                enabled: root.enough
                cursorShape: Qt.CrossCursor
                onPointChanged: root.hoverIndex = root.indexAt(point.position.x, parent.width)
                onHoveredChanged: if (!hovered) root.hoverIndex = -1
            }
            // The guide line, the marked point and a tip with the count and the time under the pointer.
            Item {
                id: hoverLayer
                anchors.fill: parent
                visible: root.hoverIndex >= 0 && root.hoverIndex < root.points.length
                readonly property var pt: visible ? root.points[root.hoverIndex] : null
                readonly property real px: pt ? root.xAt(pt[0], width) : 0
                readonly property real py: pt ? root.yAt(pt[1], height) : 0
                Rectangle {
                    x: hoverLayer.px - 0.5; y: root.padTop
                    width: 1; height: hoverLayer.height - root.padTop - root.padBottom
                    color: Theme.textTertiary; opacity: 0.7
                }
                Rectangle {
                    x: hoverLayer.px - 5; y: hoverLayer.py - 5
                    width: 10; height: 10; radius: 5
                    color: Theme.accentFg; border.width: 2; border.color: Theme.card
                }
                Rectangle {
                    id: tip
                    readonly property bool flip: hoverLayer.px > hoverLayer.width / 2
                    x: flip ? hoverLayer.px - width - 12 : hoverLayer.px + 12
                    y: Math.max(0, Math.min(hoverLayer.height - root.padBottom - height, hoverLayer.py - height / 2))
                    width: tipLabel.implicitWidth + 2 * Theme.s2
                    height: tipLabel.implicitHeight + 2 * Theme.s1
                    radius: 6
                    color: Theme.tipBg; border.width: 1; border.color: Theme.tipBorder
                    Txt {
                        id: tipLabel
                        anchors.centerIn: parent
                        kind: "caption"
                        numeric: true
                        wrapMode: Text.NoWrap
                        color: Theme.tipText
                        text: hoverLayer.pt
                            ? qsTr("%n players", "", hoverLayer.pt[1]) + "\n" + Qt.formatDateTime(new Date(hoverLayer.pt[0] * 1000), "ddd d MMM HH:mm")
                            : ""
                    }
                }
            }
            Txt {
                anchors.centerIn: parent
                visible: !root.enough
                text: qsTr("Not enough data yet.")
                color: Theme.textSecondary
            }
        }
    }
}
