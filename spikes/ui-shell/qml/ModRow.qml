import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Control {
    id: root
    required property int index
    required property int mid
    required property string name
    required property string tag
    required property string desc
    required property string ver
    required property bool enabledState
    signal toggled(bool on)

    hoverEnabled: true
    activeFocusOnTab: false
    padding: Theme.s4
    leftPadding: Theme.s4
    property int radius: Theme.rLg
    readonly property color tileColor: [Theme.accent, "#43CDCA", "#F2BF3A", "#7C9CF2", "#E58A5C"][mid % 5]
    Accessible.role: Accessible.ListItem
    Accessible.name: name + ", " + tag + ", " + (enabledState ? "enabled" : "disabled")

    background: Card { raised: root.hovered; fill: root.hovered ? Theme.cardHover : Theme.card; radius: root.radius }

    contentItem: RowLayout {
        spacing: Theme.s4
        // everything except the switch dims when the mod is off
        RowLayout {
            Layout.fillWidth: true
            spacing: Theme.s4
            opacity: root.enabledState ? 1 : 0.55
            Behavior on opacity { NumberAnimation { duration: Motion.base } }

            Rectangle {   // tile
                Layout.alignment: Qt.AlignVCenter
                Layout.preferredWidth: 44; Layout.preferredHeight: 44
                radius: 11
                color: Qt.rgba(root.tileColor.r, root.tileColor.g, root.tileColor.b, Theme.dark ? 0.20 : 0.18)
                border.width: 1
                border.color: Qt.rgba(root.tileColor.r, root.tileColor.g, root.tileColor.b, 0.45)
                Txt {
                    anchors.centerIn: parent
                    text: root.name.charAt(0)
                    kind: "heading"
                    wrapMode: Text.NoWrap
                    color: Theme.dark ? root.tileColor : Qt.darker(root.tileColor, 1.6)
                }
            }
            ColumnLayout {
                Layout.fillWidth: true
                Layout.alignment: Qt.AlignVCenter
                spacing: 3
                RowLayout {
                    spacing: 8
                    Txt { text: root.name; kind: "heading"; font.pixelSize: Theme.fsBody + 2; wrapMode: Text.NoWrap; elide: Text.ElideRight; Layout.maximumWidth: implicitWidth }
                    Rectangle {
                        implicitWidth: tagTxt.implicitWidth + 14; implicitHeight: 20; radius: 10
                        color: Theme.hoverTint
                        Txt { id: tagTxt; anchors.centerIn: parent; text: L.t(root.tag); kind: "micro"; wrapMode: Text.NoWrap }
                    }
                    Txt { text: "v" + root.ver; kind: "micro"; color: Theme.textTertiary; wrapMode: Text.NoWrap }
                    Item { Layout.fillWidth: true }
                }
                Txt { Layout.fillWidth: true; text: L.t(root.desc); kind: "caption" }
            }
        }
        ColumnLayout {
            Layout.alignment: Qt.AlignVCenter
            Layout.preferredWidth: 84
            spacing: 2
            Switch {
                id: sw
                Layout.alignment: Qt.AlignHCenter
                checked: root.enabledState
                onToggled: root.toggled(checked)
                Accessible.name: L.t("Enable %1").arg(root.name)
            }
            Item {
                Layout.fillWidth: true
                implicitHeight: stTxt.implicitHeight
                Txt {
                    id: stTxt
                    width: parent.width
                    horizontalAlignment: Text.AlignHCenter
                    text: root.enabledState ? L.t("Enabled") : L.t("Disabled")
                    kind: "micro"
                    wrapMode: Text.NoWrap
                    elide: Text.ElideRight
                    color: root.enabledState ? Theme.ok : Theme.textTertiary
                    Behavior on color { ColorAnimation { duration: Motion.base } }
                }
            }
        }
    }
}
