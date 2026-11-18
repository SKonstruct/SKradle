import QtQuick
import com.skradle.app
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Dialogs

PageScaffold {
    id: page
    title: qsTr("Graphics")
    subtitle: qsTr("Rendering options for the game. Changes apply the next time it starts.")
    readonly property bool gameRunning: LaunchModel.state === "running"
    function openCombo() { aaCombo.popup.open() }
    function closeCombo() { aaCombo.popup.close() }
    function focusSlider() { scale.forceActiveFocus(Qt.TabFocusReason) }

    // amber notice while the game runs
    Rectangle {
        Layout.fillWidth: true
        implicitHeight: notice.implicitHeight + 24
        radius: Theme.rMd
        color: Theme.amberTint
        border.width: 1
        border.color: Qt.rgba(Theme.amber.r, Theme.amber.g, Theme.amber.b, 0.45)
        visible: opacity > 0.01
        opacity: page.gameRunning ? 1 : 0
        Behavior on opacity { NumberAnimation { duration: Motion.base } }
        Accessible.role: Accessible.AlertMessage
        Accessible.name: notice.text
        RowLayout {
            id: notice0
            anchors.fill: parent
            anchors.margins: 12
            spacing: 12
            Icon { name: "warning"; size: 20; color: Theme.amber; Layout.alignment: Qt.AlignTop }
            Txt { id: notice; Layout.fillWidth: true; text: qsTr("The game is running. Close it to change rendering options."); color: Theme.amberText; font.weight: Font.Medium }
        }
    }

    // TODO(graphics): these values are saved but the game does not read them yet; the graphics agent will.
    // ---- Rendering (locked while the game runs) ----------------------------------------------------------
    Card {
        Layout.fillWidth: true
        implicitHeight: g1.implicitHeight
        ColumnLayout {
            id: g1
            width: parent.width
            spacing: 0
            ColumnLayout {
                id: g1body
                Layout.fillWidth: true
                spacing: 0
                enabled: !page.gameRunning
                opacity: page.gameRunning ? 0.5 : 1
                Behavior on opacity { NumberAnimation { duration: Motion.base } }
                Txt { Layout.margins: Theme.s5; Layout.bottomMargin: Theme.s1; text: qsTr("Rendering"); kind: "heading"; Accessible.role: Accessible.Heading }
                SettingRow {
                    label: qsTr("Render scale"); hint: qsTr("Lower values trade sharpness for speed.")
                    SSlider { id: scale; Layout.fillWidth: true; from: 50; to: 200; value: Settings.renderScale; onMoved: Settings.renderScale = Math.round(value); Accessible.name: qsTr("Render scale") }
                    Txt { Layout.preferredWidth: 46; horizontalAlignment: Text.AlignRight; text: Math.round(scale.value) + "%"; font.features: { "tnum": 1 }; wrapMode: Text.NoWrap }
                }
                Divider { Layout.leftMargin: Theme.s5; Layout.rightMargin: Theme.s5 }
                SettingRow {
                    label: qsTr("Sharpening"); hint: qsTr("Crisper edges after scaling.")
                    SSlider { id: sharp; Layout.fillWidth: true; from: 0; to: 100; value: Settings.sharpening; onMoved: Settings.sharpening = Math.round(value); Accessible.name: qsTr("Sharpening") }
                    Txt { Layout.preferredWidth: 46; horizontalAlignment: Text.AlignRight; text: Math.round(sharp.value); font.features: { "tnum": 1 }; wrapMode: Text.NoWrap }
                }
                Divider { Layout.leftMargin: Theme.s5; Layout.rightMargin: Theme.s5 }
                SettingRow {
                    label: qsTr("Anti-aliasing")
                    SComboBox { id: aaCombo; Layout.fillWidth: true; model: [qsTr("Off"), "FXAA", "MSAA 4x"]; currentIndex: Settings.antiAliasing; onActivated: (i) => Settings.antiAliasing = i; Accessible.name: qsTr("Anti-aliasing") }
                }
                Divider { Layout.leftMargin: Theme.s5; Layout.rightMargin: Theme.s5 }
                SettingRow {
                    label: qsTr("Sharp HUD"); hint: qsTr("Draw the interface at full resolution.")
                    Item { Layout.fillWidth: true }
                    SSwitch { checked: Settings.sharpHud; onToggled: Settings.sharpHud = checked; Accessible.name: qsTr("Sharp HUD") }
                }
                Divider { Layout.leftMargin: Theme.s5; Layout.rightMargin: Theme.s5 }
                SettingRow {
                    label: qsTr("FPS counter")
                    Item { Layout.fillWidth: true }
                    SSwitch { checked: Settings.showFps; onToggled: Settings.showFps = checked; Accessible.name: qsTr("FPS counter") }
                }
                Item { Layout.preferredHeight: Theme.s1 }
            }
        }
        LockedOverlay {
            anchors.fill: parent
            visible: page.gameRunning
            reason: qsTr("Close the game to change these options")
        }
    }

    // ---- Frame generation (never available in this demo: no capable GPU) -----------------------------------
    Card {
        Layout.fillWidth: true
        implicitHeight: g2.implicitHeight
        ColumnLayout {
            id: g2
            width: parent.width
            spacing: 0
            ColumnLayout {
                Layout.fillWidth: true
                spacing: 0
                enabled: false
                opacity: 0.5
                RowLayout {
                    Layout.margins: Theme.s5; Layout.bottomMargin: Theme.s1
                    spacing: Theme.s2
                    Txt { text: qsTr("Frame generation"); kind: "heading"; wrapMode: Text.NoWrap; Accessible.role: Accessible.Heading }
                    Rectangle {
                        implicitWidth: expTxt.implicitWidth + 14; implicitHeight: 20; radius: 10
                        color: Theme.amberTint
                        Txt { id: expTxt; anchors.centerIn: parent; text: qsTr("Experimental"); kind: "micro"; color: Theme.amberText; wrapMode: Text.NoWrap }
                    }
                    Item { Layout.fillWidth: true }
                }
                SettingRow {
                    label: qsTr("Mode")
                    SComboBox { Layout.fillWidth: true; model: [qsTr("Adaptive"), qsTr("Fixed multiplier")]; currentIndex: Settings.frameGenMode; onActivated: (i) => Settings.frameGenMode = i; Accessible.name: qsTr("Mode") }
                }
                Divider { Layout.leftMargin: Theme.s5; Layout.rightMargin: Theme.s5 }
                SettingRow {
                    label: qsTr("Target frame rate"); hint: qsTr("Frames per second to aim for.")
                    Item { Layout.fillWidth: true }
                    SSpinBox { from: 30; to: 360; value: Settings.targetFps; editable: true; onValueModified: Settings.targetFps = value; Accessible.name: qsTr("Target frame rate") }
                }
                Item { Layout.preferredHeight: Theme.s1 }
            }
        }
        LockedOverlay {
            anchors.fill: parent
            reason: qsTr("Needs a GPU with Vulkan 1.3. None was found.")
            icon: "lock"
        }
    }

    // ---- Runtime ------------------------------------------------------------------------------------------
    Card {
        Layout.fillWidth: true
        implicitHeight: g3.implicitHeight
        ColumnLayout {
            id: g3
            width: parent.width
            spacing: 0
            Txt { Layout.margins: Theme.s5; Layout.bottomMargin: Theme.s1; text: qsTr("Runtime"); kind: "heading"; Accessible.role: Accessible.Heading }
            SettingRow {
                label: qsTr("Java path"); hint: qsTr("Leave empty to use the game's own runtime, or a Java found on this computer.")
                controlWidth: 330
                STextField { Layout.fillWidth: true; text: Settings.javaPath; placeholderText: qsTr("Automatic"); onEditingFinished: Settings.javaPath = text.trim(); Accessible.name: qsTr("Java path") }
                SButton { text: qsTr("Browse…"); onClicked: javaDialog.open() }
            }
            Divider { Layout.leftMargin: Theme.s5; Layout.rightMargin: Theme.s5 }
            SettingRow {
                label: qsTr("Memory limit"); hint: qsTr("Megabytes available to the game.")
                Item { Layout.fillWidth: true }
                // Only a change by the player is saved: until then the game keeps the limit in its own extra.txt, shown here.
                SSpinBox {
                    from: 512; to: 16384; stepSize: 256; editable: true
                    value: Settings.memoryMb > 0 ? Settings.memoryMb : (GameModel.detectedMemoryMb > 0 ? GameModel.detectedMemoryMb : 2048)
                    onValueModified: Settings.memoryMb = value
                    Accessible.name: qsTr("Memory limit")
                }
            }
            Divider { Layout.leftMargin: Theme.s5; Layout.rightMargin: Theme.s5 }
            SettingRow {
                label: qsTr("Minimize launcher while playing"); hint: qsTr("It comes back when the game exits.")
                Item { Layout.fillWidth: true }
                SCheckBox { checked: Settings.minimizeWhilePlaying; onToggled: Settings.minimizeWhilePlaying = checked; Accessible.name: qsTr("Minimize launcher while playing") }
            }
            Item { Layout.preferredHeight: Theme.s1 }
        }
    }

    FileDialog {
        id: javaDialog
        title: qsTr("Choose a Java runtime")
        onAccepted: Settings.javaPath = AppEnv.localPath(selectedFile)
    }
}
