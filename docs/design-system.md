# SKradle design system

For whoever writes a page. Everything lives in `app/qml`; tokens in `Theme.qml`. The rules in "Never" are checked in review.

Direction: calm, exact, a little warm. Surfaces stay neutral; the world shows in the one accent (brand pink) and the mint of the logo.
The launch card is the one rich moment; everything else is quiet chrome. The live status dot is the only looping decoration.

## Tokens (`Theme`)

| Group | Names |
|---|---|
| Spacing (4 px grid) | `s0 s1 s2 s3 s4 s5 s6 s8 s10 s12 s16` = 0 4 8 12 16 20 24 32 40 48 64 |
| Semantic gaps | `gapInline 8` (icon to label, button to button), `gapStack 12`, `gapCard 16`, `gapSection 24`, `padCard 20`, `padCompact 16`, `padDialog 24`, `rowV 12` |
| Radii | `rXs 4` tags, bars; `rSm 8` buttons, fields, chips, menu items; `rMd 12` cards, banners, menus; `rLg 16` dialogs, hero; `rPill` switch, dot, count only. `rXl` is an old alias of `rLg` |
| Heights | `hSm 28`, `hMd 36`, `hLg 44`, `hRow 56`, `hNav 40`, `hLaunch 64` |
| Type | `fsDisplay 32/33`, `fsTitle 24/25`, `fsHeading 16/17`, `fsBody 13/14`, `fsCaption 12/13`, `fsMicro 11/12`, `fsMono` (macOS / elsewhere) |
| Layout | `pageMax 960`, `readMeasure 560` (paragraphs: 60-75 characters) |
| Surfaces | `bg sidebar card cardHover raised overlay track field` |
| Text | `text textSecondary textTertiary` (tertiary: placeholders and meta only) |
| Accent | `accent accentHover accentPressed accentFg accentText accentTint accentFocus` (`accentFg` for pink text and icons, `accent` for fills) |
| Status | `ok warn danger amber(Text)` with `okTint warnTint dangerTint`; filled danger button `dangerFill*` |
| State layers | `stateHover statePress stateSelected`, `disabledOpacity 0.45` (old names `hoverTint pressTint` still work) |
| Lines | `border borderStrong fieldBorder` (field border is 3:1 on the card) |
| Motion (`Motion`) | `instant 90 fast 130 base 200 slow 300 pageIn 260 pageOut 150`, `loop 1100`, `pulse 1900`, `toastInfo 4000`, `toastError 8000`. Durations are 0 under reduced motion, except `spin`, `loop`, `pulse` and the toast times |

A hex colour, a pixel radius or an off-grid spacing literal in a component is a bug. Allowed exceptions: 1 px hairlines and 2 px nudges inside icons.

Elevation: `Card { elevation }` 0 flat, 1 card (light: one faint line, dark: none), 2 menu and popover, 3 dialog (`Elevation.qml`). Dark depth comes from surface steps and the hairline.

## Components

