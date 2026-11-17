import QtQuick
import QtQuick.VectorImage

// The shared SK glyph (assets/brand/logo.svg), rendered as vector at any size. 798 x 1105 source units.
VectorImage {
    property real glyphHeight: 100
    source: "qrc:/qt/qml/SKradleUi/brand/logo.svg"
    fillMode: VectorImage.PreserveAspectFit
    preferredRendererType: VectorImage.CurveRenderer
    height: glyphHeight
    width: glyphHeight * 798 / 1105
    Accessible.ignored: true
}
