pragma Singleton
import QtQuick
import com.skradle.app

// Placeholder content for pages that do not have their real data yet (the servers and mods agents replace these). Each block names what replaces it.
// The strings are marked with QT_TRANSLATE_NOOP so they are in the catalogue; the pages translate them with
// qsTranslate("Data", ...).
QtObject {
    signal toast(string text)
    signal navigate(int page)

    // TODO(serverhost): replace with the server list (core servers module) and the hosted server.
    property ListModel servers: ListModel {
        ListElement { name: "Official servers"; address: "game.example.invalid:47011"; region: "Global"; status: "online"; players: 1284; capacity: 2000; ping: 38; isDefault: true }
        ListElement { name: "Haven community"; address: "haven.example.invalid:47011"; region: "Europe"; status: "online"; players: 96; capacity: 150; ping: 74; isDefault: false }
        ListElement { name: "Test realm"; address: "test.example.invalid:47011"; region: "North America"; status: "busy"; players: 148; capacity: 150; ping: 188; isDefault: false }
        ListElement { name: "Local server"; address: "127.0.0.1:47011"; region: "This computer"; status: "offline"; players: 0; capacity: 0; ping: 0; isDefault: false }
    }
}
