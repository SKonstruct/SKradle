pragma Singleton
import QtQuick
import com.skradle.app

// Signals the pages use to talk to each other. (The sample content that used to live here is gone: every page now
// shows real data or an honest empty state.)
QtObject {
    // Data.toast(text) or Data.toast(text, kind): kind is "info" (default), "success" or "error".
    signal toastRequested(string text, string kind)
    function toast(text, kind) { toastRequested(text, kind || "info") }
    signal navigate(int page)
    // Go to a page and scroll its section into view (the Settings sections: 0 General, 1 Game, 2 Files, 3 Graphics ...).
    signal navigateSection(int page, int section)
    // Opens the release-notes viewer (Home and Settings ask; Main owns the dialog).
    signal releaseNotes()
}
