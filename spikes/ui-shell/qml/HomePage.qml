import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

PageScaffold {
    id: page
    title: L.t("Home")
    subtitle: L.t("Spiral Knights")

    readonly property string st: LaunchModel.state

    // ---- hero / launch panel or first-run empty state -------------------------------------------------------
    Item {
        Layout.fillWidth: true
        implicitHeight: Data.gameFound ? hero.implicitHeight : nf.implicitHeight

        Card {
            id: hero
            width: parent.width
            implicitHeight: heroRow.implicitHeight + 2 * Theme.s6 + 4
            radius: Theme.rXl
            fill: Theme.card
            fillEnd: Theme.dark ? Qt.tint(Theme.card, Qt.rgba(0.671, 0.290, 0.506, 0.10)) : Qt.tint(Theme.card, Qt.rgba(0.671, 0.290, 0.506, 0.07))
            opacity: Data.gameFound ? 1 : 0
            visible: opacity > 0.01
            Behavior on opacity { NumberAnimation { duration: Motion.base } }

            RowLayout {
                id: heroRow
                anchors.left: parent.left; anchors.right: parent.right
                anchors.margins: Theme.s6
                anchors.verticalCenter: parent.verticalCenter
                spacing: Theme.s5

                ProgressRing {
                    id: ring
                    Layout.alignment: Qt.AlignVCenter
                    implicitWidth: 100; implicitHeight: 100
                    value: LaunchModel.shown
                    indeterminate: page.st === "launching"
                    color: page.st === "running" ? Theme.ok : Theme.accent
                    trackColor: page.st === "running" ? Qt.rgba(Theme.ok.r, Theme.ok.g, Theme.ok.b, 0.9) : Theme.track
                    active: page.visible
                    thickness: 8

                    // centre content, cross-faded per state
                    Logo {
                        anchors.centerIn: parent; size: 46
                        opacity: page.st === "ready" || page.st === "launching" ? 1 : 0
                        Behavior on opacity { NumberAnimation { duration: Motion.base } }
                    }
                    Txt {
                        anchors.centerIn: parent
                        text: Math.round(LaunchModel.shown * 100) + "%"
                        font.pixelSize: Theme.fsHeading + 4
                        font.weight: Font.DemiBold
                        font.features: { "tnum": 1 }
                        wrapMode: Text.NoWrap
                        opacity: page.st === "updating" ? 1 : 0
                        Behavior on opacity { NumberAnimation { duration: Motion.base } }
                    }
                    Icon {
                        anchors.centerIn: parent; size: 40; stroke: 2.2; name: "check"; color: Theme.dark ? "#0F0F0F" : "#FFFFFF"
                        opacity: page.st === "running" ? 1 : 0
                        scale: page.st === "running" ? 1 : 0.7
                        Behavior on opacity { NumberAnimation { duration: Motion.base } }
                        Behavior on scale { NumberAnimation { duration: Motion.slow; easing.type: Easing.BezierSpline; easing.bezierCurve: Motion.decel } }
                    }
                }

                ColumnLayout {
                    Layout.fillWidth: true
                    Layout.minimumWidth: 140
                    Layout.alignment: Qt.AlignVCenter
                    spacing: 4
                    Txt { Layout.fillWidth: true; text: L.t("Spiral Knights"); kind: "caption"; wrapMode: Text.NoWrap; elide: Text.ElideRight }
                    FadeText {
                        Layout.fillWidth: true
                        kind: "title"
                        maximumLineCount: 2
                        text: page.st === "ready" ? L.t("Ready to play")
                            : page.st === "updating" ? L.t("Updating game files")
                            : page.st === "launching" ? L.t("Starting the game") : L.t("Game is running")
                    }
                    Item {
                        Layout.fillWidth: true
                        implicitHeight: Math.max(detail.implicitHeight, staticDetail.implicitHeight)
                        Txt {
                            id: detail
                            width: parent.width
                            opacity: page.st === "updating" ? 1 : 0
                            Behavior on opacity { NumberAnimation { duration: Motion.base } }
                            text: L.t("%1 of %2 MB · %3 MB/s").arg(LaunchModel.doneMB.toFixed(0)).arg(LaunchModel.totalMB).arg(LaunchModel.speed.toFixed(1))
                            color: Theme.textSecondary
                            font.features: { "tnum": 1 }
                        }
                        Txt {
                            id: staticDetail
                            width: parent.width
                            opacity: page.st === "updating" ? 0 : 1
                            Behavior on opacity { NumberAnimation { duration: Motion.base } }
                            property string shownText
                            text: page.st === "ready" ? L.t("Game files are up to date.")
                                : page.st === "launching" ? L.t("Preparing the Java runtime…")
                                : L.t("The launcher stays open in the background.")
                            color: Theme.textSecondary
                        }
                    }
                }

                LaunchButton {
                    Layout.alignment: Qt.AlignVCenter
                    Layout.preferredWidth: 236
                    Layout.preferredHeight: 64
                }
            }
        }

        GameNotFound {
            id: nf
            width: parent.width
            opacity: Data.gameFound ? 0 : 1
            visible: opacity > 0.01
            Behavior on opacity { NumberAnimation { duration: Motion.base } }
            onDownload: Data.toast(L.t("Download started"))
            onLocate: Data.toast(L.t("Choose the Spiral Knights folder"))
        }
    }

    // ---- news -------------------------------------------------------------------------------------------------
    ColumnLayout {
        Layout.fillWidth: true
        spacing: Theme.s4
        visible: Data.gameFound
        Txt { text: L.t("What is new"); kind: "heading"; Accessible.role: Accessible.Heading }
        GridLayout {
            id: grid
            Layout.fillWidth: true
            columns: Math.max(1, Math.min(3, Math.floor((Math.min(Theme.pageMax, page.width - 2 * Theme.s8) + columnSpacing) / (250 + columnSpacing))))
            columnSpacing: Theme.s4
            rowSpacing: Theme.s4
            uniformCellWidths: true
            Repeater {
                model: Data.news
                NewsCard {
                    required property var modelData
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    Layout.alignment: Qt.AlignTop
                    tag: L.t(modelData.tag)
                    title: L.t(modelData.title)
                    body: L.t(modelData.body)
                    date: L.t(modelData.date)
                    hue: modelData.hue
                    onClicked: Data.toast(L.t("Opening article…"))
                }
            }
        }
    }
}
