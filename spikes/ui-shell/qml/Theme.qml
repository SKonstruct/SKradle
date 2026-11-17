pragma Singleton
import QtQuick

// Design tokens. Brand pink is used only on things we draw ourselves; native controls keep the system accent.
QtObject {
    id: t

    property SystemPalette sys: SystemPalette {}
    readonly property bool dark: AppEnv.scheme === "dark" ? true
        : AppEnv.scheme === "light" ? false
        : Qt.styleHints.colorScheme === Qt.ColorScheme.Dark ? true
        : Qt.styleHints.colorScheme === Qt.ColorScheme.Light ? false
        : sys.window.hslLightness < 0.5

    // brand
    readonly property color accent: "#AB4A81"
    // Hover/pressed keep white text at >= 4.5:1 (AA): #B04F86 = 4.9:1 on white text, #AB4A81 = 5.2:1
    readonly property color accentHover: dark ? "#B04F86" : "#9A3F72"
    readonly property color accentPressed: dark ? "#963F6E" : "#873562"
    // pink used as text/icon/focus on surfaces: lighter on dark (6.8:1 on #1E1E1E), brand pink on light (5.2:1 on white)
    readonly property color accentFg: dark ? "#E08BBB" : "#AB4A81"
    readonly property color accentText: "#FFFFFF"
    readonly property color accentTint: dark ? Qt.rgba(0.671, 0.290, 0.506, 0.22) : Qt.rgba(0.671, 0.290, 0.506, 0.13)
    readonly property color accentFocus: dark ? "#E08BBB" : "#AB4A81"
    readonly property color mint: "#C0DEDC"
    readonly property color mintOutline: "#75A09C"

    // surfaces
    readonly property color bg: dark ? "#141414" : "#F4F4F2"
    readonly property color sidebar: dark ? "#0F0F0F" : "#EAEAE7"
    readonly property color card: dark ? "#1E1E1E" : "#FFFFFF"
    readonly property color cardHover: dark ? "#242424" : "#FCFCFB"
    readonly property color raised: dark ? "#2B2B2B" : "#ECECE9"
    readonly property color track: dark ? "#2E2E2E" : "#E3E3E0"
    readonly property color sliderRail: dark ? "#707070" : "#929292"      // >= 3:1 against card
    readonly property color switchOff: dark ? "#6B6B6B" : "#8E8E8E"
    readonly property color field: dark ? "#151515" : "#F7F7F5"
    readonly property color fieldBorder: dark ? Qt.rgba(1, 1, 1, 0.22) : Qt.rgba(0, 0, 0, 0.30)   // >= 3:1 vs card
    readonly property color knob: "#FFFFFF"
    readonly property color scrollThumb: dark ? Qt.rgba(1, 1, 1, 0.38) : Qt.rgba(0, 0, 0, 0.38)
    readonly property color border: dark ? Qt.rgba(1, 1, 1, 0.08) : Qt.rgba(0, 0, 0, 0.09)
    readonly property color borderStrong: dark ? Qt.rgba(1, 1, 1, 0.18) : Qt.rgba(0, 0, 0, 0.22)
    readonly property color hoverTint: dark ? Qt.rgba(1, 1, 1, 0.06) : Qt.rgba(0, 0, 0, 0.055)
    readonly property color pressTint: dark ? Qt.rgba(1, 1, 1, 0.11) : Qt.rgba(0, 0, 0, 0.10)
    readonly property color topLight: dark ? Qt.rgba(1, 1, 1, 0.05) : Qt.rgba(1, 1, 1, 0.9)
    readonly property color shadowNear: dark ? Qt.rgba(0, 0, 0, 0.35) : Qt.rgba(20 / 255, 20 / 255, 30 / 255, 0.07)
    readonly property color shadowFar: dark ? Qt.rgba(0, 0, 0, 0.18) : Qt.rgba(20 / 255, 20 / 255, 30 / 255, 0.045)
    readonly property color scrim: dark ? Qt.rgba(0, 0, 0, 0.62) : Qt.rgba(0.08, 0.08, 0.1, 0.36)

    // text
    readonly property color text: dark ? "#F2F2F2" : "#1A1A1A"
    readonly property color textSecondary: dark ? "#A6A6A6" : "#5C5C5C"
    readonly property color textTertiary: dark ? "#8A8A8A" : "#737373"

    // status
    readonly property color amber: dark ? "#F2BF3A" : "#B27E00"
    readonly property color amberText: dark ? "#F2BF3A" : "#7A5600"
    readonly property color amberTint: dark ? Qt.rgba(0.949, 0.749, 0.227, 0.12) : Qt.rgba(0.949, 0.749, 0.227, 0.24)
    readonly property color ok: dark ? "#43CDCA" : "#12807C"
    readonly property color danger: dark ? "#FF7B72" : "#C23B33"

    // radii and spacing (4 px grid)
    readonly property int rSm: 6
    readonly property int rMd: 10
    readonly property int rLg: 14
    readonly property int rXl: 18
    readonly property int s1: 4
    readonly property int s2: 8
    readonly property int s3: 12
    readonly property int s4: 16
    readonly property int s5: 20
    readonly property int s6: 24
    readonly property int s8: 32
    readonly property int s10: 40
    readonly property int pageMax: 920

    // type scale, pixel sizes (macOS native body is 13, Windows/Linux 14)
    readonly property int base: Qt.platform.os === "osx" ? 13 : 14
    readonly property int fsMicro: base - 2
    readonly property int fsCaption: base - 1
    readonly property int fsBody: base
    readonly property int fsHeading: base + 3
    readonly property int fsTitle: base + 11
    readonly property int fsDisplay: base + 19
}
