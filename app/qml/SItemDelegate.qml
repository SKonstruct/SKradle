import QtQuick
import com.skradle.app
import QtQuick.Templates as T

T.ItemDelegate {
    id: control
    property bool danger: false
    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset, implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Theme.hMd
    leftPadding: Theme.s3
    rightPadding: Theme.s3
    hoverEnabled: true
    background: Rectangle {
        x: Theme.s1; width: parent.width - 2 * Theme.s1; height: parent.height
        radius: Theme.rXs
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
