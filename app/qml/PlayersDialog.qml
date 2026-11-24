import QtQuick
import com.skradle.app
import QtQuick.Layouts

// Players online over time: the figures (now with its trend, the 24-hour peak, the all-time peak, the usual busiest
// hour of the day and of the week) and three charts (24 hours, 7 days, 30 days). When the Knight Launcher service
// answers (asked once when this opens) its recorded history is shown; otherwise the numbers are Steam's, with the
// standalone estimate added, recorded by SKradle on this computer while it is open (see core::player_history).
ServerDialog {
    id: dlg
    size: "reading"
    preferredWidth: 700
    showClose: true
    title: qsTr("Players online")
    subtitle: fromService ? qsTr("Player counts recorded by the Knight Launcher service.") : qsTr("Steam's player counts, with an estimate for the standalone client added.")

    property var snap: ({})
    readonly property bool fromService: HomeFeed.remoteHistory !== ""
    property real now: Date.now() / 1000
    function load() {
        now = Date.now() / 1000
        try { snap = JSON.parse(fromService ? HomeFeed.remoteHistory : HomeFeed.historyJson(new Date().getTimezoneOffset())) } catch (e) { snap = ({}) }
    }
    Connections { target: HomeFeed; function onRemoteHistoryChanged() { dlg.load() } }
    function num(n) { return n === undefined || n === null ? "–" : Number(n).toLocaleString(Qt.locale(), "f", 0) }
    function hourText(h) { return h === undefined || h === null ? "–" : (h < 10 ? "0" : "") + h + ":00" }
    function weekText(w) { return w ? Qt.locale().dayName(w.weekday % 7, Locale.LongFormat) + " " + hourText(w.hour) : "–" }
    readonly property string trendText: snap.trend > 0 ? qsTr("Trending up over the last hour.") : snap.trend < 0 ? qsTr("Trending down over the last hour.") : ""

    onAboutToShow: { load(); HomeFeed.refreshHistory(new Date().getTimezoneOffset()) }
    onFocusFirst: closeButton.forceActiveFocus()
    Timer { interval: 60000; repeat: true; running: dlg.visible; onTriggered: dlg.load() }

    Txt {
        Layout.fillWidth: true
        kind: "caption"
        paragraph: true
        text: dlg.fromService
            ? qsTr("The Knight Launcher service records the player count of the official server around the clock, so these charts are full from the start. Times are in your time zone.")
            : qsTr("Each Steam count is multiplied by %1 to include players of the standalone client, so the figures are close to, not exactly, the real numbers. SKradle records a point every few minutes while it is open, so the charts fill in over time. Times are in your time zone.").arg(Number(1.4).toLocaleString(Qt.locale(), "f", 1))
    }

    GridLayout {
        Layout.fillWidth: true
        columns: dlg.width > 620 ? 3 : 2
        columnSpacing: Theme.s2
        rowSpacing: Theme.s2
        Repeater {
            model: [
                { caption: qsTr("Online now"), value: dlg.num(dlg.snap.current), hint: dlg.trendText },
                { caption: qsTr("24-hour peak"), value: dlg.num(dlg.snap.day_peak), hint: "" },
                { caption: qsTr("Highest recorded"), value: dlg.num(dlg.snap.all_time_peak ? dlg.snap.all_time_peak.n : null),
                  hint: dlg.snap.all_time_peak && dlg.snap.all_time_peak.t > 0 ? Qt.formatDateTime(new Date(dlg.snap.all_time_peak.t * 1000), "d MMM HH:mm") : "" },
                { caption: qsTr("Usual daily peak"), value: dlg.hourText(dlg.snap.usual_day_peak_hour), hint: dlg.snap.usual_day_peak_hour === undefined || dlg.snap.usual_day_peak_hour === null ? qsTr("Needs two days of data.") : "" },
                { caption: qsTr("Usual weekly peak"), value: dlg.weekText(dlg.snap.usual_week_peak), hint: dlg.snap.usual_week_peak ? "" : qsTr("Needs a week of data.") }
            ]
            Card {
                id: stat
                required property var modelData
                Layout.fillWidth: true
                Layout.fillHeight: true
                Layout.preferredWidth: 1
                elevation: 0
                implicitHeight: statCol.implicitHeight + 2 * Theme.s3
                Accessible.role: Accessible.StaticText
                Accessible.name: stat.modelData.caption + ": " + stat.modelData.value
                ColumnLayout {
                    id: statCol
                    anchors { left: parent.left; right: parent.right; top: parent.top; margins: Theme.s3 }
                    spacing: Theme.s0
                    Txt { Layout.fillWidth: true; text: stat.modelData.caption; kind: "caption"; wrapMode: Text.NoWrap; elide: Text.ElideRight }
                    Txt { Layout.fillWidth: true; text: stat.modelData.value; kind: "heading"; numeric: true; wrapMode: Text.NoWrap; elide: Text.ElideRight }
                    Txt { Layout.fillWidth: true; visible: text !== ""; text: stat.modelData.hint; kind: "micro"; color: Theme.textSecondary; paragraph: true }
                }
            }
        }
    }

    TimeChart { Layout.fillWidth: true; title: qsTr("Last 24 hours"); points: dlg.snap.day || []; labels: "hour"; span: 86400; now: dlg.now }
    TimeChart { Layout.fillWidth: true; title: qsTr("Last 7 days"); points: dlg.snap.week || []; labels: "weekday"; span: 7 * 86400; now: dlg.now }
    TimeChart { Layout.fillWidth: true; title: qsTr("Last 30 days"); points: dlg.snap.month || []; labels: "date"; span: 30 * 86400; now: dlg.now }
}
