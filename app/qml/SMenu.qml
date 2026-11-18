import QtQuick
import com.skradle.app
import QtQuick.Templates as T

// Context menu drawn by us (Popup.Item): same on every OS, positioned by Qt at the cursor / anchor and kept inside the window.
T.Menu {
    id: control
    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset, contentItem.implicitWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset, contentItem.implicitHeight + topPadding + bottomPadding)
    margins: 8
    padding: 4
    overlap: 0
    popupType: T.Popup.Item
    delegate: SMenuItem {}
    enter: Transition {
        ParallelAnimation {
            NumberAnimation { property: "opacity"; from: 0; to: 1; duration: Motion.fast; easing.type: Easing.OutCubic }
            NumberAnimation { property: "scale"; from: 0.96; to: 1; duration: Motion.base; easing.type: Easing.BezierSpline; easing.bezierCurve: Motion.decel }
        }
    }
    exit: Transition { NumberAnimation { property: "opacity"; to: 0; duration: Motion.instant } }
    contentItem: ListView {
        implicitHeight: contentHeight
        implicitWidth: 200
        model: control.contentModel
        interactive: Window.window ? contentHeight + control.topPadding + control.bottomPadding > Window.window.height : false
        clip: true
        currentIndex: control.currentIndex
        boundsBehavior: Flickable.StopAtBounds
    }
    background: Card { radius: 12; fill: Theme.card; implicitWidth: 200 }
}
