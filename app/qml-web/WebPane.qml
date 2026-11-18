import QtQuick
import QtWebView

// The embedded web view. This file is NOT part of the QML module (it is a plain resource, parsed at run time), so
// a Qt without the QtWebView add-on makes only this file fail to load: MyServerPanel.qml sees the Loader error and
// shows the "open in browser" fallback. It must not import anything of the app's module.
Item {
    id: root
    property url address
    // The title the page itself reports, asked of the page's own JavaScript (None until the first answer).
    property string pageTitle: ""
    Timer {
        interval: 300; repeat: true; running: view.loadProgress === 100 && root.pageTitle === ""
        onTriggered: view.runJavaScript("document.title", (result) => {  if (result) root.pageTitle = String(result).replace(/^"|"$/g, "") })
    }
    readonly property int loadProgress: view.loadProgress
    signal loadFailed(string reason)

    WebView {
        id: view
        anchors.fill: parent
        url: root.address
        onLoadingChanged: (request) => {
            // The view starts with an empty address before the owner sets one; that is not a failure.
            if (request.status === WebView.LoadFailedStatus && request.url.toString() !== "") root.loadFailed(request.errorString)
        }
    }
}
