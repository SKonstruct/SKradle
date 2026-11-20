import QtQuick
import com.skradle.app
import QtQuick.Templates as T

// Field + our own popup list (no OS popup): chevron, pink-tint highlight, check on the current row.
T.ComboBox {
    id: control
    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset, implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset, implicitContentHeight + topPadding + bottomPadding)
    leftPadding: Theme.s3
    rightPadding: Theme.s8 + Theme.s2
    hoverEnabled: true
    focusPolicy: Qt.StrongFocus
    Accessible.name: displayText

    delegate: SItemDelegate {
        required property var model
        required property int index
        width: ListView.view ? ListView.view.width : control.width
        // Plain arrays have no role names: `model.modelData` is the entry (the implicit `modelData` is not injected
        // into a delegate with required properties).
        text: control.textRole ? model[control.textRole] : model.modelData
        highlighted: control.highlightedIndex === index
        font.weight: control.currentIndex === index ? Font.Medium : Font.Normal
        Icon {
            anchors.right: parent.right; anchors.rightMargin: Theme.s3; anchors.verticalCenter: parent.verticalCenter
            name: "check"; size: 16; color: Theme.accentFg
            visible: control.currentIndex === index
        }
    }
    indicator: Icon {
        x: control.width - width - Theme.s3
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
        implicitHeight: Theme.hMd
        radius: Theme.rSm
        color: control.down ? Theme.pressTint : Theme.field
        border.width: control.visualFocus || control.popup.visible ? 2 : 1
        border.color: control.visualFocus || control.popup.visible ? Theme.accentFg : (control.hovered ? Theme.textSecondary : Theme.fieldBorder)
        Behavior on border.color { ColorAnimation { duration: Motion.fast } }
    }
    popup: T.Popup {
        id: dropdown
        // The slide-in is a transform, so `y` keeps its binding.
        property real slide: 0
        y: control.height + Theme.s1
        width: control.width
        implicitHeight: Math.min(contentItem.implicitHeight + Theme.s2, 320)
        padding: Theme.s1
        closePolicy: T.Popup.CloseOnEscape | T.Popup.CloseOnPressOutsideParent
        enter: Transition {
            ParallelAnimation {
                NumberAnimation { property: "opacity"; from: 0; to: 1; duration: Motion.fast; easing.type: Easing.OutCubic }
                NumberAnimation { target: dropdown; property: "slide"; from: -6; to: 0; duration: Motion.base; easing.type: Easing.BezierSpline; easing.bezierCurve: Motion.decel }
            }
        }
        exit: Transition { NumberAnimation { property: "opacity"; to: 0; duration: Motion.instant } }
        contentItem: ListView {
            transform: Translate { y: dropdown.slide }
            clip: true
            implicitHeight: contentHeight
            model: control.delegateModel
            currentIndex: control.highlightedIndex
            boundsBehavior: Flickable.StopAtBounds
        }
        background: Card {
            radius: Theme.rMd; fill: Theme.overlay; elevation: 2
            transform: Translate { y: dropdown.slide }
        }
    }
}
