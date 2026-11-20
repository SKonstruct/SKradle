import QtQuick
import com.skradle.app
import QtQuick.Controls
import QtQuick.Layouts

PageScaffold {
    id: page
    title: qsTr("Home")
    subtitle: qsTr("Spiral Knights")

    // The changelog is the launcher's only release information: a plain button, with the version it covers.
    actions: SButton {
        text: qsTr("Release notes  ·  v%1").arg(AppEnv.version)
        glyph: "info"
        Accessible.name: qsTr("Release notes, version %1").arg(AppEnv.version)
        onClicked: Data.releaseNotes()
    }

    readonly property string st: LaunchModel.state
    // The hero shows while searching too, so the "not found" screen never flashes at start-up.
    readonly property bool found: GameModel.state !== "notfound"
    // The page is on screen (the host item fades out and is hidden when another page is current).
    readonly property bool shown: visible && enabled
    readonly property var news: { try { return JSON.parse(HomeFeed.newsJson) } catch (e) { return [] } }
    readonly property bool wide: Math.min(Theme.pageMax, page.width - 2 * Theme.s8) > 700
    function formatDate(secs) { return new Date(secs * 1000).toLocaleDateString(Qt.locale(), Locale.ShortFormat) }
    function openArticle(url) { if (url.indexOf("https://") === 0) Qt.openUrlExternally(url) }
    readonly property var players: { try { return JSON.parse(HomeFeed.playersJson) } catch (e) { return ({}) } }
    readonly property var events: { try { return JSON.parse(HomeFeed.eventsJson) } catch (e) { return [] } }
    readonly property bool showEvents: HomeFeed.eventsState === "ok" || HomeFeed.eventsState === "offline"
    function refresh() { HomeFeed.refreshNews(); HomeFeed.refreshPlayers(); HomeFeed.refreshEvents() }
    function number(n) { return Number(n).toLocaleString(Qt.locale(), "f", 0) }
    Component.onCompleted: refresh()
    onShownChanged: if (shown) { refresh(); tick++ }
    // Steam is asked for the player count at most once a minute, and only while this page is on screen.
    Timer { interval: 60000; repeat: true; running: page.shown; onTriggered: { HomeFeed.refreshPlayers(); page.tick++ } }

    // The event countdowns are recomputed every minute (and when the page comes back); `tick` is what they watch.
    property int tick: 0
    function span(d, h, m) {
        return d > 0 ? qsTr("%1d %2h").arg(d).arg(h) : h > 0 ? qsTr("%1h %2m").arg(h).arg(m) : qsTr("%1m").arg(m)
    }
    function countdownText(ev) {
        page.tick
        let c
        try { c = JSON.parse(HomeFeed.countdown(ev.startsAt, ev.endsAt)) } catch (e) { return "" }
        const none = c.days === 0 && c.hours === 0 && c.minutes === 0
        switch (c.kind) {
        case "starts": return none ? qsTr("Starts in under a minute") : qsTr("Starts in %1").arg(span(c.days, c.hours, c.minutes))
        case "none": return ""
        case "ends": return qsTr("Ends in %1").arg(span(c.days, c.hours, c.minutes))
        case "endsToday": return none ? qsTr("Ends today, under a minute left") : qsTr("Ends today, %1 left").arg(span(c.days, c.hours, c.minutes))
        }
        return qsTr("Ended")
    }

    readonly property bool busyRing: st === "launching" || (st === "updating" && !LaunchModel.determinate)

    // ---- hero / launch panel or first-run empty state -------------------------------------------------------
    Item {
        Layout.fillWidth: true
        implicitHeight: page.found ? hero.implicitHeight : nf.implicitHeight

        Card {
            id: hero
            width: parent.width
            implicitHeight: heroRow.implicitHeight + 2 * Theme.s6 + 4
            radius: Theme.rXl
            fill: Theme.card
            fillEnd: Theme.dark ? Qt.tint(Theme.card, Qt.rgba(0.671, 0.290, 0.506, 0.10)) : Qt.tint(Theme.card, Qt.rgba(0.671, 0.290, 0.506, 0.07))
            opacity: page.found ? 1 : 0
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
                    value: LaunchModel.progress
                    indeterminate: page.busyRing
                    color: page.st === "running" ? Theme.ok : Theme.accent
                    trackColor: page.st === "running" ? Qt.rgba(Theme.ok.r, Theme.ok.g, Theme.ok.b, 0.9) : Theme.track
                    active: page.visible
                    thickness: 8

                    // centre content, cross-faded per state
                    Logo {
                        anchors.centerIn: parent; size: 46
                        opacity: page.st === "ready" || page.st === "error" || page.busyRing ? 1 : 0
                        Behavior on opacity { NumberAnimation { duration: Motion.base } }
                    }
                    Txt {
                        anchors.centerIn: parent
                        text: Math.round(LaunchModel.progress * 100) + "%"
                        font.pixelSize: Theme.fsHeading + 4
                        font.weight: Font.DemiBold
                        font.features: { "tnum": 1 }
                        wrapMode: Text.NoWrap
                        opacity: page.st === "updating" && LaunchModel.determinate ? 1 : 0
                        Behavior on opacity { NumberAnimation { duration: Motion.base } }
                    }
                    Icon {
                        anchors.centerIn: parent; size: 40; stroke: 2.2; name: "check"; color: Theme.ok
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
                    FadeText {
                        Layout.fillWidth: true
                        kind: "title"
                        maximumLineCount: 2
                        text: page.st === "ready" ? qsTr("Ready to play")
                            : page.st === "error" ? qsTr("Could not start the game")
                            : page.st === "updating" ? (LaunchModel.step === "downloading" ? qsTr("Updating game files") : qsTr("Checking for updates"))
                            : page.st === "launching" ? qsTr("Starting the game") : qsTr("Game is running")
                    }
                    Txt {
                        Layout.fillWidth: true
                        maximumLineCount: 3
                        elide: Text.ElideRight
                        // The game's updater reports no byte counts outside its own window, so there is no
                        // "x of y MB" line: the ring spins and this says what the updater is doing.
                        text: page.st === "ready" ? qsTr("Game files are checked each time you launch.")
                            : page.st === "error" ? LaunchModel.message
                            : page.st === "updating" ? (LaunchModel.step === "downloading" ? qsTr("Downloading and verifying game files…") : qsTr("Looking for a newer version…"))
                            : page.st === "launching" ? qsTr("Preparing the Java runtime…")
                            : qsTr("The launcher stays open in the background.")
                        color: page.st === "error" ? Theme.danger : Theme.textSecondary
                        Accessible.role: page.st === "error" ? Accessible.AlertMessage : Accessible.StaticText
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
            opacity: page.found ? 0 : 1
            visible: opacity > 0.01
            Behavior on opacity { NumberAnimation { duration: Motion.base } }
            onSteam: {
                if (!Qt.openUrlExternally("steam://install/99900"))
                    Data.toast(qsTr("Steam did not open. Use the download page instead."))
            }
            onDownloadPage: Qt.openUrlExternally("https://www.spiralknights.com/download.xhtml")
        }
    }

    // ---- news, players, support ---------------------------------------------------------------------------
    GridLayout {
        Layout.fillWidth: true
        columns: page.wide ? 2 : 1
        columnSpacing: Theme.s4
        rowSpacing: Theme.s4

        ColumnLayout {   // events (when there are any) above the news
            Layout.fillWidth: true
            Layout.preferredWidth: page.wide ? 2 : 1
            Layout.alignment: Qt.AlignTop
            spacing: Theme.s4

        Card {   // live events
            id: eventsCard
            Layout.fillWidth: true
            visible: page.showEvents
            implicitHeight: eventsCol.implicitHeight + 2 * Theme.s5
            ColumnLayout {
                id: eventsCol
                anchors.left: parent.left; anchors.right: parent.right; anchors.top: parent.top
                anchors.margins: Theme.s5
                spacing: Theme.s3
                Txt { Layout.fillWidth: true; text: qsTr("Live events"); kind: "heading"; Accessible.role: Accessible.Heading }
                Txt {
                    Layout.fillWidth: true
                    visible: HomeFeed.eventsState === "offline"
                    kind: "caption"
                    color: Theme.amberText
                    text: HomeFeed.eventsFetchedAt > 0
                        ? qsTr("Offline. Showing the events saved on %1.").arg(page.formatDate(HomeFeed.eventsFetchedAt))
                        : qsTr("Offline. Showing saved events.")
                }
                Repeater {
                    model: page.events
                    Rectangle {
                        id: ev
                        required property int index
                        required property var modelData
                        Layout.fillWidth: true
                        implicitHeight: evCol.implicitHeight + 2 * Theme.s4
                        radius: Theme.rMd
                        color: "transparent"
                        border.width: 1
                        border.color: Theme.border
                        readonly property string when: page.countdownText(modelData)
                        readonly property bool upcoming: { page.tick; return modelData.startsAt * 1000 > Date.now() }
                        Accessible.role: Accessible.Grouping
                        Accessible.name: when !== "" ? modelData.title + ", " + when : modelData.title
                        ColumnLayout {
                            id: evCol
                            anchors.left: parent.left; anchors.right: parent.right; anchors.top: parent.top
                            anchors.margins: Theme.s4
                            spacing: Theme.s2
                            RowLayout {
                                Layout.fillWidth: true
                                spacing: Theme.s3
                                Txt { Layout.fillWidth: true; text: ev.modelData.title; font.weight: Font.DemiBold; maximumLineCount: 2; elide: Text.ElideRight }
                                Rectangle {
                                    visible: ev.when !== ""
                                    Layout.alignment: Qt.AlignTop
                                    implicitWidth: whenText.implicitWidth + 2 * Theme.s3
                                    implicitHeight: whenText.implicitHeight + Theme.s2
                                    radius: height / 2
                                    color: ev.upcoming ? Theme.raised : Theme.accentTint
                                    Txt {
                                        id: whenText
                                        anchors.centerIn: parent
                                        text: ev.when
                                        kind: "micro"
                                        color: Theme.text
                                        wrapMode: Text.NoWrap
                                        font.features: { "tnum": 1 }
                                    }
                                }
                            }
                            EventBanner {
                                Layout.fillWidth: true
                                source: ev.modelData.image
                                Accessible.ignored: true
                            }
                            Txt { Layout.fillWidth: true; visible: text !== ""; text: ev.modelData.description; kind: "caption" }
                            SButton {
                                visible: ev.modelData.link !== ""
                                text: qsTr("Learn more")
                                glyph: "globe"
                                Accessible.name: qsTr("Learn more about %1").arg(ev.modelData.title)
                                onClicked: page.openArticle(ev.modelData.link)
                            }
                        }
                    }
                }
            }
        }

        Card {   // Spiral Knights news
            id: newsCard
            Layout.fillWidth: true
            implicitHeight: newsCol.implicitHeight + 2 * Theme.s5
            ColumnLayout {
                id: newsCol
                anchors.left: parent.left; anchors.right: parent.right; anchors.top: parent.top
                anchors.margins: Theme.s5
                spacing: Theme.s2
                RowLayout {
                    Layout.fillWidth: true
                    Txt { Layout.fillWidth: true; text: qsTr("Spiral Knights news"); kind: "heading"; Accessible.role: Accessible.Heading }
                    SButton {
                        variant: "ghost"
                        text: qsTr("All news on Steam")
                        onClicked: Qt.openUrlExternally("https://store.steampowered.com/news/app/99900")
                    }
                }
                Txt {
                    Layout.fillWidth: true
                    visible: HomeFeed.newsState === "offline"
                    kind: "caption"
                    color: Theme.amberText
                    text: HomeFeed.newsFetchedAt > 0
                        ? qsTr("Offline. Showing the news saved on %1.").arg(page.formatDate(HomeFeed.newsFetchedAt))
                        : qsTr("Offline. Showing saved news.")
                    Accessible.role: Accessible.StaticText
                }
                Txt {
                    Layout.fillWidth: true
                    Layout.topMargin: Theme.s2
                    Layout.bottomMargin: Theme.s2
                    visible: HomeFeed.newsState === "loading" || HomeFeed.newsState === "unavailable"
                    text: HomeFeed.newsState === "loading" ? qsTr("Loading news…") : qsTr("The news needs an internet connection, and none is saved yet.")
                    color: Theme.textSecondary
                }
                Repeater {
                    model: page.news
                    ColumnLayout {
                        id: row
                        required property int index
                        required property var modelData
                        Layout.fillWidth: true
                        spacing: 0
                        Divider { visible: row.index > 0 }
                        AbstractButton {
                            id: item
                            Layout.fillWidth: true
                            implicitHeight: itemCol.implicitHeight + 2 * Theme.s3
                            hoverEnabled: true
                            focusPolicy: Qt.StrongFocus
                            property int radius: Theme.rMd
                            Accessible.role: Accessible.Link
                            Accessible.name: row.modelData.title + ", " + page.formatDate(row.modelData.date)
                            onClicked: page.openArticle(row.modelData.url)
                            background: Rectangle {
                                radius: Theme.rMd
                                color: item.down ? Theme.pressTint : item.hovered ? Theme.hoverTint : "transparent"
                                Behavior on color { ColorAnimation { duration: Motion.fast } }
                            }
                            contentItem: ColumnLayout {
                                id: itemCol
                                spacing: 2
                                RowLayout {
                                    Layout.fillWidth: true
                                    Layout.leftMargin: Theme.s2; Layout.rightMargin: Theme.s2
                                    spacing: Theme.s2
                                    Txt { text: row.modelData.label; kind: "micro"; visible: text !== ""; wrapMode: Text.NoWrap }
                                    Txt { text: "\u00b7"; kind: "micro"; visible: row.modelData.label !== ""; color: Theme.textTertiary; wrapMode: Text.NoWrap }
                                    Txt { text: page.formatDate(row.modelData.date); kind: "micro"; color: Theme.textTertiary; wrapMode: Text.NoWrap; Layout.fillWidth: true; elide: Text.ElideRight }
                                }
                                Txt { Layout.fillWidth: true; Layout.leftMargin: Theme.s2; Layout.rightMargin: Theme.s2; text: row.modelData.title; font.weight: Font.DemiBold; maximumLineCount: 2; elide: Text.ElideRight }
                                Txt { Layout.fillWidth: true; Layout.leftMargin: Theme.s2; Layout.rightMargin: Theme.s2; visible: text !== ""; text: row.modelData.teaser; kind: "caption"; maximumLineCount: 3; elide: Text.ElideRight }
                            }
                            FocusRing {}
                        }
                    }
                }
            }
        }
        }   // events and news column

        ColumnLayout {
            Layout.fillWidth: true
            Layout.preferredWidth: 1
            Layout.alignment: Qt.AlignTop
            spacing: Theme.s4

            Card {   // players online (an estimate)
                Layout.fillWidth: true
                implicitHeight: playersCol.implicitHeight + 2 * Theme.s5
                ColumnLayout {
                    id: playersCol
                    anchors.left: parent.left; anchors.right: parent.right; anchors.top: parent.top
                    anchors.margins: Theme.s5
                    spacing: Theme.s2
                    Accessible.role: Accessible.StaticText
                    Accessible.name: HomeFeed.playersState === "ok"
                        ? qsTr("About %1 players online right now (estimate)").arg(HomeFeed.players)
                        : qsTr("Players online: not available")
                    Txt { Layout.fillWidth: true; text: qsTr("Players online"); kind: "heading" }
                    RowLayout {
                        Layout.fillWidth: true
                        spacing: Theme.s2
                        Rectangle {
                            implicitWidth: 10; implicitHeight: 10; radius: 5
                            color: HomeFeed.playersState === "ok" ? Theme.ok : Theme.textTertiary
                            Layout.alignment: Qt.AlignVCenter
                        }
                        Txt {
                            Layout.fillWidth: true
                            text: HomeFeed.playersState === "ok" ? qsTr("About %1").arg(page.number(HomeFeed.players)) : "\u2013"
                            font.pixelSize: Theme.fsTitle
                            font.weight: Font.DemiBold
                            font.features: { "tnum": 1 }
                            wrapMode: Text.NoWrap
                        }
                    }
                    Txt {
                        Layout.fillWidth: true
                        kind: "caption"
                        text: HomeFeed.playersState === "ok" ? qsTr("players online right now (an estimate)")
                            : HomeFeed.playersState === "unavailable" ? qsTr("The player count is not available right now.")
                            : qsTr("Asking Steam…")
                    }
                    GridLayout {
                        Layout.fillWidth: true
                        Layout.topMargin: Theme.s2
                        visible: HomeFeed.playersState === "ok"
                        columns: 2
                        columnSpacing: Theme.s3
                        rowSpacing: 2
                        Txt { text: qsTr("Live"); kind: "caption"; font.weight: Font.DemiBold; color: Theme.text; Layout.alignment: Qt.AlignTop }
                        Txt {
                            Layout.fillWidth: true; kind: "caption"
                            text: qsTr("%1 on Steam, about %2 in total").arg(page.number(page.players.liveSteam || 0)).arg(page.number(page.players.liveTotal || 0))
                        }
                        Txt { text: qsTr("Preview"); kind: "caption"; font.weight: Font.DemiBold; color: Theme.text; Layout.alignment: Qt.AlignTop }
                        Txt {
                            Layout.fillWidth: true; kind: "caption"
                            text: qsTr("%1 on Steam, about %2 in total").arg(page.number(page.players.previewSteam || 0)).arg(page.number(page.players.previewTotal || 0))
                        }
                    }
                    Txt {
                        Layout.fillWidth: true
                        visible: HomeFeed.playersState === "ok"
                        kind: "caption"
                        color: Theme.textTertiary
                        text: qsTr("Steam counts only some of the players, so each Steam count is multiplied by %1 to include players of the standalone client.").arg(Number(page.players.factor || 1.4).toLocaleString(Qt.locale(), "f", 1))
                    }
                }
            }

            Card {   // support
                Layout.fillWidth: true
                implicitHeight: supportCol.implicitHeight + 2 * Theme.s5
                ColumnLayout {
                    id: supportCol
                    anchors.left: parent.left; anchors.right: parent.right; anchors.top: parent.top
                    anchors.margins: Theme.s5
                    spacing: Theme.s3
                    Txt { Layout.fillWidth: true; text: qsTr("Support SKradle"); kind: "heading"; Accessible.role: Accessible.Heading }
                    Txt { Layout.fillWidth: true; text: qsTr("SKradle is free and open source. If it makes playing easier, you can buy the developer a coffee."); kind: "caption" }
                    SButton {
                        Layout.fillWidth: true
                        highlighted: true
                        glyph: "coffee"
                        text: qsTr("Buy me a coffee")
                        onClicked: Qt.openUrlExternally("https://buymeacoffee.com/sirdankk")
                    }
                }
            }
        }
    }
}
