import QtQuick
import com.skradle.app
import QtQuick.Templates as T

T.TextField {
    id: control
    property string glyph
    property string label            // visible label of the field: becomes the accessible name
    property bool invalid: false     // red border; show the sentence in a Txt below the field
    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset, contentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset, contentHeight + topPadding + bottomPadding)
    leftPadding: Theme.s3 + (glyph.length > 0 ? Theme.s6 + Theme.s1 : 0)
    rightPadding: Theme.s3
    topPadding: Theme.s2
    bottomPadding: Theme.s2
    font.pixelSize: Theme.fsBody
    color: enabled ? Theme.text : Theme.textTertiary
    placeholderTextColor: Theme.textTertiary
    selectionColor: Theme.accent
    selectedTextColor: Theme.accentText
    verticalAlignment: TextInput.AlignVCenter
    hoverEnabled: true
    selectByMouse: true
    clip: true
    focusPolicy: Qt.StrongFocus
    Accessible.name: label.length > 0 ? label : placeholderText

    background: Rectangle {
        implicitWidth: 200
        implicitHeight: Theme.hMd
        radius: Theme.rSm
        color: Theme.field
        border.width: control.activeFocus || control.invalid ? 2 : 1
        border.color: control.invalid ? Theme.danger : control.activeFocus ? Theme.accentFg : (control.hovered ? Theme.textSecondary : Theme.fieldBorder)
        Behavior on border.color { ColorAnimation { duration: Motion.fast } }
        Icon {
            visible: control.glyph.length > 0
            x: Theme.s3; anchors.verticalCenter: parent.verticalCenter
            name: control.glyph; size: 16
            color: control.activeFocus ? Theme.accentFg : Theme.textTertiary
        }
    }
    // placeholder
    Text {
        x: control.leftPadding; y: control.topPadding
        width: control.width - control.leftPadding - control.rightPadding
        height: control.height - control.topPadding - control.bottomPadding
        text: control.placeholderText
        font: control.font
        color: control.placeholderTextColor
        verticalAlignment: control.verticalAlignment
        visible: !control.length && !control.preeditText
        elide: Text.ElideRight
    }
}
