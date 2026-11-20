import QtQuick
import com.skradle.app
import QtQuick.Controls
import QtQuick.Layouts

// One release in the release-notes viewer: a header that folds the block open and closed, and its sections.
// `version` is one entry of ReleaseNotes.json; the text of every item is escaped rich text from the Rust parser.
Rectangle {
    id: root
    property var version: ({})
    property bool expanded: false

    Layout.fillWidth: true
    implicitHeight: col.implicitHeight
    radius: Theme.rMd
    color: "transparent"
    border.width: 1
    border.color: Theme.border

    readonly property string heading: version.title === "Unreleased" ? qsTr("Unreleased") : qsTr("Version %1").arg(version.title)
    readonly property int itemCount: {
        let n = 0
        for (const s of version.sections || []) n += s.items.length
        return n
    }

    // Rich text to what a screen reader should say.
    function plain(html) {
        return html.replace(/<[^>]*>/g, "").replace(/&lt;/g, "<").replace(/&gt;/g, ">").replace(/&quot;/g, "\"").replace(/&amp;/g, "&")
    }
    function openLink(url) { if (url.indexOf("https://") === 0) Qt.openUrlExternally(url) }

    ColumnLayout {
        id: col
        anchors.left: parent.left; anchors.right: parent.right; anchors.top: parent.top
        spacing: 0

        AbstractButton {
            id: head
            Layout.fillWidth: true
            implicitHeight: 48
            hoverEnabled: true
            focusPolicy: Qt.StrongFocus
            property int radius: Theme.rMd
            Accessible.role: Accessible.Button
            Accessible.name: root.expanded ? qsTr("%1, shown. Press to minimise.").arg(root.heading) : qsTr("%1, hidden. Press to maximise.").arg(root.heading)
            onClicked: root.expanded = !root.expanded
            background: Rectangle {
                radius: Theme.rMd
                color: head.down ? Theme.pressTint : head.hovered ? Theme.hoverTint : "transparent"
                Behavior on color { ColorAnimation { duration: Motion.fast } }
            }
            contentItem: RowLayout {
                spacing: Theme.s3
                Item {
                    Layout.leftMargin: Theme.s3
                    implicitWidth: 18; implicitHeight: 18
                    Icon {
                        anchors.centerIn: parent
                        name: "chevron-down"; size: 18; color: Theme.textSecondary
                        rotation: root.expanded ? 0 : -90
                        Behavior on rotation { NumberAnimation { duration: Motion.fast } }
                    }
                }
                Txt { text: root.heading; kind: "heading"; wrapMode: Text.NoWrap }
                Txt {
                    visible: !!root.version.date
                    text: root.version.date || ""
                    kind: "caption"; wrapMode: Text.NoWrap
                }
                Item { Layout.fillWidth: true }
                Txt {
                    Layout.rightMargin: Theme.s3
                    text: root.expanded ? qsTr("Minimise") : qsTr("Maximise")
                    kind: "caption"; wrapMode: Text.NoWrap
                }
            }
            FocusRing {}
        }

        ColumnLayout {
            id: body
            Layout.fillWidth: true
            Layout.leftMargin: Theme.s5; Layout.rightMargin: Theme.s5
            Layout.bottomMargin: Theme.s4
            visible: root.expanded
            spacing: Theme.s2

            Repeater {
                model: root.version.intro || []
                Txt {
                    required property string modelData
                    Layout.fillWidth: true
                    text: modelData
                    textFormat: Text.RichText
                    linkColor: Theme.accentFg
                    onLinkActivated: (url) => root.openLink(url)
                    Accessible.name: root.plain(modelData)
                }
            }
            Repeater {
                model: root.version.sections || []
                ColumnLayout {
                    id: sec
                    required property var modelData
                    Layout.fillWidth: true
                    Layout.topMargin: Theme.s2
                    spacing: Theme.s2
                    Txt {
                        Layout.fillWidth: true
                        visible: text !== ""
                        text: sec.modelData.name
                        kind: "caption"
                        color: Theme.accentFg
                        font.weight: Font.DemiBold
                        Accessible.role: Accessible.Heading
                    }
                    Repeater {
                        model: sec.modelData.items
                        RowLayout {
                            id: line
                            required property var modelData
                            Layout.fillWidth: true
                            Layout.leftMargin: line.modelData.level * Theme.s5
                            spacing: Theme.s2
                            Txt {
                                Layout.alignment: Qt.AlignTop
                                visible: line.modelData.bullet
                                text: line.modelData.level > 0 ? "–" : "•"
                                color: Theme.textTertiary
                                wrapMode: Text.NoWrap
                                Accessible.ignored: true
                            }
                            Txt {
                                id: itemText
                                Layout.fillWidth: true
                                text: line.modelData.html
                                textFormat: Text.RichText
                                linkColor: Theme.accentFg
                                onLinkActivated: (url) => root.openLink(url)
                                Accessible.name: root.plain(line.modelData.html)
                                HoverHandler { cursorShape: itemText.hoveredLink !== "" ? Qt.PointingHandCursor : Qt.ArrowCursor }
                            }
                        }
                    }
                }
            }
        }
    }
}
