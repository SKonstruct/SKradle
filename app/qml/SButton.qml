import QtQuick
import com.skradle.app
import QtQuick.Templates as T

// Our button. variant: primary (brand pink) | secondary (neutral, bordered) | ghost (text only) | danger (ghost, red text)
// Same drawing on every OS. `highlighted: true` is a synonym for variant primary.
T.Button {
    id: control
    property string variant: highlighted ? "primary" : "secondary"
    property string glyph
    property int radius: 10

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset, implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset, implicitContentHeight + topPadding + bottomPadding)
    leftPadding: 16
    rightPadding: 16
    topPadding: 8
    bottomPadding: 8
    spacing: 8
    hoverEnabled: true
    focusPolicy: Qt.StrongFocus
    font.pixelSize: Theme.fsBody
    font.weight: Font.Medium
    Accessible.name: text

    readonly property bool primary: variant === "primary"
    readonly property color fg: !enabled ? Theme.textTertiary
        : primary ? Theme.accentText
        : variant === "danger" ? Theme.danger
        : Theme.text

    scale: down && enabled ? 0.97 : 1
    Behavior on scale { NumberAnimation { duration: Motion.instant; easing.type: Easing.OutQuad } }

    background: Rectangle {
        implicitHeight: 36
        implicitWidth: 72
        radius: control.radius
        color: !control.enabled ? (control.primary ? Theme.track : "transparent")
            : control.primary ? (control.down ? Theme.accentPressed : control.hovered ? Theme.accentHover : Theme.accent)
            : control.variant === "secondary" ? (control.down ? Theme.pressTint : control.hovered ? Theme.hoverTint : Theme.raised)
            : (control.down ? Theme.pressTint : control.hovered ? Theme.hoverTint : "transparent")
        border.width: control.variant === "secondary" ? 1 : 0
        border.color: control.hovered ? Theme.borderStrong : Theme.border
        Behavior on color { ColorAnimation { duration: Motion.fast } }
    }
    contentItem: Row {
        spacing: control.spacing
        layoutDirection: Qt.LeftToRight
        Icon {
            visible: control.glyph.length > 0
            name: control.glyph; size: 18; color: control.fg
            anchors.verticalCenter: parent.verticalCenter
        }
        Text {
            text: control.text
            font: control.font
            color: control.fg
            elide: Text.ElideRight
            anchors.verticalCenter: parent.verticalCenter
        }
    }
    FocusRing { grow: 3 }
}
