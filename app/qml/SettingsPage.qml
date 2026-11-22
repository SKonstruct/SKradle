import QtQuick
import com.skradle.app
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Dialogs

// Every setting of the launcher on one scrolling page. Each control writes to the persisted settings and is applied
// the next time the game starts (Graphics, Game and Files) or at once (General, Integrations).
Item {
    id: page

    property alias scroller: sv
    property int currentSection: 0
    readonly property var sections: [secGeneral, secGame, secFiles, secGraphics, secIntegrations, secTools, secAbout]
    readonly property var sectionNames: [qsTr("General"), qsTr("Game"), qsTr("Files"), qsTr("Graphics"), qsTr("Integrations"), qsTr("Tools"), qsTr("About")]
    // A game that is launching or running locks the options that only apply at start-up and the file operations.
    readonly property bool gameBusy: LaunchModel.state !== "ready" && LaunchModel.state !== "error"
    readonly property bool hasGame: GameModel.selectedPath !== ""
    readonly property int memShown: Settings.memoryMb > 0 ? Settings.memoryMb : (GameModel.detectedMemoryMb > 0 ? GameModel.detectedMemoryMb : 2048)
    readonly property int gutter: width < 1000 - 224 ? Theme.s6 : Theme.s8
    readonly property int memMax: Math.max(GameTools.memoryMaxMb, memShown)

    function scrollTo(i) {
        const it = sections[i]
        if (!it) return
        const y = it.mapToItem(sv.contentItem, 0, 0).y
        scrollAnim.stop()
        scrollAnim.to = Math.max(0, Math.min(y - Theme.s4, sv.contentHeight - sv.height))
        scrollAnim.restart()
    }
    function openReset() { resetDialog.open() }
    function closeReset() { resetDialog.close() }
    function openCombo() { gcCombo.popup.open() }
    function closeCombo() { gcCombo.popup.close() }
    function focusSlider() { memSlider.forceActiveFocus(Qt.TabFocusReason) }
    function refreshTools() { GameTools.refresh(GameModel.selectedPath) }

    function updateCurrent() {
        let cur = 0
        for (let i = 0; i < sections.length; i++) {
            if (sections[i].mapToItem(sv.contentItem, 0, 0).y - 96 <= sv.contentY) cur = i
        }
        if (sv.contentY >= sv.contentHeight - sv.height - 2 && sv.contentHeight > sv.height) cur = sections.length - 1
        currentSection = cur
    }

    Component.onCompleted: refreshTools()
    onVisibleChanged: if (visible) refreshTools()
    Connections { target: GameModel; function onSelectedPathChanged() { page.refreshTools() } }
    Connections { target: Settings; function onJavaPathChanged() { page.refreshTools() } }
    Connections {
        target: GameTools
        function onNoticeSerialChanged() {
            // Errors say what failed and what to do; the engine's own text goes to the log, not the screen.
            const d = GameTools.noticeDetail
            switch (GameTools.notice) {
            case "reset-done": Data.toast(qsTr("Game files removed (%1). Launch the game to download them again.").arg(d), "success"); break
            case "reset-error": console.warn("reset game files:", d); Data.toast(qsTr("Could not reset the game files. Close the game and anything using its folder, then try again."), "error"); break
            case "editor-error": console.warn("start editor:", d); Data.toast(qsTr("Could not start the editor. Check the Java setting in Settings > Game."), "error"); break
            case "folder-error": console.warn("open folder:", d); Data.toast(qsTr("Could not open the folder. Check that it still exists."), "error"); break
            }
        }
    }

    // ---- small building blocks ---------------------------------------------------------------------------------
    component Sep: Divider { Layout.leftMargin: Theme.padCard; Layout.rightMargin: Theme.padCard }

    // A combo box whose rows map to setting values. A retranslation hands the box a new model and resets its index,
    // so the index is set again from the setting at start, on a new model and on a change.
    component ValueCombo: SComboBox {
        property var values: []
        property string current
        signal chosen(string value)
        Layout.fillWidth: true
        function sync() { currentIndex = Math.max(0, values.indexOf(current)) }
        onCurrentChanged: sync()
        onValuesChanged: sync()
        onModelChanged: sync()
        Component.onCompleted: sync()
        onActivated: (i) => chosen(values[i])
    }

    // A Banner inside a section card: indented like the rows. `warn` is the old spelling of kind "warn".
    component Notice: Banner {
        property bool warn: false
        property string icon: "info"
        kind: warn ? "warn" : "info"
        Layout.leftMargin: Theme.padCard; Layout.rightMargin: Theme.padCard; Layout.topMargin: Theme.s2; Layout.bottomMargin: Theme.s2
    }

    function kindLabel(kind) {
        return kind === "steam" ? qsTr("Steam") : kind === "standalone" ? qsTr("Standalone") : qsTr("Custom folder")
    }
    function lastPart(path) { const p = path.replace(/[\\/]+$/, "").split(/[\\/]/); return p[p.length - 1] }
    readonly property var installLabels: {
        const out = []
        const paths = GameModel.installPaths
        const kinds = GameModel.installKinds
        for (let i = 0; i < paths.length; i++) out.push(kinds[i] === "custom" ? kindLabel(kinds[i]) + ": " + lastPart(paths[i]) : kindLabel(kinds[i]))
        return out
    }
    readonly property bool hasCustom: GameModel.installKinds.indexOf("custom") >= 0

    readonly property string javaLine: {
        const p = GameTools.javaProblem
        if (!hasGame) return qsTr("Choose a game install first.")
        if (p === "invalid") return qsTr("That path is not a Java runtime.")
        if (p === "tooold") return qsTr("That Java is version %1; the game needs %2 or newer.").arg(GameTools.javaFound).arg(GameTools.javaNeeded)
        if (p !== "") return GameTools.javaNeeded > 0 ? qsTr("No Java %1 or newer was found.").arg(GameTools.javaNeeded) : qsTr("No Java was found.")
        if (GameTools.javaVersion === "") return qsTr("Looking for Java…")
        const where = ({ game: qsTr("the game's own runtime"), managed: qsTr("downloaded by SKradle"), env: qsTr("from JAVA_HOME"),
                         path: qsTr("found on this computer"), custom: qsTr("your choice") })[GameTools.javaSource] || ""
        return qsTr("Java %1 (%2)").arg(GameTools.javaVersion).arg(where)
    }
    // One sentence per update state; errors say what to do. Rate limit: GitHub's anonymous limit lifts within the hour.
    readonly property string updateLine: {
        const s = UpdateModel.state
        if (s === "uptodate") return qsTr("You are up to date.")
        if (s === "none") return qsTr("No releases yet. You are on the newest build.")
        if (s === "noinstaller") return qsTr("Version %1 is out, but not yet for this computer. Try again later.").arg(UpdateModel.version)
        if (s === "available") return qsTr("Version %1 is available.").arg(UpdateModel.version)
        if (s === "ready") return qsTr("Version %1 is downloaded and ready to install.").arg(UpdateModel.version)
        if (s === "installing") return qsTr("The installer is starting. SKradle closes now.")
        if (s !== "error") return ""
        const r = UpdateModel.reason
        if (UpdateModel.failedDuring === "install") return qsTr("Could not start the installer. Open the downloaded file from the updates folder yourself.")
        if (r === "ratelimit") return qsTr("GitHub is limiting requests from this connection. Try again in about an hour.")
        if (r === "offline") return qsTr("Could not reach GitHub. Check your internet connection and try again.")
        if (r === "server") return qsTr("GitHub is having trouble right now. Try again in a few minutes.")
        if (r === "malformed") return qsTr("GitHub's answer was not understood. Try again later.")
        if (r === "checksum") return qsTr("The download did not match its checksum and was discarded. Try again.")
        return UpdateModel.failedDuring === "download" ? qsTr("Could not download the update. Try again.") : qsTr("Could not check for updates. Try again.")
    }
    readonly property string agentLine: !Settings.graphicsEnabled
        ? qsTr("Off: the game starts with its own Java and no graphics agent, as the game ships. Turn it on to use the options below; they need Java 24 or newer with the java.instrument module.")
        : !GameTools.agentJavaOk
        ? qsTr("Graphics enhancements need Java 24 or newer with the java.instrument module. None was found, so the game starts without them. The game's own runtime cannot load them.")
        : !GameTools.agentInstalled
            ? qsTr("A suitable Java was found, but the graphics agent is not installed yet, so these options are saved and take effect once it is.")
            : qsTr("A suitable Java and the graphics agent were found. These options apply the next time the game starts.")

    // ---- header and jump links (fixed) -------------------------------------------------------------------------
    Item {
        id: top
        z: 2
        width: parent.width
        height: topCol.implicitHeight + Theme.s8 + Theme.s4
        Rectangle { anchors.fill: parent; color: Theme.bg }
        ColumnLayout {
            id: topCol
            y: Theme.s8
            width: Math.min(Theme.pageMax, top.width - 2 * page.gutter)
            x: (top.width - width) / 2
            spacing: Theme.s4
            PageHeader {
                Layout.fillWidth: true
                title: qsTr("Settings")
                subtitle: qsTr("Every option is saved and used the next time the game starts, unless it says otherwise.")
            }
            Flow {
                Layout.fillWidth: true
                spacing: Theme.s2
                Repeater {
                    model: page.sectionNames
                    SectionTab {
                        required property int index
                        required property string modelData
                        text: modelData
                        current: page.currentSection === index
                        onClicked: page.scrollTo(index)
                    }
                }
            }
        }
        Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Theme.border; opacity: sv.contentY > 4 ? 1 : 0; Behavior on opacity { NumberAnimation { duration: Motion.fast } } }
    }

    Flickable {
        id: sv
        anchors.top: top.bottom
        anchors.left: parent.left; anchors.right: parent.right; anchors.bottom: parent.bottom
        contentWidth: width
        contentHeight: holder.implicitHeight
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: SScrollBar {}
        onContentYChanged: page.updateCurrent()
        onContentHeightChanged: page.updateCurrent()
        NumberAnimation { id: scrollAnim; target: sv; property: "contentY"; duration: Motion.slow; easing.type: Easing.BezierSpline; easing.bezierCurve: Motion.standard }

        Item {
            id: holder
            width: sv.width
            implicitHeight: col.implicitHeight + Theme.s4 + Theme.s10
            ColumnLayout {
                id: col
                y: Theme.s4
                width: Math.min(Theme.pageMax, holder.width - 2 * page.gutter)
                x: (holder.width - width) / 2
                spacing: Theme.gapSection

                // ---- General ----------------------------------------------------------------------------------
                SettingsSection {
                    id: secGeneral
                    title: qsTr("General")
                    SettingRow {
                        label: qsTr("Appearance")
                        ValueCombo {
                            values: ["system", "light", "dark"]
                            model: [qsTr("Follow the system"), qsTr("Light"), qsTr("Dark")]
                            current: Settings.theme
                            onChosen: (v) => Settings.theme = v
                            Accessible.name: qsTr("Appearance")
                        }
                    }
                    Sep {}
                    SettingRow {
                        label: qsTr("Language")
                        ValueCombo {
                            // Each language is named in itself. Add a catalogue in i18n/ and a row here.
                            values: ["system", "en"]
                            model: [qsTr("Follow the system"), "English"]
                            current: Settings.language
                            onChosen: (v) => Settings.language = v
                            Accessible.name: qsTr("Language")
                        }
                    }
                    Sep {}
                    SettingRow {
                        label: qsTr("Reduce motion"); hint: qsTr("Turns off sliding and fading animations.")
                        Item { Layout.fillWidth: true }
                        SSwitch { checked: Settings.reduceMotion; onToggled: Settings.reduceMotion = checked; Accessible.name: qsTr("Reduce motion") }
                    }
                    Sep {}
                    SettingRow {
                        label: qsTr("Hide the launcher while the game runs"); hint: qsTr("It comes back when the game exits.")
                        Item { Layout.fillWidth: true }
                        SSwitch { checked: Settings.minimizeWhilePlaying; onToggled: Settings.minimizeWhilePlaying = checked; Accessible.name: qsTr("Hide the launcher while the game runs") }
                    }
                    Sep {}
                    SettingRow {
                        label: qsTr("Update automatically"); hint: qsTr("Looks for a newer SKradle a moment after it starts and downloads it in the background. You choose when to install. \"Check for updates\" in About works either way.")
                        Item { Layout.fillWidth: true }
                        SSwitch { checked: Settings.autoUpdate; onToggled: Settings.autoUpdate = checked; Accessible.name: qsTr("Update automatically") }
                    }
                    Sep {}
                    SettingRow {
                        label: qsTr("Send crash reports"); hint: qsTr("Error reports contain no names, folders or addresses.")
                        Item { Layout.fillWidth: true }
                        SSwitch { checked: Settings.crashReports; onToggled: Settings.crashReports = checked; Accessible.name: qsTr("Send crash reports") }
                    }
                }

                // ---- Game -------------------------------------------------------------------------------------
                SettingsSection {
                    id: secGame
                    title: qsTr("Game")
                    SettingRow {
                        label: qsTr("Game install"); hint: page.hasGame ? qsTr("Which copy of Spiral Knights SKradle starts.") : qsTr("No Spiral Knights install was found.")
                                                SComboBox {
                            id: installBox
                            Layout.fillWidth: true
                            enabled: GameModel.installPaths.length > 0
                            model: page.installLabels
                            function sync() { currentIndex = Math.max(0, GameModel.installPaths.indexOf(GameModel.selectedPath)) }
                            Component.onCompleted: sync()
                            onModelChanged: sync()
                            Connections { target: GameModel; function onSelectedPathChanged() { installBox.sync() } }
                            onActivated: (i) => GameModel.select(GameModel.installPaths[i])
                            Accessible.name: qsTr("Game install")
                        }
                    }
                    RowLayout {
                        Layout.fillWidth: true
                        Layout.leftMargin: Theme.padCard; Layout.rightMargin: Theme.padCard; Layout.bottomMargin: Theme.s3
                        spacing: Theme.s4
                        visible: page.hasGame
                        ColumnLayout {
                            Layout.fillWidth: true
                            Layout.preferredWidth: 0
                            spacing: Theme.s1
                            Txt { Layout.fillWidth: true; text: qsTr("Folder: %1").arg(GameModel.selectedPath); kind: "caption"; mono: true; elide: Text.ElideMiddle; wrapMode: Text.NoWrap }
                            Txt { Layout.fillWidth: true; text: GameModel.gameVersion !== "" ? qsTr("Game version: %1").arg(GameModel.gameVersion) : qsTr("Game version: unknown"); kind: "caption" }
                        }
                    }
                    RowLayout {
                        Layout.fillWidth: true
                        Layout.leftMargin: Theme.padCard; Layout.rightMargin: Theme.padCard; Layout.bottomMargin: Theme.s3
                        spacing: Theme.s2
                        SButton { text: qsTr("Add a folder…"); glyph: "plus"; onClicked: folderDialog.open() }
                        SButton { text: qsTr("Forget the custom folder"); visible: page.hasCustom; onClicked: GameModel.forgetCustom() }
                        SButton { text: qsTr("Open game folder"); glyph: "folder"; enabled: page.hasGame; onClicked: GameTools.openFolder(GameModel.selectedPath) }
                        Item { Layout.fillWidth: true }
                    }
                    Sep {}
                    SettingRow {
                        label: qsTr("Java"); hint: qsTr("Leave empty to let SKradle choose. A path to a Java folder or its java program overrides that.")
                                                STextField {
                            id: javaField
                            Layout.fillWidth: true; placeholderText: qsTr("Automatic")
                            onEditingFinished: Settings.javaPath = text.trim()
                            // Typing and "Browse…" both change the text; the setting is the source of truth.
                            Component.onCompleted: text = Settings.javaPath
                            Connections { target: Settings; function onJavaPathChanged() { if (javaField.text.trim() !== Settings.javaPath) javaField.text = Settings.javaPath } }
                            Accessible.name: qsTr("Java path")
                        }
                        SButton { text: qsTr("Browse…"); onClicked: javaDialog.open() }
                    }
                    Notice {   // one banner for Java: what is in use, and whether it can run the graphics agent
                        text: GameTools.javaProblem !== "" || !page.hasGame || GameTools.javaVersion === "" || !Settings.graphicsEnabled ? page.javaLine
                            : GameTools.agentJavaOk ? qsTr("%1. Supports the graphics agent.").arg(page.javaLine)
                            : qsTr("%1. Too old for the graphics agent, which needs Java 24 or newer.").arg(page.javaLine)
                        kind: page.hasGame && GameTools.javaProblem !== "" ? "warn" : page.hasGame && GameTools.javaVersion !== "" && Settings.graphicsEnabled && GameTools.agentJavaOk ? "success" : "info"
                    }
                    Sep {}
                    SettingRow {
                        label: qsTr("Memory"); hint: qsTr("Megabytes the game may use (its heap limit).")
                        Layout.bottomMargin: Theme.s0
                        Item { Layout.fillWidth: true }
                        SSpinBox {
                            from: GameTools.memoryMinMb; to: page.memMax; stepSize: 256; editable: true
                            onValueModified: Settings.memoryMb = value
                            Binding on value { value: page.memShown }
                            Accessible.name: qsTr("Memory in megabytes, exact value")
                        }
                    }
                    SSlider {
                        id: memSlider
                        Layout.fillWidth: true
                        Layout.leftMargin: Theme.padCard; Layout.rightMargin: Theme.padCard
                        from: GameTools.memoryMinMb; to: page.memMax; stepSize: 256
                        snapMode: SSlider.SnapAlways
                        onMoved: Settings.memoryMb = Math.round(value)
                        Binding on value { value: page.memShown }
                        Accessible.name: qsTr("Memory in megabytes")
                    }
                    RowLayout {
                        Layout.fillWidth: true
                        Layout.leftMargin: Theme.padCard; Layout.rightMargin: Theme.padCard; Layout.bottomMargin: Theme.s3
                        spacing: Theme.s3
                        Txt {
                            Layout.fillWidth: true
                            Layout.preferredWidth: 0
                            kind: "caption"
                            text: {
                                const parts = []
                                if (GameModel.detectedMemoryMb > 0) parts.push(qsTr("The game's own setting: %1 MB").arg(GameModel.detectedMemoryMb))
                                if (GameTools.totalMemoryMb > 0) parts.push(qsTr("This computer has %1 MB").arg(GameTools.totalMemoryMb))
                                parts.push(Settings.memoryMb > 0 ? qsTr("Your choice: %1 MB").arg(Settings.memoryMb) : qsTr("You have not changed it, so the game keeps its own setting."))
                                return parts.join("  ·  ")
                            }
                        }
                        SButton { variant: "ghost"; text: qsTr("Use the game's setting"); visible: Settings.memoryMb > 0; onClicked: Settings.memoryMb = 0 }
                    }
                    Sep {}
                    SettingRow {
                        label: qsTr("Garbage collector"); hint: qsTr("How the Java runtime frees memory. Default leaves the game's own choice.")
                        ValueCombo {
                            id: gcCombo
                            values: ["default", "g1", "zgc", "parallel", "serial"]
                            model: [qsTr("Default"), qsTr("G1"), qsTr("ZGC"), qsTr("Parallel"), qsTr("Serial")]
                            current: Settings.collector
                            onChosen: (v) => Settings.collector = v
                            Accessible.name: qsTr("Garbage collector")
                        }
                    }
                    Sep {}
                    SettingRow {
                        label: qsTr("Disable explicit garbage collection"); hint: qsTr("Ignores the game's own requests to collect garbage, which avoids some stop-the-world pauses.")
                        Item { Layout.fillWidth: true }
                        SSwitch { checked: Settings.disableExplicitGc; onToggled: Settings.disableExplicitGc = checked; Accessible.name: qsTr("Disable explicit garbage collection") }
                    }
                    Sep {}
                    SettingRow {
                        label: qsTr("Extra Java arguments"); hint: qsTr("Added last, so they win over the options above. Separate with spaces; quote values that contain spaces.")
                                                STextField {
                            id: argsField
                            Layout.fillWidth: true; placeholderText: qsTr("None")
                            onEditingFinished: Settings.extraJvmArgs = text.trim()
                            Component.onCompleted: text = Settings.extraJvmArgs
                            Connections { target: Settings; function onExtraJvmArgsChanged() { if (argsField.text.trim() !== Settings.extraJvmArgs) argsField.text = Settings.extraJvmArgs } }
                            Accessible.name: qsTr("Extra Java arguments")
                        }
                    }
                    Notice {
                        text: qsTr("Memory, collector and arguments are used when SKradle starts the game itself. A launch that first updates the game runs the game's own updater, which uses the game's own settings (only the memory limit is passed on).")
                    }
                }

                // ---- Files ------------------------------------------------------------------------------------
                SettingsSection {
                    id: secFiles
                    title: qsTr("Files")
                    SettingRow {
                        label: qsTr("File cleaning"); hint: qsTr("Applies mods on top of the game's original files, so removing a mod puts the original back. Turn it off only to keep changes from other tools.")
                        Item { Layout.fillWidth: true }
                        SSwitch { checked: Settings.fileCleaning; onToggled: Settings.fileCleaning = checked; Accessible.name: qsTr("File cleaning") }
                    }
                    Sep {}
                    SettingRow {
                        label: qsTr("File purging"); hint: qsTr("Before applying mods, removes files that earlier mods added to the game folder. Only files SKradle recorded itself are removed, never anything else.")
                        Item { Layout.fillWidth: true }
                        SSwitch { checked: Settings.filePurging; onToggled: Settings.filePurging = checked; Accessible.name: qsTr("File purging") }
                    }
                    Sep {}
                    SettingRow {
                        label: qsTr("Reset game files"); hint: qsTr("Removes every applied mod and the game's files, so the game's own updater downloads them all again. Your mods folder and settings stay.")
                                                Item { Layout.fillWidth: true }
                        SButton {
                            variant: "danger"
                            glyph: "refresh"
                            text: GameTools.busy ? qsTr("Working…") : qsTr("Reset game files…")
                            enabled: page.hasGame && !page.gameBusy && !GameTools.busy
                            onClicked: resetDialog.open()
                        }
                    }
                    Notice {
                        visible: page.gameBusy
                        warn: true
                        text: qsTr("The game is starting or running. Close it to change game files.")
                    }
                }

                // ---- Graphics ---------------------------------------------------------------------------------
                SettingsSection {
                    id: secGraphics
                    title: qsTr("Graphics")
                    SettingRow {
                        label: qsTr("Enable graphics enhancements"); hint: qsTr("Adds SKradle's graphics agent to the game. Off by default.")
                        Item { Layout.fillWidth: true }
                        SSwitch { checked: Settings.graphicsEnabled; onToggled: Settings.graphicsEnabled = checked; Accessible.name: qsTr("Enable graphics enhancements") }
                    }
                    Notice { text: page.agentLine; warn: Settings.graphicsEnabled && !GameTools.agentJavaOk }
                    Notice { visible: LaunchModel.state === "running"; warn: true; text: qsTr("The game is running. Close it to change rendering options.") }
                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 0
                        enabled: LaunchModel.state !== "running" && Settings.graphicsEnabled
                        opacity: LaunchModel.state === "running" || !Settings.graphicsEnabled ? 0.5 : 1
                        Behavior on opacity { NumberAnimation { duration: Motion.base } }
                        SettingRow {
                            label: qsTr("Render scale"); hint: qsTr("Lower values trade sharpness for speed.")
                            SSlider {
                                id: scale; Layout.fillWidth: true; from: 50; to: 200
                                onMoved: Settings.renderScale = Math.round(value)
                                // A Binding element keeps following the setting (a clamp, a reload) after the player has moved the handle.
                                Binding on value { value: Settings.renderScale }
                                Accessible.name: qsTr("Render scale")
                            }
                            Txt { Layout.preferredWidth: Theme.s12; horizontalAlignment: Text.AlignRight; text: Math.round(scale.value) + "%"; numeric: true; wrapMode: Text.NoWrap }
                        }
                        Sep {}
                        SettingRow {
                            label: qsTr("Sharpening"); hint: qsTr("Crisper edges after scaling.")
                            SSlider {
                                id: sharp; Layout.fillWidth: true; from: 0; to: 100
                                onMoved: Settings.sharpening = Math.round(value)
                                Binding on value { value: Settings.sharpening }
                                Accessible.name: qsTr("Sharpening")
                            }
                            Txt { Layout.preferredWidth: Theme.s12; horizontalAlignment: Text.AlignRight; text: Math.round(sharp.value); numeric: true; wrapMode: Text.NoWrap }
                        }
                        Sep {}
                        SettingRow {
                            label: qsTr("Sharp HUD"); hint: qsTr("Draw the interface at full resolution.")
                            Item { Layout.fillWidth: true }
                            SSwitch { checked: Settings.sharpHud; onToggled: Settings.sharpHud = checked; Accessible.name: qsTr("Sharp HUD") }
                        }
                        Sep {}
                        SettingRow {
                            label: qsTr("FPS counter")
                            Item { Layout.fillWidth: true }
                            SSwitch { checked: Settings.showFps; onToggled: Settings.showFps = checked; Accessible.name: qsTr("FPS counter") }
                        }
                        Sep {}
                        SettingRow {
                            label: qsTr("Frame generation"); hint: qsTr("Experimental. Draws extra frames to reach the target frame rate.")
                            Item { Layout.fillWidth: true }
                            SSwitch { checked: Settings.frameGenMode === 1; onToggled: Settings.frameGenMode = checked ? 1 : 0; Accessible.name: qsTr("Frame generation") }
                        }
                        Sep {}
                        SettingRow {
                            label: qsTr("Frame cap"); hint: qsTr("Frames per second that frame generation aims for.")
                            Item { Layout.fillWidth: true }
                            SSpinBox {
                                from: 30; to: 360; editable: true
                                enabled: Settings.frameGenMode === 1
                                onValueModified: Settings.targetFps = value
                                Binding on value { value: Settings.targetFps }
                                Accessible.name: qsTr("Frame cap")
                            }
                        }
                    }
                }

                // ---- Integrations -----------------------------------------------------------------------------
                SettingsSection {
                    id: secIntegrations
                    title: qsTr("Integrations")
                    SettingRow {
                        label: qsTr("Discord Rich Presence")
                        hint: AppEnv.discordConfigured ? qsTr("Shows in Discord that you are playing Spiral Knights, and for how long. Off by default; Discord must be running.")
                            : qsTr("Not available in this build.")
                        Item { Layout.fillWidth: true }
                        SSwitch {
                            checked: Settings.discordPresence
                            enabled: AppEnv.discordConfigured
                            onToggled: Settings.discordPresence = checked
                            Accessible.name: qsTr("Discord Rich Presence")
                        }
                    }
                }

                // ---- Tools ------------------------------------------------------------------------------------
                SettingsSection {
                    id: secTools
                    title: qsTr("Tools")
                    Notice { text: qsTr("The game ships two editors. Its Model Viewer, Particle Editor and Interface Tester are not in the game's files (only their texts are), so they cannot be started. Models, particles and interfaces can be edited in the Scene editor with its \"Configs...\" command. Silverweave is an item in the game, not a tool.") }
                    Repeater {
                        model: [
                            { label: qsTr("Crucible scene editor"), hint: qsTr("The game's scene and template editor.") },
                            { label: qsTr("Scene editor"), hint: qsTr("The game's own map and prefab editor. Its name inside the game may change with an update.") }
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

                // ---- About ------------------------------------------------------------------------------------
                SettingsSection {
                    id: secAbout
                    title: qsTr("About")
                    RowLayout {
                        Layout.fillWidth: true
                        Layout.leftMargin: Theme.padCard; Layout.rightMargin: Theme.padCard; Layout.topMargin: Theme.s3; Layout.bottomMargin: Theme.s3
                        spacing: Theme.s5
                        Logo { size: 56; Layout.alignment: Qt.AlignVCenter }
                        ColumnLayout {
                            Layout.fillWidth: true
                            Layout.preferredWidth: 0
                            spacing: Theme.s1
                            Txt { Layout.fillWidth: true; text: "SKradle"; kind: "heading" }
                            Txt { Layout.fillWidth: true; text: qsTr("Version %1  ·  Qt %2").arg(AppEnv.version).arg(AppEnv.qtVersion); kind: "caption" }
                        }
                        ColumnLayout {
                            Layout.alignment: Qt.AlignVCenter
                            spacing: Theme.s2
                            SButton {
                                Layout.alignment: Qt.AlignRight
                                highlighted: UpdateModel.state === "available" || UpdateModel.state === "ready"
                                enabled: UpdateModel.state !== "checking" && UpdateModel.state !== "downloading"
                                text: UpdateModel.state === "checking" ? qsTr("Checking…")
                                    : UpdateModel.state === "available" ? qsTr("Download %1").arg(UpdateModel.version)
                                    : UpdateModel.state === "downloading" ? qsTr("Downloading… %1%").arg(Math.round(UpdateModel.progress * 100))
                                    : UpdateModel.state === "ready" ? qsTr("Install and restart")
                                    : qsTr("Check for updates")
                                onClicked: {
                                    const s = UpdateModel.state
                                    if (s === "available") UpdateModel.download()
                                    else if (s === "ready") UpdateModel.install()
                                    else UpdateModel.check()
                                }
                            }
                            Txt {
                                Layout.alignment: Qt.AlignRight
                                Layout.maximumWidth: 300
                                horizontalAlignment: Text.AlignRight
                                kind: "caption"
                                visible: text !== ""
                                color: UpdateModel.state === "error" ? Theme.danger : Theme.textSecondary
                                text: page.updateLine
                            }
                        }
                    }
                    Notice { text: qsTr("Unofficial fan project. Not affiliated with Grey Havens. Spiral Knights © Grey Havens, LLC.") }
                    RowLayout {
                        Layout.fillWidth: true
                        Layout.leftMargin: Theme.padCard; Layout.rightMargin: Theme.padCard; Layout.bottomMargin: Theme.s3
                        spacing: Theme.s2
                        SButton { text: qsTr("Release notes"); glyph: "info"; onClicked: Data.releaseNotes() }
                        SButton { text: qsTr("SKradle on GitHub"); glyph: "globe"; onClicked: Qt.openUrlExternally("https://github.com/SKonstruct/SKradle") }
                        SButton { text: qsTr("Buy me a coffee"); glyph: "coffee"; onClicked: Qt.openUrlExternally("https://buymeacoffee.com/sirdankk") }
                        Item { Layout.fillWidth: true }
                    }
                }
            }
        }
    }

    FileDialog {
        id: javaDialog
        title: qsTr("Choose a Java runtime")
        onAccepted: Settings.javaPath = AppEnv.localPath(selectedFile)
    }
    FolderDialog {
        id: folderDialog
        title: qsTr("Choose the Spiral Knights folder")
        onAccepted: GameModel.addCustomFolder(selectedFolder)
    }
    ConfirmDialog {
        id: resetDialog
        title: qsTr("Reset game files?")
        message: qsTr("This removes every applied mod and deletes the game's files in %1, so the game's own updater downloads them all again on the next launch. The download can be large. Your mods folder and your settings are not touched.").arg(GameModel.selectedPath)
        confirmText: qsTr("Reset game files")
        danger: true
        onConfirmed: GameTools.resetGameFiles(GameModel.selectedPath)
    }
}
