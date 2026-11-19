import QtQuick
import com.skradle.app
import QtQuick.Controls
import QtQuick.Layouts

PageScaffold {
    id: page
    title: qsTr("About")
    subtitle: ""

    Card {
        Layout.fillWidth: true
        implicitHeight: top.implicitHeight + 2 * Theme.s6
        radius: Theme.rXl
        RowLayout {
            id: top
            anchors.left: parent.left; anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            anchors.margins: Theme.s6
            spacing: Theme.s5
            Logo { size: 72; Layout.alignment: Qt.AlignVCenter }
            ColumnLayout {
                Layout.fillWidth: true
                spacing: 4
                Txt { Layout.fillWidth: true; text: "SKradle"; kind: "display"; Accessible.role: Accessible.Heading }
                Txt { Layout.fillWidth: true; text: qsTr("A fast, friendly launcher for Spiral Knights."); color: Theme.textSecondary }
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
                    kind: "caption"
                    visible: text !== ""
                    color: UpdateModel.state === "error" ? Theme.danger : Theme.textSecondary
                    text: UpdateModel.state === "uptodate" ? qsTr("You are up to date.")
                        : UpdateModel.state === "error" ? qsTr("Could not check: %1").arg(UpdateModel.message)
                        : ""
                    wrapMode: Text.Wrap
                    Layout.maximumWidth: 300
                    horizontalAlignment: Text.AlignRight
                }
            }
        }
    }

    // disclaimer
    Rectangle {
        Layout.fillWidth: true
        implicitHeight: disc.implicitHeight + 2 * Theme.s4
        radius: Theme.rLg
        color: Theme.hoverTint
        border.width: 1
        border.color: Theme.border
        RowLayout {
            anchors.fill: parent
            anchors.margins: Theme.s4
            spacing: Theme.s3
            Icon { name: "info"; size: 20; color: Theme.textSecondary; Layout.alignment: Qt.AlignTop }
            Txt {
                id: disc
                Layout.fillWidth: true
                text: qsTr("Unofficial fan project. Not affiliated with Grey Havens. Spiral Knights © Grey Havens, LLC.")
                Accessible.role: Accessible.StaticText
            }
        }
    }

    Card {
        Layout.fillWidth: true
        implicitHeight: info.implicitHeight
        ColumnLayout {
            id: info
            width: parent.width
            spacing: 0
            Txt { Layout.margins: Theme.s5; Layout.bottomMargin: Theme.s1; text: qsTr("This build"); kind: "heading"; Accessible.role: Accessible.Heading }
            component InfoRow: RowLayout {
                property string k
                property string v
                Layout.fillWidth: true
                Layout.leftMargin: Theme.s5; Layout.rightMargin: Theme.s5
                Layout.topMargin: 9; Layout.bottomMargin: 9
                spacing: Theme.s4
                Txt { Layout.preferredWidth: 150; text: k; color: Theme.textSecondary }
                Txt { Layout.fillWidth: true; text: v; font.weight: Font.Medium }
            }
            InfoRow { k: qsTr("Version"); v: AppEnv.version }
            Divider { Layout.leftMargin: Theme.s5; Layout.rightMargin: Theme.s5 }
            InfoRow { k: qsTr("Interface toolkit"); v: "Qt " + AppEnv.qtVersion + " (Qt Quick)" }
            Divider { Layout.leftMargin: Theme.s5; Layout.rightMargin: Theme.s5 }
            InfoRow { k: qsTr("Control style"); v: AppEnv.styleName + " (themed)" }
            Divider { Layout.leftMargin: Theme.s5; Layout.rightMargin: Theme.s5 }
            InfoRow { k: qsTr("Appearance"); v: Theme.dark ? qsTr("Dark") : qsTr("Light") }
            Divider { Layout.leftMargin: Theme.s5; Layout.rightMargin: Theme.s5 }
            InfoRow { k: qsTr("Reduced motion"); v: Motion.reduce ? qsTr("On") : qsTr("Off") }
            Item { Layout.preferredHeight: Theme.s2 }
        }
    }

    // ---- preferences -----------------------------------------------------------------------------------------
    Card {
        Layout.fillWidth: true
        implicitHeight: prefs.implicitHeight
        ColumnLayout {
            id: prefs
            width: parent.width
            spacing: 0
            Txt { Layout.margins: Theme.s5; Layout.bottomMargin: Theme.s1; text: qsTr("Preferences"); kind: "heading"; Accessible.role: Accessible.Heading }
            SettingRow {
                label: qsTr("Appearance")
                SComboBox {
                    id: themeBox
                    Layout.fillWidth: true
                    readonly property var values: ["system", "light", "dark"]
                    model: [qsTr("Follow the system"), qsTr("Light"), qsTr("Dark")]
                    // A retranslation hands the box a new model, which resets its index without touching a binding,
                    // so the index is set from the setting at start, on a new model and on a change.
                    function sync() { currentIndex = Math.max(0, values.indexOf(Settings.theme)) }
                    Component.onCompleted: sync()
                    onModelChanged: sync()
                    Connections { target: Settings; function onThemeChanged() { themeBox.sync() } }
                    onActivated: (i) => Settings.theme = values[i]
                    Accessible.name: qsTr("Appearance")
                }
            }
            Divider { Layout.leftMargin: Theme.s5; Layout.rightMargin: Theme.s5 }
            SettingRow {
                label: qsTr("Reduce motion"); hint: qsTr("Turns off sliding and fading animations.")
                Item { Layout.fillWidth: true }
                SSwitch { checked: Settings.reduceMotion; onToggled: Settings.reduceMotion = checked; Accessible.name: qsTr("Reduce motion") }
            }
            Divider { Layout.leftMargin: Theme.s5; Layout.rightMargin: Theme.s5 }
            SettingRow {
                label: qsTr("Language")
                SComboBox {
                    id: languageBox
                    Layout.fillWidth: true
                    // Each language is named in itself. Add a catalogue in i18n/ and a row here.
                    readonly property var values: ["system", "en"]
                    model: [qsTr("Follow the system"), "English"]
                    // Choosing a language retranslates the window, which gives the box a new model and resets its
                    // index to the first row: set it again from the setting.
                    function sync() { currentIndex = Math.max(0, values.indexOf(Settings.language)) }
                    Component.onCompleted: sync()
                    onModelChanged: sync()
                    Connections { target: Settings; function onLanguageChanged() { languageBox.sync() } }
                    onActivated: (i) => Settings.language = values[i]
                    Accessible.name: qsTr("Language")
                }
            }
            Divider { Layout.leftMargin: Theme.s5; Layout.rightMargin: Theme.s5 }
            SettingRow {
                label: qsTr("Send crash reports"); hint: qsTr("Error reports contain no names, folders or addresses.")
                Item { Layout.fillWidth: true }
                SSwitch { checked: Settings.crashReports; onToggled: Settings.crashReports = checked; Accessible.name: qsTr("Send crash reports") }
            }
            Divider { Layout.leftMargin: Theme.s5; Layout.rightMargin: Theme.s5 }
            SettingRow {
                label: qsTr("Check for updates automatically"); hint: qsTr("Looks for a newer SKradle a moment after it starts. You always choose when to install.")
                Item { Layout.fillWidth: true }
                SSwitch { checked: Settings.autoUpdateCheck; onToggled: Settings.autoUpdateCheck = checked; Accessible.name: qsTr("Check for updates automatically") }
            }
            Item { Layout.preferredHeight: Theme.s1 }
        }
    }
}
