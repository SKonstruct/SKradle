import QtQuick
import com.skradle.app
import QtQuick.Controls
import QtQuick.Layouts

// One release in the release-notes viewer: a header that folds the block open and closed (the dialog keeps the state),
// then its notes in one reading column. `version` is one entry of ReleaseNotes.json; the text of every item is escaped
// rich text from the Rust parser.
Item {
    id: root
    property var version: ({})
    property bool expanded: false
    property bool first: false
    signal toggled()

    Layout.fillWidth: true
    implicitHeight: col.implicitHeight

    readonly property bool unreleased: version.title === "Unreleased"
    readonly property string heading: unreleased ? qsTr("Next version") : qsTr("Version %1").arg(version.title)
    // The changelog's section names, in the words a player expects.
    function sectionLabel(name) {
        switch (name) {
        case "Added": return qsTr("New")
        case "Changed": return qsTr("Changed")
        case "Fixed": return qsTr("Fixed")
        case "Removed": return qsTr("Removed")
        case "Security": return qsTr("Security")
        }
        return name
    }

    // Rich text to what a screen reader should say.
    function plain(html) {
        return html.replace(/<[^>]*>/g, "").replace(/&lt;/g, "<").replace(/&gt;/g, ">").replace(/&quot;/g, "\"").replace(/&amp;/g, "&")
    }
    function linksOf(html) {
        const out = []
        const re = /href="([^"]+)"/g
        let m
        while ((m = re.exec(html)) !== null) out.push(m[1].replace(/&amp;/g, "&"))
        return out
    }
    function openLink(url) { if (url.indexOf("https://") === 0) Qt.openUrlExternally(url) }

    // One paragraph or bullet. A text with links is a tab stop: Enter opens the first link, Left and Right pick another.
    component Line: FocusScope {
        id: line
        property string html
        property bool bullet: false
        property int level: 0
        readonly property var links: root.linksOf(html)
        property int pick: 0
        Layout.fillWidth: true
        Layout.leftMargin: level * Theme.s4
        implicitHeight: lineRow.implicitHeight
        activeFocusOnTab: links.length > 0
        Accessible.role: links.length > 0 ? Accessible.Link : Accessible.StaticText
        Accessible.name: root.plain(html)
        Accessible.description: links.length > 0 ? qsTr("Press Enter to open %1").arg(links[Math.min(pick, links.length - 1)]) : ""
        Keys.onPressed: (event) => {
            if (links.length === 0) return
            if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter || event.key === Qt.Key_Space) {
                root.openLink(links[Math.min(pick, links.length - 1)]); event.accepted = true
            } else if (event.key === Qt.Key_Right) { pick = (pick + 1) % links.length; event.accepted = true }
            else if (event.key === Qt.Key_Left) { pick = (pick + links.length - 1) % links.length; event.accepted = true }
        }
        RowLayout {
            id: lineRow
            anchors { left: parent.left; right: parent.right }
            spacing: Theme.s2
            Txt {
                Layout.alignment: Qt.AlignTop
                visible: line.bullet
                text: line.level > 0 ? "–" : "•"
                lineHeight: 1.5
                color: Theme.textSecondary
                wrapMode: Text.NoWrap
                Accessible.ignored: true
            }
            Txt {
                id: itemText
                Layout.fillWidth: true
                text: line.html
                textFormat: Text.RichText
                lineHeight: 1.5
                linkColor: Theme.accentFg
                onLinkActivated: (url) => root.openLink(url)
                Accessible.ignored: true
                HoverHandler { cursorShape: itemText.hoveredLink !== "" ? Qt.PointingHandCursor : Qt.ArrowCursor }
            }
        }
        FocusRing { active: line.activeFocus && line.links.length > 0 }
    }

    ColumnLayout {
        id: col
        anchors.left: parent.left; anchors.right: parent.right; anchors.top: parent.top
        spacing: 0

        Divider { visible: !root.first }

        AbstractButton {
            id: head
            Layout.fillWidth: true
            implicitHeight: Theme.hLg
            hoverEnabled: true
            focusPolicy: Qt.StrongFocus
            Accessible.role: Accessible.Button
            Accessible.name: root.heading
            Accessible.description: root.expanded ? qsTr("Expanded. Press to collapse.") : qsTr("Collapsed. Press to expand.")
            onClicked: root.toggled()
            background: Rectangle {
                radius: Theme.rSm
                color: head.down ? Theme.statePress : head.hovered ? Theme.stateHover : "transparent"
                Behavior on color { ColorAnimation { duration: Motion.fast } }
            }
            contentItem: RowLayout {
                spacing: Theme.s2
                Icon {
                    Layout.leftMargin: Theme.s1
                    name: "chevron-down"; size: 18; color: Theme.textSecondary
                    rotation: root.expanded ? 0 : -90
                    Behavior on rotation { NumberAnimation { duration: Motion.fast } }
                }
                Txt { text: root.heading; kind: "heading"; wrapMode: Text.NoWrap }
                Tag { visible: root.unreleased; text: qsTr("Not released yet") }
                Item { Layout.fillWidth: true }
                Txt {
                    Layout.rightMargin: Theme.s2
                    visible: !!root.version.date
                    text: root.version.date || ""
                    kind: "caption"; numeric: true; wrapMode: Text.NoWrap
                }
            }
            FocusRing {}
        }

        ColumnLayout {   // the notes, indented under the title
            id: body
            Layout.fillWidth: true
            Layout.leftMargin: Theme.s6
            Layout.rightMargin: Theme.s6
            Layout.topMargin: Theme.s1
            Layout.bottomMargin: Theme.s4
            visible: root.expanded
            spacing: Theme.s2

            Repeater {
                model: root.version.intro || []
                Line {
                    required property string modelData
                    html: modelData
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
                        text: root.sectionLabel(sec.modelData.name)
                        kind: "caption"
                        font.weight: Font.DemiBold
                        Accessible.role: Accessible.Heading
                    }
                    Repeater {
                        model: sec.modelData.items
                        Line {
                            required property var modelData
                            html: modelData.html
                            bullet: modelData.bullet
                            level: modelData.level
                        }
                    }
                }
            }
        }
    }
}
