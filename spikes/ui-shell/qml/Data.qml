pragma Singleton
import QtQuick

// Fake data. Everything here is placeholder content.
QtObject {
    signal toast(string text)
    property bool gameFound: true

    readonly property var news: [
        { tag: "Launcher", title: "Faster downloads", body: "Game files now download in parallel and resume where they stopped.", date: "2 days ago", hue: 0 },
        { tag: "Community", title: "Server list refresh", body: "Community servers report players and ping live, with a default you can set once.", date: "5 days ago", hue: 1 },
        { tag: "Tips", title: "Smoother on laptops", body: "Lower the render scale and cap the frame rate to keep temperatures down on battery.", date: "1 week ago", hue: 2 }
    ]

    property ListModel servers: ListModel {
        ListElement { name: "Official servers"; address: "game.example.invalid:47011"; region: "Global"; status: "online"; players: 1284; capacity: 2000; ping: 38; isDefault: true }
        ListElement { name: "Haven community"; address: "haven.example.invalid:47011"; region: "Europe"; status: "online"; players: 96; capacity: 150; ping: 74; isDefault: false }
        ListElement { name: "Test realm"; address: "test.example.invalid:47011"; region: "North America"; status: "busy"; players: 148; capacity: 150; ping: 188; isDefault: false }
        ListElement { name: "Local server"; address: "127.0.0.1:47011"; region: "This computer"; status: "offline"; players: 0; capacity: 0; ping: 0; isDefault: false }
    }

    readonly property var mods: [
        { id: 0, name: "Sharp HUD", tag: "Interface", desc: "Draws the interface at full resolution on high-DPI displays.", ver: "1.4", on: true },
        { id: 1, name: "Damage numbers", tag: "Interface", desc: "Larger, clearer hit numbers with optional colour coding.", ver: "0.9", on: true },
        { id: 2, name: "Loadout presets", tag: "Gameplay", desc: "Save and swap complete gear sets with one click.", ver: "2.1", on: true },
        { id: 3, name: "Chat tweaks", tag: "Social", desc: "Timestamps, per-channel colours and a quieter system log.", ver: "1.0", on: true },
        { id: 4, name: "Low-power mode", tag: "Performance", desc: "Caps the frame rate and trims effects while on battery.", ver: "0.6", on: false },
        { id: 5, name: "Colour-blind palettes", tag: "Accessibility", desc: "Alternative palettes for enemy telegraphs and item rarity.", ver: "1.2", on: false },
        { id: 6, name: "Item sorter", tag: "Inventory", desc: "Sort and filter the inventory by type, tier and value.", ver: "1.1", on: false },
        { id: 7, name: "Screenshot helper", tag: "Tools", desc: "Hides the interface and saves a clean capture to a folder.", ver: "0.8", on: true },
        { id: 8, name: "Controller remap", tag: "Input", desc: "Rebind gamepad buttons and adjust stick dead zones.", ver: "0.5", on: false }
    ]
}
