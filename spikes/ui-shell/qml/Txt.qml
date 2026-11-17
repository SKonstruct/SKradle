import QtQuick

// The only text element: kind picks size/weight/colour from the type scale.
Text {
    id: root
    property string kind: "body"   // display | title | heading | body | caption | micro
    readonly property var sizes: ({ display: Theme.fsDisplay, title: Theme.fsTitle, heading: Theme.fsHeading,
                                    body: Theme.fsBody, caption: Theme.fsCaption, micro: Theme.fsMicro })
    font.pixelSize: sizes[kind]
    font.weight: kind === "display" || kind === "title" ? Font.DemiBold
               : kind === "heading" ? Font.DemiBold
               : kind === "micro" ? Font.Medium : Font.Normal
    color: kind === "caption" || kind === "micro" ? Theme.textSecondary : Theme.text
    textFormat: Text.PlainText
    wrapMode: Text.WordWrap
    lineHeight: kind === "display" || kind === "title" ? 1.1 : 1.22
    lineHeightMode: Text.ProportionalHeight
}
