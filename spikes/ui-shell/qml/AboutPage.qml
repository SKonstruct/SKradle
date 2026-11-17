import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

PageScaffold {
    id: page
    title: L.t("About")
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
                Txt { Layout.fillWidth: true; text: L.t("A fast, friendly launcher for Spiral Knights."); color: Theme.textSecondary }
            }
            Button { text: L.t("Check for updates"); Layout.alignment: Qt.AlignVCenter; onClicked: Data.toast(L.t("You are up to date")) }
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
                text: L.t("Unofficial fan project. Not affiliated with Grey Havens. Spiral Knights © Grey Havens, LLC.")
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
            Txt { Layout.margins: Theme.s5; Layout.bottomMargin: Theme.s1; text: L.t("This build"); kind: "heading"; Accessible.role: Accessible.Heading }
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
            InfoRow { k: L.t("Interface toolkit"); v: "Qt " + AppEnv.qtVersion + " (Qt Quick)" }
            Divider { Layout.leftMargin: Theme.s5; Layout.rightMargin: Theme.s5 }
            InfoRow { k: L.t("Control style"); v: AppEnv.styleName + (AppEnv.nativeStyle ? " (native)" : " (themed)") }
            Divider { Layout.leftMargin: Theme.s5; Layout.rightMargin: Theme.s5 }
            InfoRow { k: L.t("Appearance"); v: Theme.dark ? L.t("Dark") : L.t("Light") }
            Divider { Layout.leftMargin: Theme.s5; Layout.rightMargin: Theme.s5 }
            InfoRow { k: L.t("Reduced motion"); v: Motion.reduce ? L.t("On") : L.t("Off") }
            Item { Layout.preferredHeight: Theme.s2 }
        }
    }

    Card {
        Layout.fillWidth: true
        implicitHeight: dev.implicitHeight
        ColumnLayout {
            id: dev
            width: parent.width
            spacing: 0
            Txt { Layout.margins: Theme.s5; Layout.bottomMargin: Theme.s1; text: L.t("Demo controls"); kind: "heading"; Accessible.role: Accessible.Heading }
            SettingRow {
                label: L.t("Simulate first run"); hint: L.t("Shows the empty state when the game is not installed.")
                Item { Layout.fillWidth: true }
                Switch { checked: !Data.gameFound; onToggled: Data.gameFound = !checked; Accessible.name: L.t("Simulate first run") }
            }
            Item { Layout.preferredHeight: Theme.s1 }
        }
    }
}
