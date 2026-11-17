import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// Scrolling page: centred column (max 920), 32 px gutters, header on top, children appended below.
Item {
    id: root
    property string title
    property string subtitle
    default property alias content: col.data
    property alias actions: header.actions
    property alias scroller: sv

    Flickable {
        id: sv
        anchors.fill: parent
        contentWidth: width
        contentHeight: holder.implicitHeight
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: SScrollBar {}

        Item {
            id: holder
            width: sv.width
            implicitHeight: col.implicitHeight + Theme.s8 + Theme.s10
            ColumnLayout {
                id: col
                y: Theme.s8
                width: Math.min(Theme.pageMax, holder.width - 2 * Theme.s8)
                x: (holder.width - width) / 2
                spacing: Theme.s6
                PageHeader {
                    id: header
                    Layout.fillWidth: true
                    title: root.title
                    subtitle: root.subtitle
                }
            }
        }
    }
}
