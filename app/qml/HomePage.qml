import QtQuick
import com.skradle.app
import QtQuick.Controls
import QtQuick.Layouts

PageScaffold {
    id: page
    // No page title: the hero below is the headline of Home.
    title: ""
    readonly property int tight: Theme.s4   // gap between the blocks of Home (the page default is looser)

    readonly property string st: LaunchModel.state
    // The hero shows while searching too, so the "not found" screen never flashes at start-up.
    readonly property bool found: GameModel.state !== "notfound"
    // The page is on screen (the host item fades out and is hidden when another page is current).
    readonly property bool shown: visible && enabled
    readonly property var news: { try { return JSON.parse(HomeFeed.newsJson) } catch (e) { return [] } }
    // Two columns only when each is wide enough (window >= 1060 px).
    readonly property bool wide: page.width >= 1060 - 224
    function formatDate(secs) { return new Date(secs * 1000).toLocaleDateString(Qt.locale(), Locale.ShortFormat) }
    function openArticle(url) { if (url.indexOf("https://") === 0) Qt.openUrlExternally(url) }
    readonly property var players: { try { return JSON.parse(HomeFeed.playersJson) } catch (e) { return ({}) } }
    readonly property var events: { try { return JSON.parse(HomeFeed.eventsJson) } catch (e) { return [] } }
    readonly property bool showEvents: HomeFeed.eventsState === "ok" || HomeFeed.eventsState === "offline"
    function refresh() { HomeFeed.refreshNews(); HomeFeed.refreshPlayers(); HomeFeed.refreshEvents() }
    function number(n) { return Number(n).toLocaleString(Qt.locale(), "f", 0) }
    Component.onCompleted: { refresh(); facts() }
    onShownChanged: if (shown) { refresh(); facts(); tick++ }
    // What the next launch will start: server, mods, Java. Each fact is shown only once it is known.
    function facts() {
        ServerBook.reload()
        if (!ModsModel.loaded && GameModel.state === "ready") ModsModel.refresh(GameModel.gameVersion)
        GameTools.refresh(GameModel.selectedPath)
    }
    Connections { target: GameModel; function onSelectedPathChanged() { page.facts() } }
    Connections { target: Settings; function onJavaPathChanged() { page.facts() } }
    readonly property int modsOn: { try { return JSON.parse(ModsModel.items).filter(m => m.enabled).length } catch (e) { return 0 } }
    readonly property string serverName: ServerBook.selectedName !== "" ? ServerBook.selectedName : qsTr("Official server")
    readonly property string readyLine: {
        const parts = [page.serverName]
        if (ModsModel.loaded) parts.push(qsTr("%n mod(s) on", "", page.modsOn))
        if (GameTools.javaVersion !== "") parts.push(qsTr("Java %1").arg(GameTools.javaVersion.split(".")[0]))
        return parts.join("  \u00b7  ")
    }
    // The launch failed because of Java: offer the fix (the Game section of Settings).
    readonly property bool javaIssue: st === "error" && GameTools.javaProblem !== ""
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
            implicitHeight: heroRow.implicitHeight + 2 * Theme.padCard
            radius: Theme.rLg
            fill: Theme.card
            opacity: page.found ? 1 : 0
            visible: opacity > 0.01
            Behavior on opacity { NumberAnimation { duration: Motion.base } }

            RowLayout {
                id: heroRow
                anchors.left: parent.left; anchors.right: parent.right
                anchors.margins: Theme.padCard
                anchors.verticalCenter: parent.verticalCenter
                spacing: Theme.s5

                ProgressRing {
                    id: ring
                    Layout.alignment: Qt.AlignVCenter
                    implicitWidth: 72; implicitHeight: 72
                    // At rest and on error the ring is closed, so it never reads as "0 %".
                    value: page.st === "ready" || page.st === "error" ? 1 : LaunchModel.progress
                    indeterminate: page.busyRing
                    color: page.st === "running" ? Theme.ok : page.st === "error" ? Theme.danger : Theme.accent
                    trackColor: page.st === "running" ? Theme.ok : Theme.track
                    active: page.visible
                    thickness: 6

                    // centre content, cross-faded per state
                    Logo {
                        anchors.centerIn: parent; size: 36
                        opacity: page.st === "ready" || page.busyRing ? 1 : 0
                        Behavior on opacity { NumberAnimation { duration: Motion.base } }
                    }
                    Txt {
                        anchors.fill: parent
                        horizontalAlignment: Text.AlignHCenter
                        text: Math.round(LaunchModel.progress * 100) + "%"
                        kind: "body"
                        font.weight: Font.DemiBold
                        numeric: true
                        wrapMode: Text.NoWrap
                        opacity: page.st === "updating" && LaunchModel.determinate ? 1 : 0
                        Behavior on opacity { NumberAnimation { duration: Motion.base } }
                    }
                    Icon {
                        anchors.centerIn: parent; size: 28; stroke: 2.2; name: "check"; color: Theme.ok
                        opacity: page.st === "running" ? 1 : 0
                        scale: page.st === "running" ? 1 : 0.7
                        Behavior on opacity { NumberAnimation { duration: Motion.base } }
                        Behavior on scale { NumberAnimation { duration: Motion.slow; easing.type: Easing.BezierSpline; easing.bezierCurve: Motion.decel } }
                    }
                    Icon {
                        anchors.centerIn: parent; size: 28; stroke: 2.2; name: "error"; color: Theme.danger
                        opacity: page.st === "error" ? 1 : 0
                        Behavior on opacity { NumberAnimation { duration: Motion.base } }
                    }
                }

                ColumnLayout {
                    Layout.fillWidth: true
                    Layout.minimumWidth: 140
                    Layout.alignment: Qt.AlignVCenter
                    spacing: Theme.s1
                    // The state is the headline; what it means is one quiet line below it.
                    FadeText {
                        Layout.fillWidth: true
                        kind: "title"
                        maximumLineCount: 2
                        text: page.st === "ready" ? qsTr("Ready to play")
                            : page.st === "error" ? qsTr("Could not start the game")
                            : page.st === "updating" && LaunchModel.step === "downloading" ? qsTr("Updating game files")
                            : (page.st === "updating" || page.st === "launching") && LaunchModel.step === "steam" ? qsTr("Starting Steam")
                            : (page.st === "updating" || page.st === "launching") && LaunchModel.step === "mods" ? qsTr("Applying your mods")
                            : (page.st === "updating" || page.st === "launching") && LaunchModel.step === "server" ? qsTr("Choosing the server")
                            : (page.st === "updating" || page.st === "launching") && LaunchModel.step === "starting" ? qsTr("Starting the game")
                            : page.st === "updating" || page.st === "launching" ? qsTr("Getting ready")
                            : qsTr("Game is running")
                    }
                    Txt {
                        Layout.fillWidth: true
                        maximumLineCount: 3
                        elide: Text.ElideRight
                        // The game's updater reports no byte counts outside its own window, so there is no
                        // "x of y MB" line: the ring spins and this says what is going on.
                        text: page.st === "ready" ? page.readyLine
                            : page.st === "error" ? LaunchModel.message
                            : page.st === "updating" && LaunchModel.step === "downloading" ? qsTr("Downloading and verifying game files\u2026")
                            : (page.st === "updating" || page.st === "launching") && LaunchModel.step === "steam" ? qsTr("Starting Steam and waiting for it to sign in\u2026")
                            : (page.st === "updating" || page.st === "launching") && LaunchModel.step === "mods" ? qsTr("Putting your %n enabled mod(s) into the game. This is skipped when nothing changed.", "", page.modsOn)
                            : (page.st === "updating" || page.st === "launching") && LaunchModel.step === "server" ? qsTr("Pointing the game at %1\u2026").arg(page.serverName)
                            : (page.st === "updating" || page.st === "launching") && LaunchModel.step === "starting" ? qsTr("Starting Spiral Knights on %1 with Java %2. The window opens in a moment.").arg(page.serverName).arg(GameTools.javaVersion !== "" ? GameTools.javaVersion.split(".")[0] : "")
                            : page.st === "updating" || page.st === "launching" ? qsTr("Checking Java and the game's files\u2026")
                            : qsTr("The launcher stays open in the background.")
                        color: Theme.textSecondary
                        Accessible.role: page.st === "error" ? Accessible.AlertMessage : Accessible.StaticText
                    }
                    SButton {
                        Layout.topMargin: Theme.s1
                        visible: page.javaIssue
                        size: "sm"
                        glyph: "graphics"
                        text: qsTr("Open Java settings")
                        onClicked: Data.navigateSection(4, 1)
                    }
                }

                LaunchButton {
                    Layout.alignment: Qt.AlignVCenter
                    Layout.preferredWidth: 236
                    Layout.preferredHeight: Theme.hLaunch
                }
                // Alt mode: another client next to the first, with little memory, for a second account.
                SButton {
                    id: altButton
                    Layout.alignment: Qt.AlignVCenter
                    visible: page.found && (page.st === "ready" || page.st === "running")
                    variant: "ghost"
                    size: "sm"
                    glyph: "plus"
                    text: qsTr("Alt client")
                    Accessible.name: qsTr("Start another client (alt mode)")
                    onClicked: LaunchModel.startAlt(GameModel.selectedPath)
                    Tip { text: qsTr("Start another client (alt mode). It uses little memory, so your main game keeps its speed. Holding Alt or Shift while pressing Launch does the same."); wanted: altButton.hovered }
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
                    Data.toast(qsTr("Steam did not open. Use the download page instead."), "error")
            }
            onDownloadPage: Qt.openUrlExternally("https://www.spiralknights.com/download.xhtml")
        }
    }

    // ---- the server's notice and maintenance window: only when it has something to say, and it can be closed ----------
    Banner {
        id: noticeBanner
        readonly property var n: { try { return HomeFeed.noticeJson === "" ? null : JSON.parse(HomeFeed.noticeJson) } catch (e) { return null } }
        property bool dismissed: false
        Connections { target: HomeFeed; function onNoticeJsonChanged() { noticeBanner.dismissed = false } }
        function when(sec) { return Qt.formatDateTime(new Date(sec * 1000), "ddd d MMM HH:mm") }
        readonly property bool underway: n !== null && n.startsAt > 0 && n.startsAt * 1000 <= Date.now()
        Layout.fillWidth: true
        Layout.topMargin: page.tight - Theme.gapSection
        visible: n !== null && !dismissed
        kind: "warn"
        dismissible: true
        onDismissed: dismissed = true
        title: n === null ? "" : n.title !== "" ? n.title : n.startsAt > 0 ? qsTr("Scheduled maintenance") : qsTr("Notice from the server")
        text: {
            if (n === null) return ""
            const lines = []
            if (n.startsAt > 0 && n.endsAt > 0) {
                lines.push(underway ? qsTr("Maintenance is under way. It should end around %1.").arg(when(n.endsAt))
                                    : qsTr("Maintenance is planned from %1 to about %2.").arg(when(n.startsAt)).arg(when(n.endsAt)))
            } else if (n.startsAt > 0) {
                lines.push(qsTr("Maintenance is planned for %1.").arg(when(n.startsAt)))
            }
            if (n.text !== "") lines.push(n.text)
            return lines.join("\n")
        }
    }

    // ---- players online: one slim strip ------------------------------------------------------------------------
    // Clicking it opens the history: the numbers over the last day, week and month.
    Card {
        id: playersCard
        Layout.fillWidth: true
        Layout.topMargin: page.tight - Theme.gapSection
        implicitHeight: playersGrid.implicitHeight + 2 * Theme.s3
        activeFocusOnTab: true
        Accessible.role: Accessible.Button
        Accessible.name: (HomeFeed.playersState === "ok"
            ? qsTr("About %1 players online right now (estimate)").arg(HomeFeed.players)
            : qsTr("Players online: not available")) + ". " + qsTr("Show the history")
        Accessible.onPressAction: playersDlg.openFrom(playersCard)
        Keys.onReturnPressed: playersDlg.openFrom(playersCard)
        Keys.onSpacePressed: playersDlg.openFrom(playersCard)
        Rectangle {   // hover tint
            anchors.fill: parent
            radius: playersCard.radius
            color: Theme.stateHover
            opacity: playersArea.containsMouse ? 1 : 0
            Behavior on opacity { NumberAnimation { duration: Motion.fast } }
        }
        MouseArea {
            id: playersArea
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: playersDlg.openFrom(playersCard)
        }
        FocusRing { visible: playersCard.activeFocus }
        Icon {
            anchors.right: parent.right; anchors.rightMargin: Theme.padCompact
            anchors.verticalCenter: parent.verticalCenter
            name: "chart"; size: 20
            color: playersArea.containsMouse || playersCard.activeFocus ? Theme.accentFg : Theme.textSecondary
        }
        GridLayout {
            id: playersGrid
            anchors.left: parent.left; anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            anchors.leftMargin: Theme.padCompact; anchors.rightMargin: Theme.padCompact + 32
            columns: page.wide ? 2 : 1
            columnSpacing: Theme.s5
            rowSpacing: Theme.s1
            RowLayout {
                Layout.fillWidth: true
                spacing: Theme.s2
                StatusDot {
                    color: HomeFeed.playersState === "ok" ? Theme.ok : Theme.textTertiary
                    pulse: HomeFeed.playersState === "ok"
                    Layout.alignment: Qt.AlignVCenter
                }
                Txt {
                    text: HomeFeed.playersState === "ok" ? qsTr("About %1").arg(page.number(HomeFeed.players)) : "\u2013"
                    kind: "heading"
                    numeric: true
                    wrapMode: Text.NoWrap
                }
                Txt {
                    Layout.fillWidth: HomeFeed.playersState !== "ok"
                    kind: "caption"
                    elide: Text.ElideRight
                    maximumLineCount: 1
                    text: HomeFeed.playersState === "ok" ? qsTr("players online now")
                        : HomeFeed.playersState === "unavailable" ? qsTr("The player count is not available right now.")
                        : qsTr("Asking Steam\u2026")
                }
                IconButton {
                    visible: HomeFeed.playersState === "ok"
                    size: "sm"; glyph: "info"; iconSize: 16
                    Layout.rightMargin: Theme.s0
                    tip: qsTr("Steam counts only some of the players, so each Steam count is multiplied by %1 to include players of the standalone client.").arg(Number(page.players.factor || 1.4).toLocaleString(Qt.locale(), "f", 1))
                }
                Item { Layout.fillWidth: HomeFeed.playersState === "ok" }
            }
            GridLayout {
                Layout.fillWidth: true
                visible: HomeFeed.playersState === "ok"
                columns: 2
                columnSpacing: Theme.s3
                rowSpacing: 0
                Txt { text: qsTr("Live"); kind: "caption"; font.weight: Font.DemiBold; color: Theme.text; Layout.alignment: Qt.AlignTop }
                Txt {
                    Layout.fillWidth: true; kind: "caption"; numeric: true
                    text: qsTr("%1 on Steam, about %2 in total").arg(page.number(page.players.liveSteam || 0)).arg(page.number(page.players.liveTotal || 0))
                }
                Txt { text: qsTr("Preview"); kind: "caption"; font.weight: Font.DemiBold; color: Theme.text; Layout.alignment: Qt.AlignTop }
                Txt {
                    Layout.fillWidth: true; kind: "caption"; numeric: true
                    text: qsTr("%1 on Steam, about %2 in total").arg(page.number(page.players.previewSteam || 0)).arg(page.number(page.players.previewTotal || 0))
                }
            }
        }
    }

    // ---- live events and news: two columns when there is room -----------------------------------------------------
    GridLayout {
        Layout.fillWidth: true
        Layout.topMargin: page.tight - Theme.gapSection
        columns: page.wide && page.showEvents ? 2 : 1
        columnSpacing: Theme.s4
        rowSpacing: Theme.s4

        Card {   // live events
            id: eventsCard
            Layout.fillWidth: true
            Layout.preferredWidth: 1
            Layout.alignment: Qt.AlignTop
            visible: page.showEvents
            implicitHeight: eventsCol.implicitHeight + 2 * Theme.padCompact
            ColumnLayout {
                id: eventsCol
                anchors.left: parent.left; anchors.right: parent.right; anchors.top: parent.top
                anchors.margins: Theme.padCompact
                spacing: Theme.s2
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
                    ColumnLayout {
                        id: ev
                        required property int index
                        required property var modelData
                        Layout.fillWidth: true
                        spacing: Theme.s2
                        readonly property string when: page.countdownText(modelData)
                        readonly property bool linked: modelData.link !== ""
                        readonly property bool upcoming: { page.tick; return modelData.startsAt * 1000 > Date.now() }
                        Accessible.role: Accessible.Grouping
                        Accessible.name: when !== "" ? modelData.title + ", " + when : modelData.title
                        Divider { visible: ev.index > 0; Layout.bottomMargin: Theme.s1 }
                        RowLayout {
                            Layout.fillWidth: true
                            spacing: Theme.s3
                            // The banner keeps its own proportions (800 x 550 in the feed): a thumbnail at the left,
                            // nothing cropped. No picture, no space.
                            // Up to 240 px, and never more than half the card.
                            EventBanner {
                                Layout.preferredWidth: shown ? Math.min(240, Math.round(ev.width * 0.46)) : 0
                                Layout.alignment: Qt.AlignTop
                                source: ev.modelData.image
                                Accessible.ignored: true
                                MouseArea {
                                    anchors.fill: parent
                                    enabled: ev.linked
                                    cursorShape: Qt.PointingHandCursor
                                    onClicked: page.openArticle(ev.modelData.link)
                                }
                            }
                            ColumnLayout {
                                Layout.fillWidth: true
                                Layout.alignment: Qt.AlignTop
                                spacing: Theme.s1
                                RowLayout {
                                    Layout.fillWidth: true
                                    spacing: Theme.s2
                                    // The title opens the article: it looks like a link (brand colour, underline on hover) and is one for the keyboard and screen readers.
                                    Item {
                                        id: titleLink
                                        Layout.fillWidth: true
                                        implicitHeight: titleRow.implicitHeight
                                        activeFocusOnTab: ev.linked
                                        Accessible.role: ev.linked ? Accessible.Link : Accessible.StaticText
                                        Accessible.name: ev.linked ? qsTr("%1, opens the article").arg(ev.modelData.title) : ev.modelData.title
                                        Accessible.onPressAction: if (ev.linked) page.openArticle(ev.modelData.link)
                                        Keys.onReturnPressed: if (ev.linked) page.openArticle(ev.modelData.link)
                                        Keys.onSpacePressed: if (ev.linked) page.openArticle(ev.modelData.link)
                                        RowLayout {
                                            id: titleRow
                                            anchors.left: parent.left
                                            anchors.right: parent.right
                                            spacing: Theme.s1
                                            Txt {
                                                Layout.fillWidth: true
                                                text: ev.modelData.title
                                                font.weight: Font.DemiBold
                                                font.underline: ev.linked && (titleArea.containsMouse || titleLink.activeFocus)
                                                color: ev.linked ? Theme.accentFg : Theme.text
                                                maximumLineCount: 2
                                                elide: Text.ElideRight
                                            }
                                        }
                                        MouseArea {
                                            id: titleArea
                                            anchors.fill: parent
                                            enabled: ev.linked
                                            hoverEnabled: true
                                            cursorShape: Qt.PointingHandCursor
                                            onClicked: page.openArticle(ev.modelData.link)
                                        }
                                        FocusRing { visible: titleLink.activeFocus }
                                    }
                                    Tag {
                                        visible: ev.when !== ""
                                        Layout.alignment: Qt.AlignTop
                                        text: ev.when
                                        kind: ev.upcoming ? "neutral" : "accent"
                                    }
                                }
                                Txt { Layout.fillWidth: true; visible: text !== ""; text: ev.modelData.description; kind: "caption"; paragraph: true; maximumLineCount: 3; elide: Text.ElideRight }
                            }
                        }
                    }
                }
            }
        }

        Card {   // Spiral Knights news
            id: newsCard
            Layout.fillWidth: true
            Layout.preferredWidth: 1
            Layout.alignment: Qt.AlignTop
            implicitHeight: newsCol.implicitHeight + 2 * Theme.padCompact
            ColumnLayout {
                id: newsCol
                anchors.left: parent.left; anchors.right: parent.right; anchors.top: parent.top
                anchors.margins: Theme.padCompact
                spacing: Theme.s1
                RowLayout {
                    Layout.fillWidth: true
                    Txt { Layout.fillWidth: true; text: qsTr("Spiral Knights news"); kind: "heading"; Accessible.role: Accessible.Heading }
                    IconButton {
                        glyph: "globe"
                        size: "sm"
                        tip: qsTr("All news on Steam")
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
                    visible: HomeFeed.newsState === "loading" || HomeFeed.newsState === "unavailable"
                    text: HomeFeed.newsState === "loading" ? qsTr("Loading news\u2026") : qsTr("The news needs an internet connection, and none is saved yet.")
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
                            implicitHeight: itemCol.implicitHeight + 2 * Theme.s2
                            hoverEnabled: true
                            focusPolicy: Qt.StrongFocus
                            property int radius: Theme.rSm
                            Accessible.role: Accessible.Link
                            Accessible.name: row.modelData.title + ", " + page.formatDate(row.modelData.date)
                            onClicked: page.openArticle(row.modelData.url)
                            background: Rectangle {
                                radius: Theme.rSm
                                color: item.down ? Theme.pressTint : item.hovered ? Theme.hoverTint : "transparent"
                                Behavior on color { ColorAnimation { duration: Motion.fast } }
                            }
                            contentItem: ColumnLayout {
                                id: itemCol
                                spacing: 0
                                RowLayout {
                                    Layout.fillWidth: true
                                    Layout.leftMargin: Theme.s2; Layout.rightMargin: Theme.s2
                                    spacing: Theme.s2
                                    Txt { text: row.modelData.label; kind: "micro"; visible: text !== ""; wrapMode: Text.NoWrap }
                                    Txt { text: "\u00b7"; kind: "micro"; visible: row.modelData.label !== ""; color: Theme.textTertiary; wrapMode: Text.NoWrap }
                                    Txt { text: page.formatDate(row.modelData.date); kind: "micro"; color: Theme.textTertiary; wrapMode: Text.NoWrap; Layout.fillWidth: true; elide: Text.ElideRight }
                                }
                                Txt { Layout.fillWidth: true; Layout.leftMargin: Theme.s2; Layout.rightMargin: Theme.s2; text: row.modelData.title; font.weight: Font.DemiBold; maximumLineCount: 2; elide: Text.ElideRight }
                                Txt { Layout.fillWidth: true; Layout.leftMargin: Theme.s2; Layout.rightMargin: Theme.s2; visible: text !== ""; text: row.modelData.teaser; kind: "caption"; paragraph: true; maximumLineCount: 2; elide: Text.ElideRight }
                            }
                            FocusRing {}
                        }
                    }
                }
            }
        }
    }

    // ---- footer line: support and release notes, not a card ---------------------------------------------------------
    RowLayout {
        Layout.fillWidth: true
        Layout.topMargin: page.tight - Theme.gapSection
        spacing: Theme.s2
        // "Built with <heart> by SirDank": quiet, but on the page everyone sees. The name opens the author's site.
        RowLayout {
            id: credit
            Layout.fillWidth: true
            spacing: Theme.s1
            Accessible.role: Accessible.Link
            Accessible.name: qsTr("Built with love by SirDank. Opens dankware.alwaysdata.net.")
            Accessible.onPressAction: Qt.openUrlExternally("https://dankware.alwaysdata.net/")
            Txt { text: qsTr("Built with"); kind: "caption"; wrapMode: Text.NoWrap; Accessible.ignored: true }
            Icon { name: "heart"; size: 14; color: Theme.accentFg; Accessible.ignored: true }
            Txt { text: qsTr("by"); kind: "caption"; wrapMode: Text.NoWrap; Accessible.ignored: true }
            Item {
                implicitWidth: nameText.implicitWidth
                implicitHeight: nameText.implicitHeight
                activeFocusOnTab: true
                Keys.onReturnPressed: Qt.openUrlExternally("https://dankware.alwaysdata.net/")
                Keys.onSpacePressed: Qt.openUrlExternally("https://dankware.alwaysdata.net/")
                Txt {
                    id: nameText
                    text: "SirDank"
                    kind: "caption"
                    wrapMode: Text.NoWrap
                    font.weight: Font.DemiBold
                    font.underline: nameArea.containsMouse || parent.activeFocus
                    color: Theme.accentFg
                    Accessible.ignored: true
                }
                MouseArea {
                    id: nameArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: Qt.openUrlExternally("https://dankware.alwaysdata.net/")
                }
                FocusRing { visible: parent.activeFocus }
            }
            Item { Layout.fillWidth: true }
        }
        // The changelog is the launcher's only release information: a plain button, with the version it covers.
        SButton {
            size: "sm"
            variant: "ghost"
            text: qsTr("Release notes (v%1)").arg(AppEnv.version)
            glyph: "info"
            Accessible.name: qsTr("Release notes, version %1").arg(AppEnv.version)
            onClicked: Data.releaseNotes()
        }
        SButton {
            size: "sm"
            variant: "coffee"
            glyph: "coffee"
            text: qsTr("Buy me a coffee")
            onClicked: Qt.openUrlExternally("https://buymeacoffee.com/sirdankk")
        }
    }

    // Screenshots: the history dialog.
    function openPlayers() { playersDlg.openFrom(playersCard) }
    function closePlayers() { playersDlg.close() }
    function hoverPlayers() { playersDlg.hoverDemo() }
    PlayersDialog { id: playersDlg }
    Connections {
        target: LaunchModel
        function onAltSerialChanged() {
            if (!page.shown) return
            if (LaunchModel.altNotice === "") Data.toast(qsTr("Another client is starting. It uses little memory, so it suits a second account."), "info")
            else Data.toast(LaunchModel.altNotice, "error")
        }
    }
}
