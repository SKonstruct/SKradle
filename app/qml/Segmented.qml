import QtQuick
import com.skradle.app
import QtQuick.Controls

// Single-choice control for 2-5 options, 28 high, joined. Selected = raised fill + semibold text.
//   Segmented { model: [{ id: "all", label: qsTr("All"), badge: "12" }, ...]; currentId: "all"; onPicked: (id) => ... }
// Left/Right arrows move the choice; Tab enters and leaves as one stop.
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
        const i = (Math.max(0, currentIndex) + d + model.length) % model.length
        picked(model[i].id)
    }
    Keys.onLeftPressed: step(-1)
    Keys.onRightPressed: step(1)

    Rectangle {
        anchors.fill: parent
        radius: Theme.rSm
        color: Theme.field
        border.width: 1
        border.color: Theme.fieldBorder
    }
    Row {
        id: row
        x: 1; y: 1
        height: parent.height - 2
        Repeater {
            model: root.model
            AbstractButton {
                id: seg
                required property int index
                required property var modelData
                readonly property bool on: root.currentIndex === index
                height: row.height
                implicitWidth: content.implicitWidth + 2 * Theme.s3
                hoverEnabled: true
                focusPolicy: Qt.NoFocus
                Accessible.role: Accessible.RadioButton
                Accessible.name: (modelData.badge ? modelData.label + ", " + modelData.badge : modelData.label)
                Accessible.checked: on
                onClicked: root.picked(modelData.id)
                background: Rectangle {
                    // the inner radius is the outer minus the 1 px border
                    radius: Theme.rSm - 1
                    color: seg.on ? Theme.raised : seg.hovered ? Theme.stateHover : "transparent"
                    border.width: seg.on ? 1 : 0
                    border.color: Theme.border
                    Behavior on color { ColorAnimation { duration: Motion.fast } }
                }
                contentItem: Item {
                    Row {
                        id: content
                        anchors.centerIn: parent
                        spacing: Theme.s1
                        Txt {
                            text: seg.modelData.label
                            kind: "caption"
                            wrapMode: Text.NoWrap
                            font.weight: seg.on ? Font.DemiBold : Font.Medium
                            color: seg.on ? Theme.text : Theme.textSecondary
                            anchors.verticalCenter: parent.verticalCenter
                        }
                        Txt {
                            visible: !!seg.modelData.badge
                            text: seg.modelData.badge || ""
                            kind: "micro"
                            numeric: true
                            wrapMode: Text.NoWrap
                            color: Theme.textSecondary
                            anchors.verticalCenter: parent.verticalCenter
                        }
                    }
                }
            }
        }
    }
    FocusRing { active: root.activeFocus; grow: 3; radius: Theme.rSm + 3 }
}
