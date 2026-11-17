import QtQuick
import QtQuick.Templates as T

T.MenuItem {
    id: control
    property bool danger: false
    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset, implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: 36
    leftPadding: 14
    rightPadding: 14
    hoverEnabled: true
    background: Rectangle {
        x: 4; width: parent.width - 8; height: parent.height
        radius: 7
        color: control.down ? Theme.pressTint : control.highlighted ? (control.danger ? Qt.rgba(Theme.danger.r, Theme.danger.g, Theme.danger.b, 0.14) : Theme.accentTint) : "transparent"
    }
    contentItem: Text {
        text: control.text
        font.pixelSize: Theme.fsBody
        color: !control.enabled ? Theme.textTertiary : control.danger ? Theme.danger : Theme.text
        verticalAlignment: Text.AlignVCenter
        elide: Text.ElideRight
    }
}
