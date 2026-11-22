import QtQuick
import com.skradle.app
import QtQuick.Layouts

// Bulk actions for the selected mods. It takes the place of the search row (same height), so the list never moves.
Rectangle {
    id: root
    property int count: 0
    property bool canAct: true
    signal enable()
    signal disable()
    signal remove()
    signal clear()

    implicitHeight: Theme.hMd
    radius: Theme.rSm
    color: Theme.raised
    border.width: 1
    border.color: Theme.border
    Accessible.role: Accessible.StatusBar
    Accessible.name: qsTr("%n mod(s) selected", "", count)

    RowLayout {
        anchors.fill: parent
        anchors.leftMargin: Theme.s3
        anchors.rightMargin: Theme.s1
        spacing: Theme.gapInline
        Txt { Layout.fillHeight: true; verticalAlignment: Text.AlignVCenter; text: qsTr("%n selected", "", root.count); font.weight: Font.Medium; wrapMode: Text.NoWrap; numeric: true }
        Item { Layout.fillWidth: true }
        SButton { size: "sm"; text: qsTr("Enable"); enabled: root.canAct; onClicked: root.enable()
                  Accessible.name: qsTr("Enable the selected mods") }
        SButton { size: "sm"; text: qsTr("Disable"); enabled: root.canAct; onClicked: root.disable()
                  Accessible.name: qsTr("Disable the selected mods") }
        SButton { size: "sm"; text: qsTr("Remove"); variant: "danger"; enabled: root.canAct; onClicked: root.remove()
                  Accessible.name: qsTr("Remove the selected mods") }
        // the ghost button has no visible edge, so its inner padding is trimmed to put the text where the label
        // on the left sits (the bar's 12 px margin)
        SButton { size: "sm"; text: qsTr("Clear"); variant: "ghost"; leftPadding: Theme.s2; rightPadding: Theme.s2; implicitWidth: implicitContentWidth + leftPadding + rightPadding; onClicked: root.clear()
                  Accessible.name: qsTr("Clear the selection") }
    }
}
