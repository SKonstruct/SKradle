import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// "Disabled group" pattern: the group underneath is disabled and dimmed by its owner; this overlay explains why
// (visible chip) and shows a tooltip on hover or keyboard focus.
Control {
    id: root
    property string reason
    property int radius: Theme.rLg
    property string icon: "lock"
    hoverEnabled: true
    activeFocusOnTab: visible
    focusPolicy: Qt.StrongFocus
    Accessible.role: Accessible.StaticText
    Accessible.name: reason
    Accessible.description: reason

    background: Rectangle {
        radius: root.radius
        color: Qt.rgba(Theme.card.r, Theme.card.g, Theme.card.b, Theme.dark ? 0.55 : 0.6)
    }
    contentItem: Item {
        Rectangle {
            id: chip
            anchors.centerIn: parent
            width: Math.min(parent.width - 2 * Theme.s5, chipRow.implicitWidth + 28)
            height: chipRow.implicitHeight + 18
            radius: Math.min(height / 2, 20)
            color: Theme.raised
            border.width: 1
            border.color: Theme.borderStrong
            RowLayout {
                id: chipRow
                x: 14; y: 9
                width: chip.width - 28
                spacing: 8
                Icon { name: root.icon; size: 16; color: Theme.textSecondary; Layout.alignment: Qt.AlignVCenter }
                Txt {
                    Layout.fillWidth: true
                    text: root.reason
                    kind: "caption"
                    color: Theme.text
                    font.weight: Font.Medium
                }
            }
        }
    }
    FocusRing { grow: -2 }
    Tip { text: root.reason; wanted: root.hovered || root.visualFocus; delay: 350 }
}
