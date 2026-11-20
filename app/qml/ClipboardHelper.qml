import QtQuick

// Puts text on the system clipboard. The text lives in a hidden text edit that selects itself and copies: Qt Quick has
// no direct clipboard call. The text is cleared again at once, so a password does not stay in memory here.
Item {
    id: root
    visible: false
    function copy(text) {
        edit.text = text
        edit.selectAll()
        edit.copy()
        edit.text = ""
    }
    TextEdit { id: edit }
}
