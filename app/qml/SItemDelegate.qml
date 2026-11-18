import QtQuick
import com.skradle.app
import QtQuick.Templates as T

T.ItemDelegate {
    id: control
    property bool danger: false
    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset, implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: 36
    leftPadding: 12
    rightPadding: 12
    hoverEnabled: true
    background: Rectangle {
        x: 4; width: parent.width - 8; height: parent.height
        radius: 7
        color: control.down ? Theme.pressTint : control.highlighted ? Theme.accentTint : control.hovered ? Theme.hoverTint : "transparent"
    }
    contentItem: Text {
        text: control.text
        font.pixelSize: Theme.fsBody
        font.weight: control.highlighted ? Font.Medium : Font.Normal
        color: !control.enabled ? Theme.textTertiary : control.danger ? Theme.danger : Theme.text
        elide: Text.ElideRight
        verticalAlignment: Text.AlignVCenter
    }
}
