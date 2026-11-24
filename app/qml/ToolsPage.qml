import QtQuick
import com.skradle.app
import QtQuick.Layouts

// The game's editors (Crucible, scene editor, and the downloadable Silverweave, Model Viewer, Particle Editor and
// Interface Tester) and websites for Spiral Knights that open inside the launcher. The websites keep their sign-in and
// data between runs: the system web view stores cookies and local storage on disk (see docs/web-tools.md).
Item {
    id: page

    // The website shown full page ("" for the list).
    property string webTool: ""
    readonly property var webTools: [
        { id: "sk-ah", name: qsTr("Auction House"), site: "sk-ah.com", url: "https://sk-ah.com/",
          hint: qsTr("Live Spiral Knights Auction House prices: search by item, UVs and price, and set alerts.") },
        { id: "kozmas", name: qsTr("Kozmas"), site: "kozmas.gg", url: "https://www.kozmas.gg/",
          hint: qsTr("Find items, discover merchants with stock, and build your next costume.") }
    ]
    readonly property var current: { for (const t of webTools) if (t.id === webTool) return t; return null }

    function refreshTools() { GameTools.refresh(GameModel.selectedPath) }
    function back() { webTool = "" }
    // Screenshots: shows a website full page.
    function shot(name) { if (name === "web") webTool = "kozmas"; else webTool = "" }

    Component.onCompleted: { refreshTools(); EditorPack.check() }
    onVisibleChanged: { if (visible) refreshTools(); else webTool = "" }
    Connections { target: GameModel; function onSelectedPathChanged() { page.refreshTools() } }
    Connections { target: Settings; function onJavaPathChanged() { page.refreshTools() } }
    // A finished download (or an update of the editors) changes which of them can start.
    Connections { target: EditorPack; function onStateChanged() { if (EditorPack.state === "ready" || EditorPack.state === "none") page.refreshTools() } }
    Connections {
        target: GameTools
        function onNoticeSerialChanged() {
            if (!page.visible) return
            if (GameTools.notice === "editor-error") {
                console.warn("start editor:", GameTools.noticeDetail)
                Data.toast(qsTr("Could not start the editor. Check the Java setting in Settings > Game."), "error")
            } else if (GameTools.notice === "folder-error") {
                Data.toast(qsTr("Could not open the folder. Check that it still exists."), "error")
            }
        }
    }

    component Sep: Divider { Layout.leftMargin: Theme.padCard; Layout.rightMargin: Theme.padCard }

    // ---- the list ------------------------------------------------------------------------------------------------
    PageScaffold {
        anchors.fill: parent
        visible: page.webTool === ""
        title: qsTr("Tools")
        subtitle: qsTr("Websites for Spiral Knights that open here, and the game's editors.")

        SettingsSection {
            title: qsTr("Websites")
            Repeater {
                model: page.webTools
                ColumnLayout {
                    id: site
                    required property int index
                    required property var modelData
                    Layout.fillWidth: true
                    spacing: 0
                    Sep { visible: site.index > 0 }
                    SettingRow {
                        label: site.modelData.name + "  ·  " + site.modelData.site
                        hint: site.modelData.hint
                        Item { Layout.fillWidth: true }
                        IconButton {
                            glyph: "globe"
                            tip: qsTr("Open %1 in your browser").arg(site.modelData.site)
                            onClicked: Qt.openUrlExternally(site.modelData.url)
                        }
                        SButton {
                            text: qsTr("Open")
                            glyph: "play"
                            Accessible.name: qsTr("Open %1 here").arg(site.modelData.name)
                            onClicked: page.webTool = site.modelData.id
                        }
                    }
                }
            }
            Txt {
                Layout.fillWidth: true
                Layout.leftMargin: Theme.s5; Layout.rightMargin: Theme.s5; Layout.topMargin: Theme.s1; Layout.bottomMargin: Theme.s2
                text: AppEnv.webViewAvailable
                    ? qsTr("These open inside SKradle and remember your sign-in, like a browser would. The sites are not part of SKradle or the game.")
                    : qsTr("This computer has no built-in web view, so the sites open in your browser, which keeps your sign-in as usual.")
                kind: "caption"
                paragraph: true
            }
        }

        SettingsSection {
            title: qsTr("Game editors")
            SettingRow {
                label: qsTr("Scene editor, Silverweave, Model Viewer, Particle Editor, Interface Tester and datdec")
                hint: EditorPack.state === "downloading" ? qsTr("Downloading… %1%").arg(Math.round(EditorPack.progress * 100))
                    : EditorPack.state === "checking" ? qsTr("Looking for the editors…")
                    : EditorPack.state === "ready" ? qsTr("Downloaded (build %1). SKradle keeps them up to date by itself.").arg(EditorPack.build)
                    : EditorPack.state === "error" ? (EditorPack.reason === "unpublished" ? qsTr("The editors have not been published yet. Try again later.")
                        : EditorPack.reason === "ratelimit" ? qsTr("GitHub is limiting requests from this connection. Try again in about an hour.")
                        : EditorPack.reason === "offline" ? qsTr("Could not reach GitHub. Check the connection and try again.")
                        : EditorPack.reason === "server" ? qsTr("GitHub had a problem. Try again later.")
                        : EditorPack.reason === "checksum" ? qsTr("The download was damaged, so it was discarded. Try again.")
                        : qsTr("Could not download the editors."))
                    : qsTr("Open-source tools by Three Rings (Clyde), spiralview and datdec, downloaded once (about 25 MB) and kept up to date.")
                Item { Layout.fillWidth: true }
                SButton {
                    visible: EditorPack.state === "none" || EditorPack.state === "error"
                    text: EditorPack.state === "error" ? qsTr("Try again") : qsTr("Download")
                    glyph: "download"
                    Accessible.name: qsTr("Download the editors")
                    onClicked: EditorPack.install()
                }
            }
            Sep {}
            Repeater {
                model: [
                    { label: qsTr("Crucible scene editor"), hint: qsTr("The game's scene and template editor.") },
                    { label: qsTr("Scene editor"), hint: qsTr("Edit scenes, maps and prefabs.") },
                    { label: qsTr("Silverweave"), hint: qsTr("Try knight loadouts and items.") },
                    { label: qsTr("Model Viewer"), hint: qsTr("Look at the game's models and animations.") },
                    { label: qsTr("Particle Editor"), hint: qsTr("Edit the game's particle effects.") },
                    { label: qsTr("Interface Tester"), hint: qsTr("Try the game's interface layouts.") },
                    { label: qsTr("datdec"), hint: qsTr("Export and import the game's .dat and .xml files.") }
                ]
                ColumnLayout {
                    id: tool
                    required property int index
                    required property var modelData
                    readonly property string blocker: index < GameTools.editorBlockers.length ? GameTools.editorBlockers[index] : "game"
                    Layout.fillWidth: true
                    spacing: 0
                    Sep { visible: tool.index > 0 }
                    SettingRow {
                        label: tool.modelData.label
                        hint: tool.blocker === "game" ? qsTr("Needs a game install.")
                            : tool.blocker === "java" ? (GameTools.javaNeeded > 0 ? qsTr("Needs Java %1 or newer, which was not found.").arg(GameTools.javaNeeded) : qsTr("Needs Java, which was not found."))
                            : tool.blocker === "pack" ? qsTr("Download the editors above first.")
                            : tool.blocker === "javapack" ? qsTr("Needs Java %1 or newer, which was not found.").arg(EditorPack.javaMin)
                            : tool.blocker === "files" ? qsTr("The game folder lacks files this editor needs. Start the game once so it downloads them.")
                            : tool.modelData.hint
                        Item { Layout.fillWidth: true }
                        SButton {
                            text: qsTr("Open")
                            glyph: "play"
                            enabled: tool.blocker === ""
                            Accessible.name: qsTr("Open %1").arg(tool.modelData.label)
                            onClicked: GameTools.launchEditor(tool.index, GameModel.selectedPath)
                        }
                    }
                }
            }
        }

        SettingsSection {
            title: qsTr("Community tools")
            Repeater {
                model: [
                    { label: "Kozmadeus", url: "https://github.com/Crowfunder/Kozmadeus",
                      hint: qsTr("Imports 3D models (.dae, .obj) into the game's model format. A Windows program (GPL-3.0); other systems run it from source.") },
                    { label: "ThreeRingsSharp", url: "https://github.com/EtiTheSpirit/ThreeRingsSharp",
                      hint: qsTr("Exports the game's models and animations as glTF for Blender and other tools. A Windows program that is no longer updated.") },
                    { label: qsTr("Item tracker"), url: "https://github.com/Tranzorro/spiral_knights_tracking",
                      hint: qsTr("A community list of the game's items (JSON, free to use) and a companion app to track your collection.") }
                ]
                ColumnLayout {
                    id: community
                    required property int index
                    required property var modelData
                    Layout.fillWidth: true
                    spacing: 0
                    Sep { visible: community.index > 0 }
                    SettingRow {
                        label: community.modelData.label
                        hint: community.modelData.hint
                        Item { Layout.fillWidth: true }
                        SButton {
                            text: qsTr("Open on GitHub")
                            glyph: "github"
                            variant: "ghost"
                            Accessible.name: qsTr("Open %1 on GitHub").arg(community.modelData.label)
                            onClicked: Qt.openUrlExternally(community.modelData.url)
                        }
                    }
                }
            }
            Txt {
                Layout.fillWidth: true
                kind: "caption"
                color: Theme.textSecondary
                paragraph: true
                text: qsTr("These are made by other people and open in your browser. They are not part of SKradle or the game.")
            }
        }
    }


    // ---- one website, full page ----------------------------------------------------------------------------------
    ColumnLayout {
        id: webView
        anchors.fill: parent
        anchors.margins: page.width < 1000 - 224 ? Theme.s6 : Theme.s8
        anchors.topMargin: Theme.s8
        visible: page.webTool !== ""
        spacing: Theme.s4

        property bool failed: false
        readonly property bool embedded: AppEnv.webViewAvailable && !failed
        onVisibleChanged: if (visible) failed = false

        PageHeader {
            Layout.fillWidth: true
            title: page.current ? page.current.name : ""
            subtitle: page.current ? page.current.site : ""
            IconButton {
                glyph: "refresh"
                tip: qsTr("Reload the page")
                visible: webView.embedded
                onClicked: if (webLoader.item) webLoader.item.reload()
            }
            IconButton {
                glyph: "globe"
                tip: qsTr("Open in your browser")
                onClicked: if (page.current) Qt.openUrlExternally(page.current.url)
            }
            SButton { text: qsTr("Back to tools"); variant: "ghost"; onClicked: page.back(); Accessible.name: text }
        }

        Item {
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.minimumHeight: 240
            Card {
                anchors.fill: parent
                radius: Theme.rLg
                // Native web views are drawn above Qt Quick; keep a margin so the card border stays visible.
                Loader {
                    id: webLoader
                    anchors.fill: parent
                    anchors.margins: 2
                    active: page.visible && page.webTool !== "" && AppEnv.webViewAvailable && !webView.failed
                    source: "qrc:/qt/qml/com/skradle/app/web/WebPane.qml"
                    onLoaded: item.loadFailed.connect((why) => { console.warn("web view: " + why); webView.failed = true })
                    Binding { target: webLoader.item; property: "address"; value: page.current ? page.current.url : ""; when: webLoader.status === Loader.Ready }
                }
            }
            ColumnLayout {
                anchors.centerIn: parent
                width: Math.min(parent.width - 2 * Theme.s8, 440)
                spacing: Theme.s3
                visible: !AppEnv.webViewAvailable || webView.failed
                Txt {
                    Layout.fillWidth: true
                    horizontalAlignment: Text.AlignHCenter
                    text: AppEnv.webViewAvailable ? qsTr("The built-in web view could not show this site, so it opens in your browser.")
                                                   : qsTr("This computer has no built-in web view, so the site opens in your browser.")
                    color: Theme.textSecondary
                }
                SButton {
                    Layout.alignment: Qt.AlignHCenter
                    highlighted: true
                    text: qsTr("Open in browser")
                    onClicked: if (page.current) Qt.openUrlExternally(page.current.url)
                    Accessible.name: text
                }
            }
        }
    }
}
