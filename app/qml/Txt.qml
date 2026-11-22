import QtQuick
import com.skradle.app

// The only text element: kind picks size/weight/colour/line height from the type scale.
// numeric: tabular figures (counts, ping, sizes, ports). mono: fixed pitch (addresses, paths).
Text {
    id: root
    property string kind: "body"   // display | title | heading | body | caption | micro
    property bool numeric: false
    property bool mono: false
    readonly property var sizes: ({ display: Theme.fsDisplay, title: Theme.fsTitle, heading: Theme.fsHeading,
                                    body: Theme.fsBody, caption: Theme.fsCaption, micro: Theme.fsMicro })
    readonly property var lines: ({ display: 1.1, title: 1.15, heading: 1.25, body: 1.35, caption: 1.3, micro: 1.2 })
    font.pixelSize: sizes[kind]
    font.weight: kind === "display" || kind === "title" || kind === "heading" ? Font.DemiBold
               : kind === "micro" ? Font.Medium : Font.Normal
    font.features: numeric ? { "tnum": 1 } : ({})
    font.family: mono ? Theme.monoFamily : Qt.application.font.family
    color: kind === "caption" || kind === "micro" ? Theme.textSecondary : Theme.text
    textFormat: Text.PlainText
    wrapMode: Text.WordWrap
    // Centring never depends on font metrics: Qt adds proportional leading above the first line, which pushed
    // single-line labels down on fonts with large line gaps. So the default is the font's natural line height (1);
    // only copy that wraps over several lines opts in to the looser `paragraph` leading. Give a label the full
    // height of its parent and it centres exactly.
    property bool paragraph: false
    lineHeight: paragraph ? lines[kind] : 1
    lineHeightMode: Text.ProportionalHeight
    verticalAlignment: Text.AlignVCenter
}
