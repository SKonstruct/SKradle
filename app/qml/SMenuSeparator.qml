import QtQuick
import com.skradle.app
import QtQuick.Templates as T

T.MenuSeparator {
    implicitWidth: 200
    implicitHeight: 9
    contentItem: Item {
        Rectangle {
            x: 12; width: parent.width - 24; height: 1
            anchors.verticalCenter: parent.verticalCenter
            color: Theme.border
        }
    }
}
