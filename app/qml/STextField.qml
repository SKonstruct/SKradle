import QtQuick
import com.skradle.app
import QtQuick.Templates as T

T.TextField {
    id: control
    property string glyph
    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset, contentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset, contentHeight + topPadding + bottomPadding)
    leftPadding: 12 + (glyph.length > 0 ? 26 : 0)
    rightPadding: 12
    topPadding: 8
    bottomPadding: 8
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
    Accessible.name: placeholderText

    background: Rectangle {
        implicitWidth: 200
        implicitHeight: 38
        radius: 10
        color: Theme.field
        border.width: control.activeFocus ? 2 : 1
        border.color: control.activeFocus ? Theme.accentFg : (control.hovered ? Theme.borderStrong : Theme.fieldBorder)
        Behavior on border.color { ColorAnimation { duration: Motion.fast } }
        Icon {
            visible: control.glyph.length > 0
            x: 12; anchors.verticalCenter: parent.verticalCenter
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
