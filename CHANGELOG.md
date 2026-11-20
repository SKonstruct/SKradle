# Changelog

All notable changes to SKradle. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions
follow [Semantic Versioning](https://semver.org/). Nothing is released yet, so everything is under Unreleased and the
number in `Cargo.toml` (`0.1.0`) is a placeholder. Entries say what a player or a tester would notice; the commit
history has the rest.

## [Unreleased]

### Added

- **Launcher app** (Qt Quick, one look on every OS) with four pages: **Home**, **Servers**, **Mods** and **Settings**;
  light and dark theme; reduced-motion setting; English with a translation catalogue and a pseudo-language for layout
  testing.
- **Home**: the launch card (update, launching and running states); the three latest Spiral Knights news items; an
  estimate of the players online (the Steam counts of the Live and Preview servers, each multiplied by 1.4 for
  standalone players, with the breakdown shown); the game's live events with a countdown and a Learn more button (from
  the project's backend; the card stays hidden until events are published); a donation button; and **Release notes**,
  this changelog bundled into the app, readable offline with each version collapsible. News and player counts come
  from Steam's public API, are kept on disk and shown as offline when Steam is unreachable.
- **Launching Spiral Knights** from a Steam or standalone install, found automatically; a custom folder can be added.
  A launch checks for an update first (through the game's own updater), then applies mods, writes the server choice
  and starts the game directly on a suitable Java.
- **Settings**: the game install to use, Java, memory (RAM), garbage collector and explicit-GC behaviour (recommended
  values by default: 4096 MB, G1, explicit GC disabled), extra JVM arguments; file cleaning, file purging and *Reset game files*; the graphics enhancements (render scale, sharpening,
  sharp HUD, frame generation, frame cap, FPS counter) passed to the game as the skgfx agent, which needs Java 24 or
  newer; Discord Rich Presence (off until the project's Discord application id is set); the game's editors; appearance,
  language, reduced motion, hiding the launcher while playing, update check, crash reports; version and links.
- **Mods**: the mods in your mods folder with an on/off switch each, a picture per mod (from `mod.json` or `mod.png`,
  a generic icon otherwise), tags found automatically, filters for enabled, disabled, mods with warnings and each tag,
  search, drag and drop (or browse) to add mods (only valid ones are copied in), select several mods and enable,
  disable or remove them with the keyboard, open the mods folder, and a one-button install and update of the
  maintained modpack (hash-checked, only changed files downloaded, cancellable).
- **Servers**: your saved servers with live reachability and ping, choose which one the game uses, add and edit servers
  (name, address, port, public key, getdown URL, content URL), and the **Your server** strip above the list to install,
  start, stop, restart, back up and update a local Tortodrone server (after accepting its licence), with its web
  interface shown inside the window while it runs (system web view on macOS and Windows; the browser on Linux).
- **Updates and reports**: checks GitHub for a newer launcher (can be switched off); optional crash reports, scrubbed
  of personal details, with an off switch.
- Windows: Steam is found where the registry says it is, so installs on another drive work.
- **Test builds** from the Actions tab (`Build` workflow): a `.dmg` for macOS (Apple silicon and Intel), a `.zip` for
  Windows (x64 and ARM64) and an AppImage for Linux x64. They are unsigned: on macOS run `xattr -cr SKradle.app` once,
  on Windows choose "More info" then "Run anyway".

### Fixed

- Launching a Steam copy of the game on the Official server now signs in through Steam automatically instead of
  showing the password screen; if Steam is not running, SKradle starts it and asks you to launch again.
- The app crashed on exit on macOS (Qt's destructors ran with the interface still alive); it now closes in order.
- The Launching button was hard to read in the light theme; it is now an opaque brand pink with white text.
- Mod downloads are streamed to disk and verified there instead of being held in memory.
- Stopping the local server during start-up ends the wait at once; a failed start leaves nothing running.
- Several smaller safety and correctness fixes found by the code review (names that Windows cannot store, links
  inside mod archives and backups, Java 8 version numbers, Discord presence replies, `getdown.txt` comments).

### Removed

- The early UI prototypes (`spikes/`); the app replaced them. They stay in the git history.

### Known gaps

- Everything is untested against a real Tortodrone engine, a real game launch and Windows; the first real runs will
  find problems.
- Players on the Official servers are shown as Steam's count (the standalone client is not counted).
- No installer, no code signing, no automatic update of the launcher itself yet.
- Linux arm64 is not supported (the game ships no Linux arm64 runtime).
