import QtQuick
import com.skradle.app
import QtQuick.Templates as T

// Our button. variant: primary (brand pink, ONE per region) | secondary (neutral, bordered) | ghost (text only)
//   | danger (ghost, red text) | danger-primary (filled red, for irreversible confirmations)
//   | coffee (Buy Me a Coffee yellow, dark text; the donate button only).
// size: sm (28) | md (36, default) | lg (44). busy: a spinner replaces the glyph, the label stays, clicks are ignored.
// `highlighted: true` is a synonym for variant primary.
T.Button {
    id: control
    property string variant: highlighted ? "primary" : "secondary"
    property string size: "md"
    property string glyph
    property url iconSource          // a picture instead of the glyph (a tool's own app icon)
    property bool busy: false
    property int radius: Theme.rSm

    readonly property int boxHeight: size === "sm" ? Theme.hSm : size === "lg" ? Theme.hLg : Theme.hMd
    readonly property int iconPx: size === "sm" ? 16 : 18
    readonly property bool hasPicture: iconSource.toString().length > 0 && !busy

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset, implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: boxHeight
    leftPadding: size === "sm" ? Theme.s3 : size === "lg" ? Theme.s5 : Theme.s4
    rightPadding: leftPadding
    topPadding: 0
    bottomPadding: 0
    spacing: Theme.gapInline
    hoverEnabled: true
    focusPolicy: busy ? Qt.NoFocus : Qt.StrongFocus
    font.pixelSize: Theme.fsBody
    font.weight: Font.Medium
    Accessible.name: text
    Accessible.description: busy ? qsTr("Working") : ""

    readonly property bool primary: variant === "primary"
    readonly property bool filledDanger: variant === "danger-primary"
    readonly property bool solid: primary || filledDanger
    readonly property color fg: !enabled ? Theme.textTertiary
        : solid ? Theme.accentText
        : variant === "danger" ? Theme.danger
        : variant === "coffee" ? Theme.ink
        : Theme.text
    readonly property bool live: hovered && enabled && !busy

    scale: down && enabled && !busy ? 0.97 : 1
    Behavior on scale { NumberAnimation { duration: Motion.instant; easing.type: Easing.OutQuad } }

    background: Rectangle {
        implicitHeight: control.boxHeight
        // Text-only buttons are as wide as their label; a minimum width would leave invisible padding.
        implicitWidth: control.variant === "ghost" || control.variant === "danger" ? 0 : 72
        radius: control.radius
        color: !control.enabled ? (control.solid ? Theme.track : "transparent")
            : control.primary ? (control.down ? Theme.accentPressed : control.live ? Theme.accentHover : Theme.accent)
            : control.variant === "coffee" ? (control.down ? Theme.coffeePressed : control.live ? Theme.coffeeHover : Theme.coffee)
            : control.filledDanger ? (control.down ? Theme.dangerFillPressed : control.live ? Theme.dangerFillHover : Theme.dangerFill)
            : control.variant === "secondary" ? (control.down ? Theme.statePress : control.live ? Theme.stateHover : Theme.raised)
            : (control.down ? Theme.statePress : control.live ? Theme.stateHover : "transparent")
        border.width: control.variant === "secondary" ? 1 : 0
        border.color: control.live ? Theme.borderStrong : Theme.border
        Behavior on color { ColorAnimation { duration: Motion.fast } }
    }
    contentItem: Row {
        height: control.availableHeight
        spacing: control.spacing
        layoutDirection: Qt.LeftToRight
        Icon {
            id: spin
            visible: control.busy || (control.glyph.length > 0 && !control.hasPicture)
            name: control.busy ? "refresh" : control.glyph
            size: control.iconPx
            color: control.fg
            anchors.verticalCenter: parent.verticalCenter
            // essential progress feedback: still turns under reduced motion
            RotationAnimator on rotation { from: 0; to: 360; duration: Motion.spin; loops: Animation.Infinite; running: control.busy && control.visible }
            onVisibleChanged: if (!control.busy) rotation = 0
        }
        Image {
            visible: control.hasPicture
            source: control.iconSource
            width: control.iconPx + 4
            height: width
            sourceSize: Qt.size(width * 2, height * 2)
            fillMode: Image.PreserveAspectFit
            smooth: true
            mipmap: true
            opacity: control.enabled ? 1 : 0.4
            anchors.verticalCenter: parent.verticalCenter
        }
        Text {
            text: control.text
            font: control.font
            color: control.fg
            elide: Text.ElideRight
            height: parent.height
            verticalAlignment: Text.AlignVCenter
        }
    }
    // swallows clicks while busy
    MouseArea { anchors.fill: parent; visible: control.busy; hoverEnabled: true; cursorShape: Qt.BusyCursor }
    FocusRing { grow: 3 }
}
