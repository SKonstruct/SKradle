pragma Singleton
import QtQuick
import com.skradle.app

// Placeholder content for features the core does not have yet. Each block names what replaces it.
// The strings are marked with QT_TRANSLATE_NOOP so they are in the catalogue; the pages translate them with
// qsTranslate("Data", ...).
QtObject {
    signal toast(string text)
    signal navigate(int page)

    // TODO(mods): replace with the mods engine (updates available for installed mods).
    property var modUpdates: [
        { name: QT_TRANSLATE_NOOP("Data", "Sharp HUD"), from: "1.4", to: "1.5" },
        { name: QT_TRANSLATE_NOOP("Data", "Chat tweaks"), from: "1.0", to: "1.1" }
    ]
    // TODO(graphics): replace with presets applied through the graphics agent settings.
    readonly property var presets: [
        { name: QT_TRANSLATE_NOOP("Data", "Battery saver"), detail: QT_TRANSLATE_NOOP("Data", "Render scale 75%, 60 fps cap, effects trimmed") },
        { name: QT_TRANSLATE_NOOP("Data", "Balanced"), detail: QT_TRANSLATE_NOOP("Data", "Render scale 100%, 120 fps cap, sharpening 40") },
        { name: QT_TRANSLATE_NOOP("Data", "Sharp"), detail: QT_TRANSLATE_NOOP("Data", "Render scale 150%, uncapped, sharpening 60") }
    ]
    // TODO(backend): replace with the launcher news feed.
    readonly property var news: [
        { tag: QT_TRANSLATE_NOOP("Data", "Launcher"), title: QT_TRANSLATE_NOOP("Data", "Faster downloads"), body: QT_TRANSLATE_NOOP("Data", "Game files now download in parallel and resume where they stopped."), date: QT_TRANSLATE_NOOP("Data", "2 days ago"), hue: 0 },
        { tag: QT_TRANSLATE_NOOP("Data", "Community"), title: QT_TRANSLATE_NOOP("Data", "Server list refresh"), body: QT_TRANSLATE_NOOP("Data", "Community servers report players and ping live, with a default you can set once."), date: QT_TRANSLATE_NOOP("Data", "5 days ago"), hue: 1 },
        { tag: QT_TRANSLATE_NOOP("Data", "Tips"), title: QT_TRANSLATE_NOOP("Data", "Smoother on laptops"), body: QT_TRANSLATE_NOOP("Data", "Lower the render scale and cap the frame rate to keep temperatures down on battery."), date: QT_TRANSLATE_NOOP("Data", "1 week ago"), hue: 2 }
    ]

    // TODO(serverhost): replace with the server list (core servers module) and the hosted server.
    property ListModel servers: ListModel {
        ListElement { name: "Official servers"; address: "game.example.invalid:47011"; region: "Global"; status: "online"; players: 1284; capacity: 2000; ping: 38; isDefault: true }
        ListElement { name: "Haven community"; address: "haven.example.invalid:47011"; region: "Europe"; status: "online"; players: 96; capacity: 150; ping: 74; isDefault: false }
        ListElement { name: "Test realm"; address: "test.example.invalid:47011"; region: "North America"; status: "busy"; players: 148; capacity: 150; ping: 188; isDefault: false }
        ListElement { name: "Local server"; address: "127.0.0.1:47011"; region: "This computer"; status: "offline"; players: 0; capacity: 0; ping: 0; isDefault: false }
    }
}
