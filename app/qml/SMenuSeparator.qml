import QtQuick
import com.skradle.app
import QtQuick.Templates as T

T.MenuSeparator {
    implicitWidth: 200
    implicitHeight: 9
    contentItem: Item {
        Rectangle {
            x: Theme.s3; width: parent.width - 2 * Theme.s3; height: 1
            anchors.verticalCenter: parent.verticalCenter
            color: Theme.border
        }
    }
}
