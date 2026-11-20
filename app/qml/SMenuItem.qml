import QtQuick
import com.skradle.app
import QtQuick.Templates as T

T.MenuItem {
    id: control
    property bool danger: false
    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset, implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Theme.hMd
    leftPadding: Theme.s4
    rightPadding: Theme.s4
    hoverEnabled: true
    background: Rectangle {
        x: Theme.s1; width: parent.width - 2 * Theme.s1; height: parent.height
        radius: Theme.rXs
        color: control.down ? Theme.pressTint : control.highlighted ? (control.danger ? Theme.dangerTint : Theme.accentTint) : "transparent"
    }
    contentItem: Text {
        text: control.text
        font.pixelSize: Theme.fsBody
        color: !control.enabled ? Theme.textTertiary : control.danger ? Theme.danger : Theme.text
        verticalAlignment: Text.AlignVCenter
        elide: Text.ElideRight
    }
}
