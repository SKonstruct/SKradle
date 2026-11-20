pragma Singleton
import QtQuick
import com.skradle.app

// Signals the pages use to talk to each other. (The sample content that used to live here is gone: every page now
// shows real data or an honest empty state.)
QtObject {
    signal toast(string text)
    signal navigate(int page)
}
