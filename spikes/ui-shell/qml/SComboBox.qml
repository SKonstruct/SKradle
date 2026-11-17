import QtQuick
import QtQuick.Templates as T

// Field + our own popup list (no OS popup): chevron, pink-tint highlight, check on the current row.
T.ComboBox {
    id: control
    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset, implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset, implicitContentHeight + topPadding + bottomPadding)
    leftPadding: 12
    rightPadding: 36
    hoverEnabled: true
    focusPolicy: Qt.StrongFocus
    Accessible.name: displayText

    delegate: SItemDelegate {
        required property var model
        required property int index
        width: ListView.view ? ListView.view.width : control.width
        text: model[control.textRole] !== undefined ? model[control.textRole] : modelData
        highlighted: control.highlightedIndex === index
        font.weight: control.currentIndex === index ? Font.Medium : Font.Normal
        Icon {
            anchors.right: parent.right; anchors.rightMargin: 12; anchors.verticalCenter: parent.verticalCenter
            name: "check"; size: 16; color: Theme.accentFg
            visible: control.currentIndex === index
        }
    }
    indicator: Icon {
        x: control.width - width - 12
        y: (control.height - height) / 2
        name: "chevron-down"; size: 16
        color: control.enabled ? Theme.textSecondary : Theme.textTertiary
        rotation: control.popup.visible ? 180 : 0
        Behavior on rotation { NumberAnimation { duration: Motion.base; easing.type: Easing.BezierSpline; easing.bezierCurve: Motion.standard } }
    }
    contentItem: Text {
        text: control.displayText
        font.pixelSize: Theme.fsBody
        color: control.enabled ? Theme.text : Theme.textTertiary
        verticalAlignment: Text.AlignVCenter
        elide: Text.ElideRight
    }
    background: Rectangle {
        implicitWidth: 160
        implicitHeight: 38
        radius: 10
        color: control.down ? Theme.pressTint : Theme.field
        border.width: control.visualFocus || control.popup.visible ? 2 : 1
        border.color: control.visualFocus || control.popup.visible ? Theme.accentFg : (control.hovered ? Theme.borderStrong : Theme.fieldBorder)
        Behavior on border.color { ColorAnimation { duration: Motion.fast } }
    }
    popup: T.Popup {
        y: control.height + 6
        width: control.width
        implicitHeight: Math.min(contentItem.implicitHeight + 8, 320)
        padding: 4
        closePolicy: T.Popup.CloseOnEscape | T.Popup.CloseOnPressOutsideParent
        enter: Transition {
            ParallelAnimation {
                NumberAnimation { property: "opacity"; from: 0; to: 1; duration: Motion.fast; easing.type: Easing.OutCubic }
                NumberAnimation { property: "y"; from: control.height; to: control.height + 6; duration: Motion.base; easing.type: Easing.BezierSpline; easing.bezierCurve: Motion.decel }
            }
        }
        exit: Transition { NumberAnimation { property: "opacity"; to: 0; duration: Motion.instant } }
        contentItem: ListView {
            clip: true
            implicitHeight: contentHeight
            model: control.delegateModel
            currentIndex: control.highlightedIndex
            boundsBehavior: Flickable.StopAtBounds
        }
        background: Card { radius: 12; fill: Theme.card }
    }
}
