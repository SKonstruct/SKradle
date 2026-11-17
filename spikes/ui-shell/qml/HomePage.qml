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

    // ---- quick graphics preset ----------------------------------------------------------------------------
    ColumnLayout {
        Layout.fillWidth: true
        spacing: Theme.s4
        visible: Data.gameFound
        Txt { text: L.t("Quick graphics preset"); kind: "heading"; Accessible.role: Accessible.Heading }
        Card {
            Layout.fillWidth: true
            implicitHeight: presetRow.implicitHeight + 2 * Theme.s5
            property int preset: 1
            id: presetCard
            GridLayout {
                id: presetRow
                anchors.left: parent.left; anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                anchors.margins: Theme.s5
                columns: width > 640 ? 2 : 1
                columnSpacing: Theme.s5
                rowSpacing: Theme.s3
                ColumnLayout {
                    Layout.fillWidth: true
                    Layout.preferredWidth: 0
                    spacing: 4
                    Txt { Layout.fillWidth: true; text: L.t(Data.presets[presetCard.preset].name); font.weight: Font.Medium }
                    Txt { Layout.fillWidth: true; text: L.t(Data.presets[presetCard.preset].detail); kind: "caption" }
                }
                RowLayout {
                    Layout.alignment: Qt.AlignVCenter | Qt.AlignRight
                    spacing: Theme.s2
                    Repeater {
                        model: Data.presets
                        Chip {
                            required property int index
                            required property var modelData
                            text: L.t(modelData.name)
                            selected: presetCard.preset === index
                            onClicked: presetCard.preset = index
                        }
                    }
                    SButton { variant: "ghost"; text: L.t("Fine-tune"); glyph: "graphics"; onClicked: Data.navigate(3) }
                }
            }
        }
    }

    // ---- jump back in + mod updates -----------------------------------------------------------------------
    GridLayout {
        Layout.fillWidth: true
        visible: Data.gameFound
        columns: Math.min(Theme.pageMax, page.width - 2 * Theme.s8) > 700 ? 2 : 1
        columnSpacing: Theme.s4
        rowSpacing: Theme.s4
        uniformCellWidths: true

        Card {   // recent servers
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.alignment: Qt.AlignTop
            implicitHeight: srv.implicitHeight + 2 * Theme.s5
            ColumnLayout {
                id: srv
                anchors.left: parent.left; anchors.right: parent.right; anchors.top: parent.top
                anchors.margins: Theme.s5
                spacing: Theme.s2
                RowLayout {
                    Layout.fillWidth: true
                    Txt { Layout.fillWidth: true; text: L.t("Jump back in"); kind: "heading"; Accessible.role: Accessible.Heading }
                    SButton { variant: "ghost"; text: L.t("All servers"); onClicked: Data.navigate(1) }
                }
                Repeater {
                    model: Data.servers
                    RowLayout {
                        required property int index
                        required property string name
                        required property string status
                        required property int players
                        required property int ping
                        visible: index < 3
                        Layout.fillWidth: true
                        spacing: Theme.s3
                        StatusDot { color: status === "online" ? Theme.ok : status === "busy" ? Theme.amber : Theme.textTertiary; pulse: status === "online"; Layout.alignment: Qt.AlignVCenter }
                        ColumnLayout {
                            Layout.fillWidth: true
                            Layout.preferredWidth: 0
                            spacing: 0
                            Txt { Layout.fillWidth: true; text: name; font.weight: Font.Medium }
                            Txt { Layout.fillWidth: true; text: L.t("%1 players").arg(players.toLocaleString(Qt.locale(), "f", 0)) + "  \u00b7  " + ping + " ms"; kind: "caption"; font.features: { "tnum": 1 } }
                        }
                        SButton { variant: "secondary"; text: L.t("Join"); enabled: status !== "offline"; onClicked: Data.toast(L.t("Joining %1\u2026").arg(name)) }
                    }
                }
            }
        }

        Card {   // mod updates
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.alignment: Qt.AlignTop
            implicitHeight: mods.implicitHeight + 2 * Theme.s5
            ColumnLayout {
                id: mods
                anchors.left: parent.left; anchors.right: parent.right; anchors.top: parent.top
                anchors.margins: Theme.s5
                spacing: Theme.s2
                RowLayout {
                    Layout.fillWidth: true
                    Txt { Layout.fillWidth: true; text: L.t("Mods"); kind: "heading"; Accessible.role: Accessible.Heading }
                    SButton { variant: "ghost"; text: L.t("Manage"); onClicked: Data.navigate(2) }
                }
                Txt { Layout.fillWidth: true; text: L.t("5 of 9 enabled"); color: Theme.textSecondary }
                Txt {
                    Layout.fillWidth: true
                    Layout.topMargin: Theme.s2
                    visible: Data.modUpdates.length === 0
                    text: L.t("All mods are up to date.")
                    kind: "caption"
                }
                Repeater {
                    model: Data.modUpdates
                    RowLayout {
                        required property int index
                        required property var modelData
                        Layout.fillWidth: true
                        spacing: Theme.s3
                        Icon { name: "refresh"; size: 18; color: Theme.accentFg; Layout.alignment: Qt.AlignVCenter }
                        ColumnLayout {
                            Layout.fillWidth: true
                            Layout.preferredWidth: 0
                            spacing: 0
                            Txt { Layout.fillWidth: true; text: modelData.name; font.weight: Font.Medium }
                            Txt { Layout.fillWidth: true; text: "v" + modelData.from + "  \u2192  v" + modelData.to; kind: "caption"; font.features: { "tnum": 1 } }
                        }
                        SButton {
                            variant: "primary"
                            text: L.t("Update")
                            onClicked: {
                                const list = Data.modUpdates.slice()
                                list.splice(index, 1)
                                Data.modUpdates = list
                                Data.toast(L.t("%1 updated").arg(modelData.name))
                            }
                        }
                    }
                }
            }
        }
    }
}