| Component | API | Notes |
|---|---|---|
| `Txt` | `kind: display|title|heading|body|caption|micro`, `numeric`, `mono`, `paragraph` | The only text element. Line height is the font's natural one; `paragraph: true` opts wrapped copy in to looser leading. `numeric` = tabular figures (ping, sizes, counts, ports). `mono` for addresses and paths |
| `SButton` | `variant: primary|secondary|ghost|danger|danger-primary|coffee`, `size: sm|md|lg`, `glyph`, `busy`, `highlighted` (= primary) | One `primary` per region. `coffee` (Buy Me a Coffee yellow, dark text) is for the Home donate button only. `busy` shows a spinner in place of the glyph, keeps the label, ignores clicks. Icons all-or-none inside a row |
| `IconButton` | `glyph`, `tip` (required: tooltip and accessible name), `size: sm|md`, `iconSize`, `toggledOn` | 28 or 36 square |
| `Chip` | `text`, `badge`, `selected` | Filter toggle, 28 high, check when selected. For one choice among 2-5 use `Segmented` |
| `Segmented` | `model: [{id, label, badge}]`, `currentId`, `label`, signal `picked(id)` | Joined single choice, arrow keys move it |
| `Tag` | `text`, `kind: neutral|accent|warn` | Static label, not clickable |
| `Card` | `radius`, `fill`, `elevation`, `padding` (the margin to use inside; not applied), `raised`, `outlined`, `surface` | No gradient. Never nest a bordered card: use `Divider` |
| `Banner` | `kind: info|success|warn|danger`, `text`, `title`, `actionText`, `dismissible`, signals `action()` `dismissed()` | Inline in a page or card; icon plus text, never colour alone. Fills the layout width |
| `EmptyState` | `glyph`, `title`, `text`, `actionText`, `actionGlyph`, `linkText`, signals `action()` `link()` | One sentence why, one primary action |
| `ServerDialog` | `title`, `subtitle`, `size: form|reading` (480 / 560; an explicit `preferredWidth` wins), `showClose` (X top right, no buttons row needed), `buttons`, `openFrom(item)`, `focusFirst()` | The dialog base for every dialog. Cancel secondary plus one primary, right aligned |
| `ConfirmDialog` | `title`, `message`, `confirmText`, `danger`, signal `confirmed()` | Cancel has the focus; `danger` makes the confirm button filled red |
| Toast | `Data.toast(text, kind)` with kind `info` (default), `success`, `error` | 4 s (error 8 s), +1 s per 40 characters over 80; stays while hovered or the close button has focus; errors have a close button |
| `SettingRow` | `label`, `hint`, `controlWidth` (default 280) | One control column on every row; the switch edge lines up with the selects |
| `SettingsSection` | `title` | One card per section |
| `PageScaffold` | `title`, `subtitle`, `actions`, `scroller`, `gutter` | Centred column 960, gutters 32 (24 under a 1000 px window), empty title hides the heading |
| Fields | `STextField` (`label`, `invalid`, `glyph`), `SComboBox`, `SSpinBox`, `SSlider`, `SSwitch`, `SCheckBox` | 36 high, 8 radius. Give every field an accessible name; never the placeholder alone. Show errors as a sentence below the field, not just a red border |
| Menus | `SMenu`, `SMenuItem`, `SMenuSeparator`, `SItemDelegate` | Our own popups, elevation 2 |
| `StatusDot` | `color`, `pulse` | Live things only (game, server, Steam players), always paired with a word or the row's accessible name |

## Vertical centring

Centring never depends on font metrics (Windows fonts have very different ascent, descent and line gap). A label in a button, chip, tag,
field, tab or row is a `Txt` (or `Text`) given the full height of its parent with `verticalAlignment: Text.AlignVCenter`; never a
manual `y`, a `topPadding` nudge, or proportional `lineHeight` on a single line (Qt adds that leading above the first line and pushes the
text down). Only multi-line copy sets `paragraph: true`. Icons beside text use `anchors.verticalCenter` or a `Row` of full-height items.
Check a change by running the shots with another family (a temporary `QGuiApplication::setFont`, e.g. Courier New or Verdana).

## Layout

- Window minimum 900x600; sidebar 224 (72 collapsed). Two columns only when each is at least 320 px wide.
- Section spacing 24, card gap 16, card padding 20 (16 for rows and compact lists), list row 56 (mods 60), field 36, button 36.
- One primary action per region; page header actions right aligned, at most two.
- A horizontal group of controls is a `Flow` or collapses into an overflow menu: nothing clips at 900x600.
- Rows: leading tile or control, text block (two lines at most), trailing meta in a fixed right-aligned column, trailing action.
- Selection in a list is one 3 px accent bar on the left edge plus the `raised` fill, never border + fill + glyph + label together.

## Colour rules

Pink fill only on the one `primary` of a region; never a pink wash or gradient on a surface. Semantic colours always come with an icon or a word.
`textTertiary` is for placeholders and meta and still passes 4.5:1. Selection and state never rely on tint alone.

## Copy

Sentence case everywhere. Short strings have no final period; paragraphs keep theirs. Errors say what failed, why, and what to do; raw
library or engine text never is the message (log it, or put it under "Details"). Buttons are verb + object ("Install modpack", "Back up and restore").
An ellipsis on a button only when it opens a dialog or picker. Shortcut hints go through `ModsText.cmdKey`. Empty states give the next action.

## Never

1. A hex colour, pixel radius or off-grid spacing literal in a component.
2. More than one filled pink button in a region; a pink gradient or wash.
3. Selection shown by border + fill + glyph + label at once.
4. A control group that clips at 900x600.
5. Colour alone for state; `textTertiary` for text that must be read.
6. Raw engine or Rust error strings as the message.
7. A bordered card inside a card.
8. A looping animation except the live dot and real progress; animating the size or position of layout.
9. A placeholder as the only label or accessible name.
10. Chrome (drop zones, banners, hint sentences) pushing the main list below the fold at 1180x780.
11. Fixed platform shortcut text.
12. A pill-shaped control other than switch, tag, count or dot.

## Checking a page

Run `skradle --scheme dark|light --size 1180x780 --shots <dir> --prefix d` and again with `--size 900x600`; add your states to `shotNames`
and `setupShot` in `Main.qml`. `skradle --selftest` (also with `SKRADLE_REDUCE_MOTION=1`) fails on any QML warning.
