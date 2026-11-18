import QtQuick
import com.skradle.app
import QtQuick.Templates as T

// Stepper: [-] value [+]
T.SpinBox {
    id: control
    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset, contentItem.implicitWidth + 2 * 40, 150)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset, 38)
    editable: true
    leftPadding: 40
    rightPadding: 40
    hoverEnabled: true
    focusPolicy: Qt.StrongFocus
    Accessible.name: "Value"
    validator: IntValidator { locale: control.locale.name; bottom: Math.min(control.from, control.to); top: Math.max(control.from, control.to) }

    contentItem: TextInput {
        z: 2
        clip: true
        selectByMouse: true
        text: control.textFromValue(control.value, control.locale)
        font.pixelSize: Theme.fsBody
        font.features: { "tnum": 1 }
        color: control.enabled ? Theme.text : Theme.textTertiary
        selectionColor: Theme.accent
        selectedTextColor: Theme.accentText
        horizontalAlignment: Qt.AlignHCenter
        verticalAlignment: Qt.AlignVCenter
        readOnly: !control.editable
        validator: control.validator
        inputMethodHints: Qt.ImhFormattedNumbersOnly
    }
    up.indicator: Rectangle {
        x: control.width - width
        height: control.height; implicitWidth: 40; implicitHeight: 38
        radius: 10
        color: control.up.pressed ? Theme.pressTint : control.up.hovered ? Theme.hoverTint : "transparent"
        Icon { anchors.centerIn: parent; name: "plus"; size: 16; color: control.value >= control.to || !control.enabled ? Theme.textTertiary : Theme.text }
    }
    down.indicator: Rectangle {
        height: control.height; implicitWidth: 40; implicitHeight: 38
        radius: 10
        color: control.down.pressed ? Theme.pressTint : control.down.hovered ? Theme.hoverTint : "transparent"
        Icon { anchors.centerIn: parent; name: "minus"; size: 16; color: control.value <= control.from || !control.enabled ? Theme.textTertiary : Theme.text }
    }
    background: Rectangle {
        implicitWidth: 140
        radius: 10
        color: Theme.field
        border.width: control.activeFocus ? 2 : 1
        border.color: control.activeFocus ? Theme.accentFg : (control.hovered ? Theme.borderStrong : Theme.fieldBorder)
        Behavior on border.color { ColorAnimation { duration: Motion.fast } }
    }
}
