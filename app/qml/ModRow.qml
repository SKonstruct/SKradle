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
    readonly property bool warned: item.incompatible || item.warnings.length > 0
    hoverEnabled: true
    activeFocusOnTab: false
    padding: Theme.s4
    property int radius: Theme.rLg
    Accessible.role: Accessible.ListItem
    Accessible.selected: selected
    Accessible.focusable: true
    Accessible.name: item.name + ", " + item.tags.map(t => ModsText.tagLabel(t)).join(", ") + ", "
                     + (item.enabled ? qsTr("enabled") : qsTr("disabled")) + (broken ? ", " + item.error : "")

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

            Rectangle {
                Layout.alignment: Qt.AlignTop
                Layout.preferredWidth: 44; Layout.preferredHeight: 44
                radius: 11
                color: root.broken || root.warned ? Theme.amberTint : Theme.accentTint
                border.width: 1
                border.color: Theme.border
                Icon { visible: root.broken; anchors.centerIn: parent; name: "warning"; size: 22; color: Theme.amberText }
                Txt {
                    visible: !root.broken
                    anchors.centerIn: parent
                    text: root.item.name.charAt(0).toUpperCase()
                    kind: "heading"
                    wrapMode: Text.NoWrap
                    color: root.warned ? Theme.amberText : Theme.accentFg
                }
            }
            ColumnLayout {
                Layout.fillWidth: true
                spacing: 3
                RowLayout {
                    Layout.fillWidth: true
                    spacing: Theme.s2
                    Txt { text: root.item.name; kind: "heading"; font.pixelSize: Theme.fsBody + 2; wrapMode: Text.NoWrap; elide: Text.ElideRight; Layout.fillWidth: true; Layout.maximumWidth: implicitWidth }
                    Txt { id: verTxt; visible: root.item.version.length > 0; text: "v" + root.item.version; kind: "micro"; color: Theme.textTertiary; wrapMode: Text.NoWrap }
                    Item { Layout.fillWidth: true }
                }
                Txt {
                    Layout.fillWidth: true
                    visible: text.length > 0
                    text: (root.item.author.length > 0 ? qsTr("by %1").arg(root.item.author) + (root.item.description.length > 0 ? " · " : "") : "") + root.item.description
                    kind: "caption"
                    maximumLineCount: 2
                    elide: Text.ElideRight
                }
                Txt {
                    Layout.fillWidth: true
                    text: (root.item.file !== root.item.name ? root.item.file + " · " : "") + ModsText.kindText(root.item)
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
                Txt {
                    Layout.fillWidth: true
                    visible: !root.broken && root.item.warnings.length > 0
                    text: root.item.warnings[0] + (root.item.warnings.length > 1 ? " " + qsTr("(+%n more)", "", root.item.warnings.length - 1) : "")
                    kind: "caption"
                    color: Theme.amberText
                    maximumLineCount: 2
                    elide: Text.ElideRight
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
