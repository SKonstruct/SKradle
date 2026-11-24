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
    onPointsChanged: canvas.requestPaint()
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
            Txt {
                anchors.centerIn: parent
                visible: !root.enough
                text: qsTr("Not enough data yet.")
                color: Theme.textSecondary
            }
        }
    }
}
