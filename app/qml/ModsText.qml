pragma Singleton
import QtQuick
import com.skradle.app

// Words for what the mods bridge reports (tag ids, container kinds, sizes), so every page says them the same way.
QtObject {
    function tagLabel(id) {
        switch (id) {
        case "modpack": return qsTr("Modpack")
        case "code": return qsTr("Code")
        case "textures": return qsTr("Textures")
        case "audio": return qsTr("Audio")
        case "text": return qsTr("Text")
        case "resources": return qsTr("Game files")
        case "broken": return qsTr("Problem")
        }
        return id
    }
    function sizeText(bytes) {
        if (bytes >= 1048576) return qsTr("%1 MB").arg((bytes / 1048576).toFixed(bytes >= 10485760 ? 0 : 1))
        if (bytes >= 1024) return qsTr("%1 KB").arg(Math.round(bytes / 1024))
        return qsTr("%1 B").arg(bytes)
    }
    // "ZIP · 1.2 MB", "Modpack of 12 mods · 84 MB"
    function kindText(item) {
        if (item.container === "modpack") return qsTr("Modpack of %n mod(s)", "", item.parts) + " · " + sizeText(item.size)
        return (item.container === "jar" ? qsTr("JAR") : qsTr("ZIP")) + " · " + sizeText(item.size)
    }
    readonly property string cmdKey: Qt.platform.os === "osx" ? qsTr("Cmd") : qsTr("Ctrl")
}
