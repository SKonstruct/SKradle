import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

AbstractButton {
    id: root
    property string tag
    property string title
    property string body
    property string date
    property int hue: 0
    hoverEnabled: true
    activeFocusOnTab: true
    focusPolicy: Qt.StrongFocus
    property int radius: Theme.rLg
    implicitHeight: col.implicitHeight
    Accessible.role: Accessible.Button
    Accessible.name: title + ". " + body
    text: title

    readonly property var tints: [Theme.accent, "#43CDCA", "#F2BF3A"]
    readonly property color tint: tints[hue % 3]

    background: Card {
        raised: root.hovered
        fill: root.hovered ? Theme.cardHover : Theme.card
        radius: root.radius
    }
    contentItem: ColumnLayout {
        id: col
        spacing: 0
        Rectangle {   // art: gradient plate with drawn shapes, kept inside the rounded top corners
            Layout.fillWidth: true
            Layout.preferredHeight: 104
            Layout.margins: 1
            topLeftRadius: root.radius - 1
            topRightRadius: root.radius - 1
            gradient: Gradient {
                orientation: Gradient.Horizontal
                GradientStop { position: 0; color: Qt.rgba(root.tint.r, root.tint.g, root.tint.b, Theme.dark ? 0.30 : 0.38) }
                GradientStop { position: 1; color: Qt.rgba(root.tint.r, root.tint.g, root.tint.b, Theme.dark ? 0.08 : 0.12) }
            }
            Rectangle { x: parent.width * 0.62; y: 22; width: 64; height: 64; radius: 32; color: "transparent"; border.width: 2; border.color: Qt.rgba(root.tint.r, root.tint.g, root.tint.b, 0.55) }
            Rectangle { x: parent.width * 0.62 + 26; y: 48; width: 38; height: 38; radius: 19; color: Qt.rgba(root.tint.r, root.tint.g, root.tint.b, 0.35) }
            Rectangle { x: 20; y: 24; width: 34; height: 34; radius: 8; rotation: 45; color: "transparent"; border.width: 2; border.color: Qt.rgba(root.tint.r, root.tint.g, root.tint.b, 0.5) }
            Rectangle { x: 20; y: 72; width: 60; height: 6; radius: 3; color: Qt.rgba(root.tint.r, root.tint.g, root.tint.b, 0.35) }
        }
        ColumnLayout {
            Layout.fillWidth: true
            Layout.margins: Theme.s4
            Layout.topMargin: Theme.s3
            spacing: 6
            RowLayout {
                spacing: 8
                Txt { text: root.tag; kind: "micro"; color: Theme.textSecondary; wrapMode: Text.NoWrap }
                Txt { text: "·"; kind: "micro"; color: Theme.textTertiary; wrapMode: Text.NoWrap }
                Txt { text: root.date; kind: "micro"; color: Theme.textTertiary; wrapMode: Text.NoWrap; Layout.fillWidth: true; elide: Text.ElideRight }
            }
            Txt { Layout.fillWidth: true; text: root.title; kind: "heading" }
            Txt { Layout.fillWidth: true; text: root.body; kind: "caption"; Layout.bottomMargin: 2 }
        }
    }
    FocusRing {}
}
