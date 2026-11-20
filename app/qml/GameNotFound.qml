import QtQuick
import com.skradle.app
import QtQuick.Layouts
import QtQuick.Shapes
import QtQuick.Dialogs
import QtQuick.Controls

// First-run empty state: nothing to launch yet, two clear ways forward.
Card {
    id: root
    radius: Theme.rLg
    implicitHeight: col.implicitHeight + 2 * Theme.s10
    signal steam()
    signal downloadPage()

    ColumnLayout {
        id: col
        width: Math.min(parent.width - 2 * Theme.s8, 460)
        anchors.horizontalCenter: parent.horizontalCenter
        y: Theme.s10
        spacing: Theme.s3

        Item {
            Layout.alignment: Qt.AlignHCenter
            Layout.preferredWidth: 112; Layout.preferredHeight: 112
            Shape {   // dashed ring + folder glyph: "an empty slot"
                anchors.fill: parent
                preferredRendererType: Shape.CurveRenderer
                ShapePath {
                    strokeColor: Theme.accentFg
                    strokeWidth: 2
                    fillColor: Theme.accentTint
                    strokeStyle: ShapePath.DashLine
                    dashPattern: [3, 3.4]
                    PathAngleArc { centerX: 56; centerY: 56; radiusX: 52; radiusY: 52; startAngle: 0; sweepAngle: 360 }
                }
            }
            Logo { anchors.centerIn: parent; size: 64 }
            Rectangle {
                x: 70; y: 70; width: 30; height: 30; radius: Theme.rPill
                color: Theme.amber
                Icon { anchors.centerIn: parent; name: "question"; size: 20; stroke: 2.2; color: Theme.ink }
            }
        }
        Txt {
            Layout.fillWidth: true
            Layout.topMargin: Theme.s3
            horizontalAlignment: Text.AlignHCenter
            text: qsTr("Game not found")
            kind: "title"
            Accessible.role: Accessible.Heading
        }
        Txt {
            Layout.fillWidth: true
            horizontalAlignment: Text.AlignHCenter
            text: qsTr("SKradle could not find a Spiral Knights install. Download it now, or point to a copy you already have.")
            color: Theme.textSecondary
        }
        RowLayout {
            Layout.alignment: Qt.AlignHCenter
            Layout.topMargin: Theme.s4
            spacing: Theme.s3
            SButton {
                text: qsTr("Install with Steam")
                highlighted: true
                Accessible.name: text
                onClicked: root.steam()
            }
            SButton {
                text: qsTr("Locate existing install…")
                Accessible.name: text
                onClicked: folderDialog.open()
            }
        }
        AbstractButton {   // a link: underlined, link colour
            id: dl
            Layout.alignment: Qt.AlignHCenter
            implicitWidth: dlText.implicitWidth + 2 * Theme.s2
            implicitHeight: Theme.hSm
            hoverEnabled: true
            focusPolicy: Qt.StrongFocus
            property int radius: Theme.rXs
            text: qsTr("Or download it from spiralknights.com")
            Accessible.name: text
            Accessible.role: Accessible.Link
            onClicked: root.downloadPage()
            contentItem: Txt {
                id: dlText
                text: dl.text
                color: Theme.accentFg
                font.underline: true
                font.weight: dl.hovered ? Font.DemiBold : Font.Normal
                wrapMode: Text.NoWrap
                horizontalAlignment: Text.AlignHCenter
                verticalAlignment: Text.AlignVCenter
            }
            FocusRing { grow: 2 }
        }
        Txt {
            Layout.fillWidth: true
            Layout.topMargin: Theme.s2
            horizontalAlignment: Text.AlignHCenter
            text: qsTr("We looked in the default install locations.")
            kind: "micro"
            color: Theme.textSecondary
        }
    }

    FolderDialog {
        id: folderDialog
        title: qsTr("Choose the Spiral Knights folder")
        onAccepted: GameModel.addCustomFolder(selectedFolder)
    }
}
