import QtQuick
import com.skradle.app
import QtQuick.Controls
import QtQuick.Layouts

// One mod. Columns: info (fills), type tags (fixed), switch (fixed, on the right edge). The page gives the two fixed
// widths so every row lines up. Clicking selects (the page decides what that means with the modifier keys).
Control {
    id: root
    required property var item
    property bool selected: false
    property bool showFocus: false
    property bool canToggle: true
    property int tagsWidth: 140
    property int switchWidth: 52
    signal clicked(int modifiers)
    signal toggled(bool on)

    readonly property bool broken: item.error !== null
    readonly property bool warned: item.warning
    hoverEnabled: true
    activeFocusOnTab: false
    padding: Theme.s4
    property int radius: Theme.rLg
    Accessible.role: Accessible.ListItem
    Accessible.selected: selected
    Accessible.focusable: true
    Accessible.name: item.name + ", " + item.tags.map(t => ModsText.tagLabel(t)).join(", ") + ", "
                     + (item.enabled ? qsTr("enabled") : qsTr("disabled")) + (broken ? ", " + item.error : "")
                     + (!broken && warned ? ", " + qsTr("has a warning") : "")

    background: Card {
        raised: false
        fill: root.selected ? Theme.cardHover : (root.hovered ? Theme.cardHover : Theme.card)
        radius: root.radius
        Rectangle {
            anchors.fill: parent; radius: root.radius
            color: root.selected ? Theme.accentTint : "transparent"
            border.width: root.selected ? 2 : 0
            border.color: Theme.accentFg
        }
        MouseArea {
            anchors.fill: parent
            acceptedButtons: Qt.LeftButton
            onClicked: (mouse) => root.clicked(mouse.modifiers)
        }
    }

    contentItem: RowLayout {
        spacing: Theme.s4
        RowLayout {   // info column; dims when the mod is off
            Layout.fillWidth: true
            Layout.alignment: Qt.AlignVCenter
            spacing: Theme.s4
            opacity: root.item.enabled ? 1 : 0.55
            Behavior on opacity { NumberAnimation { duration: Motion.base } }

            ModImage {
                Layout.alignment: Qt.AlignTop
                source: root.item.image
                warned: root.warned
            }
            ColumnLayout {   // each fact once: title, version and author, description, file, then problems
                Layout.fillWidth: true
                spacing: 3
                Txt {   // wraps to a second line before it ever elides
                    Layout.fillWidth: true
                    text: root.item.name
                    kind: "heading"
                    font.pixelSize: Theme.fsBody + 2
                    wrapMode: Text.WrapAtWordBoundaryOrAnywhere
                    maximumLineCount: 2
                    elide: Text.ElideRight
                }
                Txt {
                    Layout.fillWidth: true
                    visible: text.length > 0
                    readonly property var bits: [root.item.version.length > 0 ? "v" + root.item.version : "",
                                                 root.item.author.length > 0 ? qsTr("by %1").arg(root.item.author) : ""]
                    text: bits.filter(b => b.length > 0).join(" · ")
                    kind: "caption"
                    color: Theme.textSecondary
                    wrapMode: Text.NoWrap
                    elide: Text.ElideRight
                }
                Txt {
                    Layout.fillWidth: true
                    visible: text.length > 0
                    text: root.item.description.replace(/\s+/g, " ")
                    kind: "caption"
                    wrapMode: Text.NoWrap
                    elide: Text.ElideRight
                }
                Txt {   // the file name only when it says something the title does not
                    Layout.fillWidth: true
                    text: (root.item.file_note.length > 0 ? root.item.file_note + " · " : "") + ModsText.sizeText(root.item.size)
                          + (root.item.container === "modpack" ? " · " + (root.item.parts === 1 ? qsTr("1 mod") : qsTr("%1 mods").arg(root.item.parts)) : "")
                    kind: "micro"
                    color: Theme.textTertiary
                    wrapMode: Text.NoWrap
                    elide: Text.ElideMiddle
                }
                Txt {
                    Layout.fillWidth: true
                    visible: root.broken
                    text: qsTr("Cannot be used: %1").arg(root.item.error || "")
                    kind: "caption"
                    color: Theme.danger
                }
                Txt {
                    Layout.fillWidth: true
                    visible: !root.broken && root.item.incompatible
                    text: qsTr("Built for another game version. It is skipped when the game starts.")
                    kind: "caption"
                    color: Theme.amberText
                }
                Repeater {   // what the scan noted: every warning, none repeated
                    model: root.broken ? [] : root.item.warnings
                    delegate: Txt {
                        required property string modelData
                        Layout.fillWidth: true
                        text: modelData
                        kind: "caption"
                        color: Theme.amberText
                        maximumLineCount: 2
                        elide: Text.ElideRight
                    }
                }
            }
        }
        Flow {   // type tags column
            Layout.preferredWidth: root.tagsWidth
            Layout.maximumWidth: root.tagsWidth
            Layout.alignment: Qt.AlignVCenter
            spacing: 6
            opacity: root.item.enabled ? 1 : 0.55
            Repeater {
                model: root.item.tags
                delegate: Rectangle {
                    required property string modelData
                    implicitWidth: tagTxt.implicitWidth + 16; implicitHeight: 22; radius: 11
                    color: modelData === "broken" ? Theme.amberTint : Theme.hoverTint
                    Txt { id: tagTxt; anchors.centerIn: parent; text: ModsText.tagLabel(parent.modelData); kind: "micro"; wrapMode: Text.NoWrap
                          color: parent.modelData === "broken" ? Theme.amberText : Theme.textSecondary }
                }
            }
        }
        Item {   // switch column, flush with the right edge
            Layout.preferredWidth: root.switchWidth
            Layout.alignment: Qt.AlignVCenter
            implicitHeight: sw.implicitHeight
            SSwitch {
                id: sw
                anchors.right: parent.right
                enabled: root.canToggle
                focusPolicy: Qt.NoFocus   // the list is one tab stop; Space toggles the focused or selected mods
                checked: root.item.enabled
                onToggled: root.toggled(checked)
                Accessible.name: qsTr("Enable %1").arg(root.item.name)
            }
        }
    }
    FocusRing { active: root.showFocus; grow: 3 }
}
