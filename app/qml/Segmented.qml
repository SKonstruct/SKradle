import QtQuick
import com.skradle.app
import QtQuick.Controls

// Sliding single-choice control for 2-5 options, 28 high. One raised pill glides to the chosen segment (the edge
// that leads moves first, the other catches up; instant under reduced motion).
//   Segmented { model: [{ id: "all", label: qsTr("All"), badge: "12" }, ...]; currentId: "all"; onPicked: (id) => ... }
// Left/Right move the choice (Home/End jump); Tab enters and leaves as one stop. Text fills each segment's full height
// and centres vertically, so it does not depend on font metrics.
FocusScope {
    id: root
    property var model: []           // [{ id, label, badge? }]
    property var currentId
    property string label            // accessible name of the group
    signal picked(var id)

    implicitHeight: Theme.hSm
    implicitWidth: row.implicitWidth + 2
    activeFocusOnTab: true
    Accessible.role: Accessible.Grouping
    Accessible.name: label

    readonly property int currentIndex: {
        for (let i = 0; i < model.length; i++) if (model[i].id === currentId) return i
        return -1
    }
    function step(d) {
        if (model.length === 0) return
        const i = Math.max(0, Math.min(model.length - 1, Math.max(0, currentIndex) + d))
        if (i !== currentIndex) picked(model[i].id)
    }
    Keys.onLeftPressed: step(-1)
    Keys.onRightPressed: step(1)
    Keys.onPressed: (event) => {
        if (event.key === Qt.Key_Home) { step(-model.length); event.accepted = true }
        else if (event.key === Qt.Key_End) { step(model.length); event.accepted = true }
    }

    // The pill's two edges, followed separately so the leading one can move first.
    readonly property var seg: { row.implicitWidth; rep.count; return rep.itemAt(currentIndex) }
    readonly property real targetL: seg ? row.x + seg.x : 0
    readonly property real targetR: seg ? row.x + seg.x + seg.width : 0
    property real pillL: 0
    property real pillR: 0
    property bool settled: false
    // The edge that moves in the direction of travel leads, the other one trails.
    property int leftDur: Motion.base
    property int rightDur: Motion.base
    onTargetLChanged: { leftDur = targetL < pillL ? Motion.pillLead : Motion.pillTrail; pillL = targetL }
    onTargetRChanged: { rightDur = targetR > pillR ? Motion.pillLead : Motion.pillTrail; pillR = targetR }
    Component.onCompleted: { pillL = targetL; pillR = targetR; settled = true }
    Behavior on pillL { enabled: root.settled; NumberAnimation { duration: root.leftDur; easing.type: Easing.OutCubic } }
    Behavior on pillR { enabled: root.settled; NumberAnimation { duration: root.rightDur; easing.type: Easing.OutCubic } }

    Rectangle {   // the track
        anchors.fill: parent
        radius: Theme.rSm
        color: Theme.field
        border.width: 1
        border.color: Theme.fieldBorder
    }
    Rectangle {   // the pill; the inner radius is the outer minus the 1 px border
        visible: root.currentIndex >= 0
        x: root.pillL
        y: 1
        width: Math.max(0, root.pillR - root.pillL)
        height: parent.height - 2
        radius: Theme.rSm - 1
        color: Theme.raised
        border.width: 1
        border.color: Theme.border
    }
    Row {
        id: row
        x: 1; y: 1
        height: parent.height - 2
        Repeater {
            id: rep
            model: root.model
            AbstractButton {
                id: segBtn
                required property int index
                required property var modelData
                readonly property bool on: root.currentIndex === index
                height: row.height
                implicitWidth: content.implicitWidth + 2 * Theme.s3
                width: implicitWidth
                hoverEnabled: true
                focusPolicy: Qt.NoFocus
                Accessible.role: Accessible.RadioButton
                Accessible.name: (modelData.badge ? modelData.label + ", " + modelData.badge : modelData.label)
                Accessible.checked: on
                onClicked: root.picked(modelData.id)
                background: Rectangle {
                    radius: Theme.rSm - 1
                    color: !segBtn.on && segBtn.hovered ? Theme.stateHover : "transparent"
                    Behavior on color { ColorAnimation { duration: Motion.fast } }
                }
                contentItem: Item {
                    Row {
                        id: content
                        x: (parent.width - width) / 2
                        height: parent.height
                        spacing: Theme.s1
                        Txt {
                            height: parent.height
                            verticalAlignment: Text.AlignVCenter
                            text: segBtn.modelData.label
                            kind: "caption"
                            wrapMode: Text.NoWrap
                            font.weight: Font.Medium
                            color: segBtn.on ? Theme.text : Theme.textSecondary
                            Behavior on color { ColorAnimation { duration: Motion.fast } }
                        }
                        Txt {
                            height: parent.height
                            verticalAlignment: Text.AlignVCenter
                            visible: !!segBtn.modelData.badge
                            text: segBtn.modelData.badge || ""
                            kind: "micro"
                            numeric: true
                            wrapMode: Text.NoWrap
                            color: segBtn.on ? Theme.text : Theme.textSecondary
                        }
                    }
                }
            }
        }
    }
    FocusRing { active: root.activeFocus; grow: 3; radius: Theme.rSm + 3 }
}
