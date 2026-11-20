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
    lineHeight: lines[kind]
    lineHeightMode: Text.ProportionalHeight
}
