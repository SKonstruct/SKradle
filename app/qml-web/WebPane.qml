import QtQuick
import QtWebView

// The embedded web view. This file is NOT part of the QML module (it is a plain resource, parsed at run time), so
// a Qt without the QtWebView add-on makes only this file fail to load: MyServerPanel.qml sees the Loader error and
// shows the "open in browser" fallback. It must not import anything of the app's module.
Item {
    id: root
    property url address
    property int loadStatus: view.loadProgress
    signal loadFailed(string reason)

    WebView {
        id: view
        anchors.fill: parent
        url: root.address
        onLoadingChanged: (request) => {
            if (request.status === WebView.LoadFailedStatus) root.loadFailed(request.errorString)
        }
    }
}
