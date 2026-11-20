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

    // TODO(mods): replace with the installed mods from the mods engine.
    readonly property var mods: [
        { id: 0, name: "Sharp HUD", tag: QT_TRANSLATE_NOOP("Data", "Interface"), desc: QT_TRANSLATE_NOOP("Data", "Draws the interface at full resolution on high-DPI displays."), ver: "1.4", on: true },
        { id: 1, name: "Damage numbers", tag: QT_TRANSLATE_NOOP("Data", "Interface"), desc: QT_TRANSLATE_NOOP("Data", "Larger, clearer hit numbers with optional colour coding."), ver: "0.9", on: true },
        { id: 2, name: "Loadout presets", tag: QT_TRANSLATE_NOOP("Data", "Gameplay"), desc: QT_TRANSLATE_NOOP("Data", "Save and swap complete gear sets with one click."), ver: "2.1", on: true },
        { id: 3, name: "Chat tweaks", tag: QT_TRANSLATE_NOOP("Data", "Social"), desc: QT_TRANSLATE_NOOP("Data", "Timestamps, per-channel colours and a quieter system log."), ver: "1.0", on: true },
        { id: 4, name: "Low-power mode", tag: QT_TRANSLATE_NOOP("Data", "Performance"), desc: QT_TRANSLATE_NOOP("Data", "Caps the frame rate and trims effects while on battery."), ver: "0.6", on: false },
        { id: 5, name: "Colour-blind palettes", tag: QT_TRANSLATE_NOOP("Data", "Accessibility"), desc: QT_TRANSLATE_NOOP("Data", "Alternative palettes for enemy telegraphs and item rarity."), ver: "1.2", on: false },
        { id: 6, name: "Item sorter", tag: QT_TRANSLATE_NOOP("Data", "Inventory"), desc: QT_TRANSLATE_NOOP("Data", "Sort and filter the inventory by type, tier and value."), ver: "1.1", on: false },
        { id: 7, name: "Screenshot helper", tag: QT_TRANSLATE_NOOP("Data", "Tools"), desc: QT_TRANSLATE_NOOP("Data", "Hides the interface and saves a clean capture to a folder."), ver: "0.8", on: true },
        { id: 8, name: "Controller remap", tag: QT_TRANSLATE_NOOP("Data", "Input"), desc: QT_TRANSLATE_NOOP("Data", "Rebind gamepad buttons and adjust stick dead zones."), ver: "0.5", on: false }
    ]
}
