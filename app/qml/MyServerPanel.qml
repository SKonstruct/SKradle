import QtQuick
import com.skradle.app
import QtQuick.Layouts

// The hosted server's own web interface. A web view where the platform has one; otherwise a button that opens the
// system browser. The choice is made at run time: WebPane.qml is loaded by URL, so a missing QtWebView module or
// backend can only make this Loader fail. The panel only has something to show while the server runs.
Item {
    id: root
    signal back()

    // "stopped" (not running), "loading", "embedded" or "browser": what is shown now (the self-test reads it).
    readonly property bool running: ServerHost.state === "running"
    readonly property string mode: !running ? (ServerHost.state === "starting" ? "loading" : "stopped")
        : !AppEnv.webViewAvailable ? "browser"
        : loader.status === Loader.Ready && !failed ? "embedded"
        : loader.status === Loader.Error || failed ? "browser" : "loading"
    property bool failed: false
    // "http://127.0.0.1:8080" of the page being shown (the script refuses any other origin).
    readonly property string origin: { const m = /^(https?:\/\/[^\/]+)\//.exec(String(ServerHost.url)); return m ? m[1] : "" }
    ClipboardHelper { id: clip }
    readonly property int pageProgress: loader.item ? loader.item.loadProgress : -1
    // The loaded page's title (the self-test checks that the page really loaded and ran its script).
    readonly property string pageTitle: loader.item ? loader.item.pageTitle : ""
    readonly property bool shown: visible && Window.window && Window.window.visible
    // A failure is not forever: a new address, or coming back to the panel, tries again.
    onShownChanged: if (shown) failed = false
    Connections { target: ServerHost; function onUrlChanged() { root.failed = false } }

    ColumnLayout {
        anchors.fill: parent
        spacing: Theme.s4

        PageHeader {
            Layout.fillWidth: true
            title: qsTr("Server web interface")
            subtitle: root.running ? qsTr("The server's own page for accounts, items and its console.")
                    : qsTr("The server isn't running, so there is nothing to show.")
            SButton { text: qsTr("Back to servers"); variant: "ghost"; onClicked: root.back(); Accessible.name: text }
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
                    id: loader
                    anchors.fill: parent
                    anchors.margins: 2
                    // A failed native view stays on top of everything below it, so it must not stay mounted.
                    active: root.shown && root.running && AppEnv.webViewAvailable && !root.failed
                    source: "qrc:/qt/qml/com/skradle/app/web/WebPane.qml"
                    onLoaded: item.loadFailed.connect((why) => { console.warn("web view: " + why); root.failed = true })
                    // A Binding, not a one-off assignment: the view follows the server to another address.
                    Binding { target: loader.item; property: "address"; value: ServerHost.url; when: loader.status === Loader.Ready }
                    // Sign in automatically: the script only runs on the server's own origin and holds the password.
                    Binding {
                        target: loader.item; property: "signInScript"
                        value: root.origin.length > 0 ? ServerHost.signInScript(root.origin) : ""
                        when: loader.status === Loader.Ready
                    }
                }
            }

            // Fallback: no web view on this system.
            ColumnLayout {
                anchors.centerIn: parent
                width: Math.min(parent.width - 2 * Theme.s8, 440)
                spacing: Theme.s3
                visible: root.mode === "browser"
                Txt {
                    Layout.fillWidth: true
                    horizontalAlignment: Text.AlignHCenter
                    text: root.failed ? qsTr("The built-in web view could not show the server's interface, so it opens in your browser.")
                                        : qsTr("This computer has no built-in web view, so the server's interface opens in your browser.")
                    color: Theme.textSecondary
                }
                SButton {
                    Layout.alignment: Qt.AlignHCenter
                    text: qsTr("Copy password"); glyph: "copy"
                    visible: ServerHost.status.indexOf('"adminUser":"') >= 0 && ServerHost.status.indexOf('"adminUser":""') < 0
                    onClicked: { clip.copy(ServerHost.adminPassword()); Data.toast(qsTr("Password copied")) }
                    Accessible.name: qsTr("Copy the web interface password. The user name is skradle.")
                }
                SButton {
                    Layout.alignment: Qt.AlignHCenter
                    highlighted: true
                    text: qsTr("Open in browser")
                    // Only a web address can be handed to the browser.
                    enabled: /^https?:\/\//.test(String(ServerHost.url))
                    onClicked: Qt.openUrlExternally(ServerHost.url)
                    Accessible.name: text
                }
            }
            Txt {
                anchors.centerIn: parent
                width: Math.min(parent.width - 2 * Theme.s8, 440)
                horizontalAlignment: Text.AlignHCenter
                visible: !root.running
                text: ServerHost.state === "starting" ? qsTr("Starting the server\u2026") : qsTr("The server isn't running. Start it from the server controls, then open its web interface.")
                color: Theme.textSecondary
            }
        }
    }
}
