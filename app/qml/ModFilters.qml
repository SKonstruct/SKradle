import QtQuick
import com.skradle.app
import QtQuick.Layouts

// The two filters of the Mods page, kept apart: a status (one of All, Enabled, Disabled, Warnings) and a type (one of
// the types the installed mods have). The counts say what each choice would list given the other filter and the search.
//   statusModel: [{ id, label, badge }]   typeModel: [{ id, label }] (first entry: any type)
RowLayout {
    id: root
    property var statusModel: []
    property var typeModel: []
    property string status: "all"
    property string tag: ""
    signal statusPicked(string id)
    signal tagPicked(string id)
    spacing: Theme.s3

    Segmented {
        label: qsTr("Show by status")
        model: root.statusModel
        currentId: root.status
        onPicked: (id) => root.statusPicked(id)
    }
    SComboBox {
        id: typeBox
        visible: root.typeModel.length > 1
        Layout.preferredWidth: 168
        implicitHeight: Theme.hSm
        model: root.typeModel.map(t => t.label)
        currentIndex: Math.max(0, root.typeModel.findIndex(t => t.id === root.tag))
        Accessible.name: qsTr("Show by type")
        Accessible.description: displayText
        onActivated: (i) => root.tagPicked(root.typeModel[i].id)
    }
    Item { Layout.fillWidth: true }
}
