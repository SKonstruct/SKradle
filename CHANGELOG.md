# Changelog

All notable changes to SKradle. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions
follow [Semantic Versioning](https://semver.org/). Nothing is released yet, so everything is under Unreleased (the first release is described by Added alone) and the
number in `Cargo.toml` (`0.1.0`) is a placeholder. Entries say what a player or a tester would notice; the commit
history has the rest.

## [Unreleased]

### Added

- **Launcher app** (Qt Quick, one look on every OS) with five pages: **Home**, **Servers**, **Mods**, **Tools** and **Settings**;
  light and dark theme; reduced-motion setting; English with a translation catalogue and a pseudo-language for layout
  testing; the launch, mod and folder error sentences are translatable too.
- **Home**: the launch card (update, launching and running states); the three latest Spiral Knights news items; an
  estimate of the players online (the Steam counts of the Live and Preview servers, each multiplied by 1.4 for
  standalone players, with the breakdown shown; click it for the history: now, 24-hour and recorded peaks, the usual busiest
  hour and weekday, and charts of the last 24 hours, 7 days and 30 days, from the Knight Launcher service's recorded history, asked once when the dialog opens with nothing about you in the request, or recorded on this computer while SKradle is open when the service cannot be reached); the game's live events (the same event feed the mobile apps use) with a countdown and a large banner, the title
  opens the article; hidden when there are none or the feed is unreachable (banners are kept on disk); a donation button; and **Release notes**,
  this changelog bundled into the app, readable offline with each version collapsible. News and player counts come
  from Steam's public API, are kept on disk and shown as offline when Steam is unreachable.
- **Launching Spiral Knights** from a Steam or standalone install, found automatically; a custom folder can be added.
  A launch applies mods (only when something changed: a mod added, removed, edited, enabled or disabled, a file
  cleaning or purging switch, or a game update), writes the server choice and starts the game directly on a suitable
  Java, and Home says which of these it is doing; a Steam copy is
  signed in through Steam (SKradle starts Steam and waits for it if needed).
- **Settings**: the game install to use, Java, memory (RAM), garbage collector and explicit-GC behaviour (recommended
  values by default: 4096 MB, G1, explicit GC disabled), extra JVM arguments; file cleaning, file purging and *Reset game files*; the graphics enhancements (render scale, sharpening,
  sharp HUD, frame generation, frame cap, FPS counter) passed to the game as the skgfx agent, which needs Java 24 or
  newer; Discord Rich Presence (off by default); appearance,
  language, reduced motion, hiding the launcher while playing, update check, crash reports; version and links.
- **Mods**: the mods in your mods folder with an on/off switch each, a picture per mod (from `mod.json` or `mod.png`,
  a generic icon otherwise), tags found automatically, filters for enabled, disabled, mods with warnings and each tag,
  search, drag and drop (or browse) to add mods (only valid ones are copied in), select several mods and enable,
  disable or remove them with the keyboard, open the mods folder, and a one-button install and update of the
  maintained modpack from one compact button next to the other actions (hash-checked, only changed files downloaded,
  cancellable).
- **Servers**: your saved servers with live reachability and ping, choose which one the game uses, add and edit servers
  (name, address, port, public key, getdown URL, content URL), and the **Your server** strip above the list to install,
  start, stop, restart, back up, open the folder of and update a local Tortodrone server (after accepting its licence), with its web
  interface shown inside the window while it runs (system web view on macOS and Windows; the browser on Linux).
- **Updates and reports**: checks GitHub for a newer launcher (can be switched off); optional crash reports, scrubbed
  of personal details, with an off switch.
- **Logs**: SKradle writes a log (`skradle.log`, scrubbed of your user name and secrets) with buttons in Settings > About
  to open its folder or copy its path; GitHub issue forms ask for it.
- **Languages**: catalogues for Arabic, German, Spanish, Estonian, French, Italian, Japanese, Polish, Brazilian
  Portuguese, Russian and Simplified and Traditional Chinese, picked in Settings (text not yet translated shows English);
  right-to-left layouts mirror. See `docs/translating.md`.
- **Alt mode**: a second client next to the first, with little memory, from a button on Home or by holding Alt or Shift
  while pressing Launch.
- **Tray icon** (menu bar on macOS) with Open and Quit; with it the launcher disappears completely while the game runs.
- **Mod order and conflicts**: drag a mod (or press Alt with the arrow keys) to change the order; a mod lower in the list
  wins when two change the same file, and each mod says which others it overrides or is overridden by.
- **Server notice**: the Official server's notice and planned maintenance show on Home when there is one.
- A quiet credit, "Built with ♥ by SirDank", at the bottom of Home; the name opens the author's site.
- **Tools** page: the game's Crucible editor and, from an editors pack that SKradle downloads once and keeps at the
  newest build by itself (a weekly build of the open-source Clyde and spiralview tools; Java 25 or newer), the scene
  editor, Silverweave, Model Viewer, Particle Editor and Interface Tester; and two Spiral Knights websites (Auction
  House, Kozmas) that open inside SKradle and keep your sign-in. See `docs/editors.md` and `docs/web-tools.md`.
- Files SKradle writes for you to read (`settings.json`, `servers.json`, the saved feeds) are indented, one entry per
  line, so they can be edited by hand.
- Windows: Steam is found where the registry says it is, so installs on another drive work.
- **Test builds** from every CI run on the Actions tab (pushes to main, version tags and manual runs). Each download is
  one zip holding one folder with a
  `SHA256SUMS.txt` inside it: a `.dmg` for macOS (Apple silicon and Intel), the app folder for Windows (x64 and
  ARM64; run `skradle.exe`) and an AppImage for Linux x64. They are unsigned: on macOS run `xattr -cr SKradle.app`
  once, on Windows choose "More info" then "Run anyway".

### Known gaps

- Live events and banners come from a third-party feed over plain HTTP (the only plain-HTTP request SKradle makes,
  limited to that one address); the app works normally without it.
- Everything is untested against a real Tortodrone engine, a real game launch and Windows; the first real runs will
  find problems.
- Players on the Official servers are shown as Steam's count (the standalone client is not counted).
- No installer, no code signing, no automatic update of the launcher itself yet.
- Linux arm64 is not supported (the game ships no Linux arm64 runtime).
