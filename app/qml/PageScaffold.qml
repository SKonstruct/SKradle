import QtQuick
import com.skradle.app
import QtQuick.Controls
import QtQuick.Layouts

// Scrolling page: centred column (max Theme.pageMax 960), 32 px gutters (24 below a 1000 px window), header on top,
// children appended below with Theme.gapSection between them. An empty `title` hides the page title (Home has none).
Item {
    id: root
    property string title
    property string subtitle
    default property alias content: col.data
    property alias actions: header.actions
    property alias scroller: sv
    readonly property int gutter: width < 1000 - 224 ? Theme.s6 : Theme.s8

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
                width: Math.min(Theme.pageMax, holder.width - 2 * root.gutter)
                x: (holder.width - width) / 2
                spacing: Theme.gapSection
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
