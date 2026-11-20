import QtQuick
import com.skradle.app
import QtQuick.Controls
import QtQuick.Layouts

// One mod, 64 px: picture, then two lines (title with version and author; one line of description, or the problem
// when there is one), the type tags and the switch on the right. Clicking selects (the page decides what that means
// with the modifier keys). Selected = a 3 px accent bar on the left edge plus the raised fill; the switch stays the
// only thing that is pink for "on".
Control {
    id: root
    required property var item
    property bool selected: false
    property bool showFocus: false
    property bool canToggle: true
    property int tagsWidth: 132
    signal clicked(int modifiers)
    signal toggled(bool on)

    readonly property bool broken: item.error !== null
    readonly property bool warned: item.warning
    readonly property int shownTags: Math.min(item.tags.length, 2)
    readonly property string firstWarning: item.warnings.length > 0 ? item.warnings[0] : ""
    readonly property string fileInfo: (item.file_note.length > 0 ? item.file_note + " · " : "") + ModsText.sizeText(item.size)
                                       + (item.container === "modpack" ? " · " + (item.parts === 1 ? qsTr("1 mod") : qsTr("%1 mods").arg(item.parts)) : "")
    readonly property string problem: broken ? item.error
                                      : item.incompatible ? qsTr("Built for another game version, so it is skipped at launch")
                                      : firstWarning + (item.warnings.length > 1 ? " " + qsTr("(+%1 more)").arg(item.warnings.length - 1) : "")
    hoverEnabled: true
    activeFocusOnTab: false
    implicitHeight: Theme.hRow + Theme.s2
    leftPadding: Theme.s4
    rightPadding: Theme.s4
    topPadding: Theme.s3
    bottomPadding: Theme.s3
    Accessible.role: Accessible.ListItem
    Accessible.selected: selected
    Accessible.focusable: true
    Accessible.name: item.name + ", " + item.tags.map(t => ModsText.tagLabel(t)).join(", ") + ", "
                     + (item.enabled ? qsTr("enabled") : qsTr("disabled")) + (problem.length > 0 ? ", " + problem : "")
    Accessible.description: fileInfo

    background: Card {
        elevation: 0
        fill: root.selected ? Theme.raised : (root.hovered ? Theme.cardHover : Theme.card)
        MouseArea {
            anchors.fill: parent
            acceptedButtons: Qt.LeftButton
            onClicked: (mouse) => root.clicked(mouse.modifiers)
        }
        Rectangle {   // the selection cue
            visible: root.selected
            x: 0; y: Theme.s2
            width: 3; height: parent.height - 2 * Theme.s2
            radius: Theme.rXs
            color: Theme.accent
        }
    }

    contentItem: RowLayout {
        spacing: Theme.s3
        RowLayout {   // picture and text; dims when the mod is off
            Layout.fillWidth: true
            Layout.alignment: Qt.AlignVCenter
            spacing: Theme.s3
            opacity: root.item.enabled ? 1 : 0.65
            Behavior on opacity { NumberAnimation { duration: Motion.base } }

            ModImage {
                Layout.alignment: Qt.AlignVCenter
                source: root.item.image
                warned: root.warned
            }
            ColumnLayout {
                Layout.fillWidth: true
                Layout.alignment: Qt.AlignVCenter
                spacing: 2
                RowLayout {   // title, then version and author on the same line
                    Layout.fillWidth: true
                    spacing: Theme.s2
                    TextMetrics { id: nameMetrics; text: root.item.name; font: nameTxt.font }
                    Txt {
                        id: nameTxt
                        Layout.preferredWidth: Math.ceil(nameMetrics.advanceWidth) + 1
                        Layout.maximumWidth: Math.ceil(nameMetrics.advanceWidth) + 1
                        text: root.item.name
                        font.weight: Font.Medium
                        wrapMode: Text.NoWrap
                        elide: Text.ElideRight
                    }
                    Txt {
                        id: metaTxt
                        Layout.fillWidth: true
                        Layout.preferredWidth: 0
                        visible: text.length > 0
                        readonly property var bits: [root.item.version.length > 0 ? "v" + root.item.version : "",
                                                     root.item.author.length > 0 ? qsTr("by %1").arg(root.item.author) : ""]
                        text: bits.filter(b => b.length > 0).join(" · ")
                        kind: "caption"
                        wrapMode: Text.NoWrap
                        elide: Text.ElideRight
                    }
                }
                RowLayout {   // a problem (with its icon), else the description, else the file facts
                    Layout.fillWidth: true
                    spacing: Theme.s1
                    Icon {
                        visible: root.problem.length > 0
                        name: root.broken ? "error" : "warning"
                        size: 14
                        color: root.broken ? Theme.danger : Theme.amberText
                    }
                    Txt {
                        Layout.fillWidth: true
                        text: root.problem.length > 0 ? root.problem
                              : root.item.description.length > 0 ? root.item.description.replace(/\s+/g, " ")
                              : root.fileInfo
                        kind: "caption"
                        color: root.broken ? Theme.danger : root.problem.length > 0 ? Theme.amberText : Theme.textSecondary
                        wrapMode: Text.NoWrap
                        elide: Text.ElideRight
                    }
                }
            }
        }
        Row {   // type tags; at most two, the rest as a count
            Layout.preferredWidth: root.tagsWidth
            Layout.alignment: Qt.AlignVCenter
            layoutDirection: Qt.RightToLeft
            spacing: Theme.s1
            opacity: root.item.enabled ? 1 : 0.65
            visible: root.item.tags.length > 0
            Tag { visible: root.item.tags.length > 2; text: "+" + (root.item.tags.length - 2); Accessible.ignored: true }
            Repeater {
                model: root.item.tags.slice(0, 2).reverse()
                delegate: Tag { required property string modelData; text: ModsText.tagLabel(modelData); Accessible.ignored: true }
            }
        }
        SSwitch {
            Layout.alignment: Qt.AlignVCenter
            enabled: root.canToggle
            focusPolicy: Qt.NoFocus   // the list is one tab stop; Space toggles the focused or selected mods
            checked: root.item.enabled
            onToggled: root.toggled(checked)
            Accessible.name: qsTr("Enable %1").arg(root.item.name)
        }
    }
    FocusRing { active: root.showFocus; grow: 3; radius: Theme.rMd + 3 }
    Tip {
        text: root.broken ? root.item.error_detail : root.fileInfo
        wanted: root.hovered
        placement: "bottom"
    }
}
