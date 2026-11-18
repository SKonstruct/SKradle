import QtQuick
import QtWebView

// The embedded web view. This file is NOT part of the QML module (it is a plain resource, parsed at run time), so
// a Qt without the QtWebView add-on makes only this file fail to load: MyServerPanel.qml sees the Loader error and
// shows the "open in browser" fallback. It must not import anything of the app's module.
Item {
    id: root
    property url address
    // The title the page itself reports, asked of the page's own JavaScript (empty until the first answer). It is
    // asked a bounded number of times per navigation, so a page without a title is not polled forever.
    property string pageTitle: ""
    property int titleTries: 0
    function decodeTitle(result) {
        // Some backends answer with a JSON string ("\"Title\""): decode it once instead of stripping quotes.
        const text = String(result)
        if (text.length >= 2 && text[0] === '"' && text[text.length - 1] === '"') {
            try { return JSON.parse(text) } catch (e) { /* not JSON: use as is */ }
        }
        return text
    }
    function newPage() { pageTitle = ""; titleTries = 0 }
    onAddressChanged: newPage()
    Timer {
        interval: 300; repeat: true; running: view.loadProgress === 100 && root.pageTitle === "" && root.titleTries < 20
        onTriggered: {
            root.titleTries++
            view.runJavaScript("document.title", (result) => { if (root && result) root.pageTitle = root.decodeTitle(result) })
        }
    }
    readonly property int loadProgress: view.loadProgress
    signal loadFailed(string reason)

    WebView {
        id: view
        anchors.fill: parent
        url: root.address
        onLoadingChanged: (request) => {
            if (request.status === WebView.LoadStartedStatus) root.newPage()
            // The view starts with an empty address before the owner sets one; that is not a failure.
            if (request.status === WebView.LoadFailedStatus && request.url.toString() !== "") root.loadFailed(request.errorString)
        }
    }
}
