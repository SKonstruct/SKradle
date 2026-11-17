import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

PageScaffold {
    id: page
    title: L.t("Graphics")
    subtitle: L.t("Rendering options for the game. Changes apply the next time it starts.")
    readonly property bool gameRunning: LaunchModel.state === "running"

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
            Txt { id: notice; Layout.fillWidth: true; text: L.t("The game is running. Close it to change rendering options."); color: Theme.amberText; font.weight: Font.Medium }
        }
    }

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
                Txt { Layout.margins: Theme.s5; Layout.bottomMargin: Theme.s1; text: L.t("Rendering"); kind: "heading"; Accessible.role: Accessible.Heading }
                SettingRow {
                    label: L.t("Render scale"); hint: L.t("Lower values trade sharpness for speed.")
                    Slider { id: scale; Layout.fillWidth: true; from: 50; to: 200; value: 100; Accessible.name: L.t("Render scale") }
                    Txt { Layout.preferredWidth: 46; horizontalAlignment: Text.AlignRight; text: Math.round(scale.value) + "%"; font.features: { "tnum": 1 }; wrapMode: Text.NoWrap }
                }
                Divider { Layout.leftMargin: Theme.s5; Layout.rightMargin: Theme.s5 }
                SettingRow {
                    label: L.t("Sharpening"); hint: L.t("Crisper edges after scaling.")
                    Slider { id: sharp; Layout.fillWidth: true; from: 0; to: 100; value: 40; Accessible.name: L.t("Sharpening") }
                    Txt { Layout.preferredWidth: 46; horizontalAlignment: Text.AlignRight; text: Math.round(sharp.value); font.features: { "tnum": 1 }; wrapMode: Text.NoWrap }
                }
                Divider { Layout.leftMargin: Theme.s5; Layout.rightMargin: Theme.s5 }
                SettingRow {
                    label: L.t("Anti-aliasing")
                    ComboBox { Layout.fillWidth: true; model: [L.t("Off"), "FXAA", "MSAA 4x"]; currentIndex: 1; Accessible.name: L.t("Anti-aliasing") }
                }
                Divider { Layout.leftMargin: Theme.s5; Layout.rightMargin: Theme.s5 }
                SettingRow {
                    label: L.t("Sharp HUD"); hint: L.t("Draw the interface at full resolution.")
                    Item { Layout.fillWidth: true }
                    Switch { checked: true; Accessible.name: L.t("Sharp HUD") }
                }
                Divider { Layout.leftMargin: Theme.s5; Layout.rightMargin: Theme.s5 }
                SettingRow {
                    label: L.t("FPS counter")
                    Item { Layout.fillWidth: true }
                    Switch { checked: false; Accessible.name: L.t("FPS counter") }
                }
                Item { Layout.preferredHeight: Theme.s1 }
            }
        }
        LockedOverlay {
            anchors.fill: parent
            visible: page.gameRunning
            reason: L.t("Close the game to change these options")
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
                    Txt { text: L.t("Frame generation"); kind: "heading"; wrapMode: Text.NoWrap; Accessible.role: Accessible.Heading }
                    Rectangle {
                        implicitWidth: expTxt.implicitWidth + 14; implicitHeight: 20; radius: 10
                        color: Theme.amberTint
                        Txt { id: expTxt; anchors.centerIn: parent; text: L.t("Experimental"); kind: "micro"; color: Theme.amberText; wrapMode: Text.NoWrap }
                    }
                    Item { Layout.fillWidth: true }
                }
                SettingRow {
                    label: L.t("Mode")
                    ComboBox { Layout.fillWidth: true; model: [L.t("Adaptive"), L.t("Fixed multiplier")]; Accessible.name: L.t("Mode") }
                }
                Divider { Layout.leftMargin: Theme.s5; Layout.rightMargin: Theme.s5 }
                SettingRow {
                    label: L.t("Target frame rate"); hint: L.t("Frames per second to aim for.")
                    Item { Layout.fillWidth: true }
                    SpinBox { from: 30; to: 360; value: 120; editable: true; Accessible.name: L.t("Target frame rate") }
                }
                Item { Layout.preferredHeight: Theme.s1 }
            }
        }
        LockedOverlay {
            anchors.fill: parent
            reason: L.t("Needs a GPU with Vulkan 1.3. None was found.")
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
            Txt { Layout.margins: Theme.s5; Layout.bottomMargin: Theme.s1; text: L.t("Runtime"); kind: "heading"; Accessible.role: Accessible.Heading }
            SettingRow {
                label: L.t("Java path"); hint: L.t("Leave empty to use the bundled runtime.")
                controlWidth: 330
                TextField { Layout.fillWidth: true; text: "/usr/bin/java"; Accessible.name: L.t("Java path") }
                Button { text: L.t("Browse…"); Accessible.name: text }
            }
            Divider { Layout.leftMargin: Theme.s5; Layout.rightMargin: Theme.s5 }
            SettingRow {
                label: L.t("Memory limit"); hint: L.t("Megabytes available to the game.")
                Item { Layout.fillWidth: true }
                SpinBox { from: 512; to: 16384; stepSize: 256; value: 2048; editable: true; Accessible.name: L.t("Memory limit") }
            }
            Item { Layout.preferredHeight: Theme.s1 }
        }
    }
}
