import QtQuick
import com.skradle.app
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

    background: Card {
        raised: root.hovered
        fill: root.hovered ? Theme.cardHover : Theme.card
        radius: root.radius
    }
    contentItem: ColumnLayout {
        id: col
        spacing: 0
        Rectangle {   // art: brand-colour plate, the shared glyph cropped large, one icon tile. Cropped by the card.
            id: art
            Layout.fillWidth: true
            Layout.preferredHeight: 112
            Layout.margins: 1
            topLeftRadius: root.radius - 1
            topRightRadius: root.radius - 1
            clip: true
            readonly property var tops: Theme.dark ? ["#B2548A", "#2A5C58", "#8A6414"] : ["#B8568E", "#B7D8D4", "#F5D273"]
            readonly property var bots: Theme.dark ? ["#6E2C52", "#17302E", "#4A370C"] : ["#D58AB4", "#8FBDB8", "#EDB63A"]
            gradient: Gradient {
                GradientStop { position: 0; color: art.tops[root.hue % 3] }
                GradientStop { position: 1; color: art.bots[root.hue % 3] }
            }
            Glyph {   // the real mark, large, running off the bottom edge
                x: parent.width * 0.58
                y: 16
                glyphHeight: 160
                opacity: root.hue === 0 ? 1 : (Theme.dark ? 0.16 : 0.30)
            }
            Rectangle {   // icon tile
                x: 18; y: 18; width: 44; height: 44; radius: 12
                color: Qt.rgba(1, 1, 1, Theme.dark ? 0.14 : 0.30)
                border.width: 1
                border.color: Qt.rgba(1, 1, 1, Theme.dark ? 0.26 : 0.55)
                Icon {
                    anchors.centerIn: parent
                    name: ["download", "globe", "bolt"][root.hue % 3]
                    size: 24; stroke: 2
                    color: root.hue === 2 && !Theme.dark ? "#4A370C" : "#FFFFFF"
                }
            }
            Rectangle {   // hairline separating art and text
                anchors.bottom: parent.bottom
                width: parent.width; height: 1
                color: Theme.border
            }
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
