import QtQuick
import com.skradle.app
import QtQuick.Controls
import QtQuick.Layouts

// The frame of every dialog: title, optional explanation, a scrolling body and a right-aligned row of buttons.
// size: "form" (480 px, default) or "reading" (560 px, for text to read). An explicit preferredWidth still wins.
// Modal; Tab stays inside it; Escape closes it; when it closes the focus goes back to the control that opened it.
Popup {
    id: dlg
    property string title
    property string subtitle
    property string size: "form"
    property int preferredWidth: size === "reading" ? Theme.readMeasure : 480
    // The control that had the focus when the dialog opened (it gets it back).
    property Item opener: null
    // Where the focus goes when that control is gone (for example the row that was just deleted).
    property Item fallback: null
    default property alias body: bodyCol.data
    property alias buttons: buttonRow.data
    // Called when the dialog opens: put the focus on the first field or the safest button.
    signal focusFirst()

    parent: Overlay.overlay
    anchors.centerIn: parent
    modal: true
    focus: true
    padding: 0
    width: Math.min(preferredWidth, (parent ? parent.width : preferredWidth) - 48)
    height: Math.min(implicitHeight, (parent ? parent.height : 640) - 48)
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside

    function openFrom(item) {
        opener = item || (parent && parent.Window.window ? parent.Window.activeFocusItem : null)
        open()
    }
    // Scrolls a focused field into view.
    function reveal(item) {
        const top = item.mapToItem(bodyCol, 0, 0).y
        const bottom = top + item.height + Theme.s2
        if (top < flick.contentY) flick.contentY = Math.max(0, top - Theme.s2)
        else if (bottom > flick.contentY + flick.height) flick.contentY = bottom - flick.height
    }

    onOpened: dlg.focusFirst()
    onClosed: {
        const target = opener ? opener : fallback
        if (target) target.forceActiveFocus()
    }

    Overlay.modal: Rectangle { color: Theme.scrim }
    // Fade plus an 8 px rise (a transform, so the centring anchor keeps its binding).
    property real rise: 0
    enter: Transition {
        ParallelAnimation {
            NumberAnimation { property: "opacity"; from: 0; to: 1; duration: Motion.base; easing.type: Easing.BezierSpline; easing.bezierCurve: Motion.decel }
            NumberAnimation { property: "rise"; from: Theme.s2; to: 0; duration: Motion.base; easing.type: Easing.BezierSpline; easing.bezierCurve: Motion.decel }
        }
    }
    exit: Transition {
        NumberAnimation { property: "opacity"; to: 0; duration: Motion.pageOut; easing.type: Easing.BezierSpline; easing.bezierCurve: Motion.accel }
    }
    background: Card { radius: Theme.rLg; fill: Theme.overlay; elevation: 3; transform: Translate { y: dlg.rise } }

    contentItem: ColumnLayout {
        spacing: 0
        transform: Translate { y: dlg.rise }
        Accessible.role: Accessible.Dialog
        Accessible.name: dlg.title
        ColumnLayout {
            Layout.fillWidth: true
            Layout.margins: Theme.padDialog
            Layout.bottomMargin: Theme.s3
            spacing: Theme.s1
            Txt { Layout.fillWidth: true; text: dlg.title; kind: "title"; Accessible.role: Accessible.Heading }
            Txt { Layout.fillWidth: true; visible: dlg.subtitle.length > 0; text: dlg.subtitle; color: Theme.textSecondary }
        }
        Flickable {
            id: flick
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.preferredHeight: bodyCol.implicitHeight
            Layout.minimumHeight: Math.min(bodyCol.implicitHeight, 120)
            contentWidth: width
            contentHeight: bodyCol.implicitHeight
            clip: true
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: SScrollBar {}
            ColumnLayout {
                id: bodyCol
                x: Theme.padDialog
                width: flick.width - 2 * Theme.padDialog
                spacing: Theme.s2
            }
        }
        Divider { Layout.topMargin: Theme.s2 }
        RowLayout {
            id: buttonRow
            Layout.fillWidth: true
            Layout.margins: Theme.s4
            Layout.leftMargin: Theme.padDialog; Layout.rightMargin: Theme.padDialog
            spacing: Theme.s2
        }
    }
}
