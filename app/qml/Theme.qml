pragma Singleton
import QtQuick
import com.skradle.app

// Design tokens (see docs/design-system.md). Brand pink is used only on things we draw ourselves.
//
// Contract for every page (do not rename without migrating all users):
//   spacing   s0 s1 s2 s3 s4 s5 s6 s8 s10 s12 s16 (4 px grid) and gapInline/gapStack/gapCard/gapSection/padCard/padCompact/padDialog/rowV
//   radii     rXs 4 (tags, keycaps, bars)  rSm 8 (buttons, fields, chips)  rMd 12 (cards, banners, menus)
//             rLg 16 (dialogs, hero)  rPill (switch, dot, count badge only)   rXl is an alias of rLg
//   type      fsDisplay fsTitle fsHeading fsBody fsCaption fsMicro fsMono
//   colour    text* surfaces, accent*, ok/warn/danger (+Tint), state layers stateHover/statePress/stateSelected
// A hex colour, a pixel radius or an off-grid spacing literal in a component is a bug: add a token here instead.
QtObject {
    id: t

    property SystemPalette sys: SystemPalette {}
    readonly property bool dark: AppEnv.scheme === "dark" ? true
        : AppEnv.scheme === "light" ? false
        : Settings.theme === "dark" ? true
        : Settings.theme === "light" ? false
        : Qt.styleHints.colorScheme === Qt.ColorScheme.Dark ? true
        : Qt.styleHints.colorScheme === Qt.ColorScheme.Light ? false
        : sys.window.hslLightness < 0.5

    // ---- brand ------------------------------------------------------------------------------------------------
    readonly property color accent: "#AB4A81"
    // Hover/pressed keep white text at >= 4.5:1 (AA): #B04F86 = 4.9:1 on white text, #AB4A81 = 5.2:1
    readonly property color accentHover: dark ? "#B04F86" : "#9A3F72"
    readonly property color accentPressed: dark ? "#963F6E" : "#873562"
    // pink used as text/icon/focus on surfaces: lighter on dark (6.8:1 on #1E1E1E), brand pink on light (5.2:1 on white)
    readonly property color accentFg: dark ? "#E08BBB" : "#AB4A81"
    readonly property color accentText: "#FFFFFF"
    readonly property color accentTint: dark ? Qt.rgba(0.671, 0.290, 0.506, 0.22) : Qt.rgba(0.671, 0.290, 0.506, 0.13)
    readonly property color accentFocus: accentFg
    readonly property color mint: "#C0DEDC"
    readonly property color mintOutline: "#75A09C"
    // Buy Me a Coffee yellow (the donate button only); `ink` text on it is 13:1
    readonly property color coffee: "#FFDD00"
    readonly property color coffeeHover: "#FFE54D"
    readonly property color coffeePressed: "#F0CE00"

    // ---- surfaces ---------------------------------------------------------------------------------------------
    // Dark depth comes from surface steps and the hairline; light adds one small neutral shadow (Elevation.qml).
    readonly property color bg: dark ? "#141414" : "#F4F4F2"
    readonly property color sidebar: dark ? "#0F0F0F" : "#EAEAE7"
    readonly property color card: dark ? "#1E1E1E" : "#FFFFFF"
    readonly property color cardHover: dark ? "#242424" : "#FCFCFB"
    readonly property color raised: dark ? "#2B2B2B" : "#ECECE9"
    readonly property color overlay: dark ? "#262626" : "#FFFFFF"        // menus, popovers, dialogs
    readonly property color track: dark ? "#2E2E2E" : "#E3E3E0"
    readonly property color sliderRail: dark ? "#707070" : "#929292"      // >= 3:1 against card
    readonly property color switchOff: dark ? "#808080" : "#858585"       // >= 3:1 on card and page
    readonly property color field: dark ? "#151515" : "#F7F7F5"
    readonly property color fieldBorder: dark ? Qt.rgba(1, 1, 1, 0.42) : Qt.rgba(0, 0, 0, 0.50)   // >= 3:1 vs card
    readonly property color knob: "#FFFFFF"
    readonly property color scrollThumb: dark ? Qt.rgba(1, 1, 1, 0.38) : Qt.rgba(0, 0, 0, 0.38)
    readonly property color border: dark ? Qt.rgba(1, 1, 1, 0.08) : Qt.rgba(0, 0, 0, 0.09)
    readonly property color borderStrong: dark ? Qt.rgba(1, 1, 1, 0.18) : Qt.rgba(0, 0, 0, 0.22)
    readonly property color scrim: dark ? Qt.rgba(0, 0, 0, 0.62) : Qt.rgba(0.08, 0.08, 0.1, 0.36)
    // Neutral dark chip for tooltips and toasts (same in both schemes, text on it is 12:1+)
    readonly property color tipBg: dark ? "#2E2E2E" : "#262626"
    readonly property color tipText: "#F2F2F2"
    readonly property color tipBorder: dark ? Qt.rgba(1, 1, 1, 0.14) : Qt.rgba(0, 0, 0, 0.25)
    readonly property color ink: "#1A1A1A"                                // text on amber/yellow fills

    // ---- state layers (compose over any surface) ---------------------------------------------------------------
    readonly property color stateHover: dark ? Qt.rgba(1, 1, 1, 0.06) : Qt.rgba(0, 0, 0, 0.055)
    readonly property color statePress: dark ? Qt.rgba(1, 1, 1, 0.11) : Qt.rgba(0, 0, 0, 0.10)
    readonly property color stateSelected: accentTint
    readonly property real disabledOpacity: 0.45
    // old names, kept so existing pages keep working
    readonly property color hoverTint: stateHover
    readonly property color pressTint: statePress

    // ---- elevation shadows (Elevation.qml reads these) ----------------------------------------------------------
    readonly property color shadowNear: dark ? Qt.rgba(0, 0, 0, 0.5) : Qt.rgba(20 / 255, 20 / 255, 30 / 255, 0.10)
    readonly property color shadowFar: dark ? Qt.rgba(0, 0, 0, 0.3) : Qt.rgba(20 / 255, 20 / 255, 30 / 255, 0.06)
    readonly property color shadowCard: Qt.rgba(20 / 255, 20 / 255, 30 / 255, 0.06)   // e1, light only

    // ---- text -------------------------------------------------------------------------------------------------
    readonly property color text: dark ? "#F2F2F2" : "#1A1A1A"
    readonly property color textSecondary: dark ? "#A6A6A6" : "#5C5C5C"
    // placeholders and meta only (>= 4.5:1 on card, bg, sidebar and raised)
    readonly property color textTertiary: dark ? "#939393" : "#666666"

    // ---- status (text/icon colours carry meaning only together with an icon or a word) --------------------------
    readonly property color amber: dark ? "#F2BF3A" : "#B27E00"           // fills, dots
    readonly property color amberText: dark ? "#F2BF3A" : "#7A5600"       // text on card
    readonly property color amberTint: dark ? Qt.rgba(0.949, 0.749, 0.227, 0.12) : Qt.rgba(0.949, 0.749, 0.227, 0.24)
    readonly property color warn: dark ? "#F2BF3A" : "#9A6B00"            // icon (4.7:1 on card)
    readonly property color warnTint: amberTint
    readonly property color ok: dark ? "#43CDCA" : "#0F766E"
    readonly property color okTint: dark ? Qt.rgba(0.263, 0.804, 0.792, 0.16) : Qt.rgba(0.059, 0.463, 0.431, 0.12)
    readonly property color danger: dark ? "#FF7B72" : "#C23B33"
    readonly property color dangerTint: dark ? Qt.rgba(1, 0.482, 0.447, 0.16) : Qt.rgba(0.761, 0.231, 0.2, 0.12)
    readonly property color dangerFill: "#B02A37"                         // filled irreversible-action button, white text 6.5:1
    readonly property color dangerFillHover: "#9A2430"
    readonly property color dangerFillPressed: "#841E29"

    // ---- spacing (4 px grid; 2 px nudges inside icons and 1 px hairlines are the only exceptions) ---------------
    readonly property int s0: 0
    readonly property int s1: 4
    readonly property int s2: 8
    readonly property int s3: 12
    readonly property int s4: 16
    readonly property int s5: 20
    readonly property int s6: 24
    readonly property int s8: 32
    readonly property int s10: 40
    readonly property int s12: 48
    readonly property int s16: 64
    // semantic gaps
    readonly property int gapInline: s2      // icon to label, button to button
    readonly property int gapStack: s3       // rows inside a group
    readonly property int gapCard: s4        // cards in a grid
    readonly property int gapSection: s6     // sections of a page
    readonly property int padCard: s5
    readonly property int padCompact: s4
    readonly property int padDialog: s6
    readonly property int rowV: s3           // SettingRow vertical padding

    // ---- radii (inner = outer - padding, min 4) ---------------------------------------------------------------------
    readonly property int rXs: 4
    readonly property int rSm: 8
    readonly property int rMd: 12
    readonly property int rLg: 16
    readonly property int rXl: rLg           // old name
    readonly property int rPill: 999

    // ---- control heights ----------------------------------------------------------------------------------------
    readonly property int hSm: 28            // small button, icon button, chip, segmented
    readonly property int hMd: 36            // button, field, select
    readonly property int hLg: 44            // large button
    readonly property int hRow: 56           // list and setting rows
    readonly property int hNav: 40
    readonly property int hLaunch: 64

    // ---- layout ---------------------------------------------------------------------------------------------------
    readonly property int pageMax: 960
    readonly property int readMeasure: 560   // paragraph width: 60-75 characters

    // ---- type scale, pixel sizes (macOS native body is 13, Windows/Linux 14) ---------------------------------------
    readonly property int base: Qt.platform.os === "osx" ? 13 : 14
    readonly property int fsMicro: base - 2
    readonly property int fsCaption: base - 1
    readonly property int fsBody: base
    readonly property int fsMono: base
    readonly property int fsHeading: base + 3
    readonly property int fsTitle: base + 11
    readonly property int fsDisplay: base + 19
    readonly property string monoFamily: Qt.platform.os === "osx" ? "Menlo" : Qt.platform.os === "windows" ? "Consolas" : "monospace"
}
