# Changelog

All notable changes to SKradle. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions
follow [Semantic Versioning](https://semver.org/). Nothing is released yet, so everything is under Unreleased and the
number in `Cargo.toml` (`0.1.0`) is a placeholder. Entries say what a player or a tester would notice; the commit
history has the rest.

## [Unreleased]

### Added

- **Launcher app** (Qt Quick, one look on every OS): Home, Servers, Mods, Graphics and About pages; light and dark
  theme; reduced-motion setting; English with a translation catalogue and a pseudo-language for layout testing.
- **Launching Spiral Knights** from a Steam or standalone install, found automatically; a custom folder can be added.
  A launch checks for an update first (through the game's own updater), then applies mods, writes the server choice
  and starts the game directly on a suitable Java.
- **Graphics enhancements** (render scale, sharpening, anti-aliasing, sharp HUD, frame generation, frame cap, FPS
  counter) passed to the game as the skgfx agent, which needs Java 24 or newer.
- **Mods**: reads `.zip`, `.jar` and `.modpack` mods, applies and removes them against the game's own file checks, all
  or nothing, and can sync a maintained modpack with hash-checked downloads.
- **Community servers**: store and edit server entries and point the game at one without touching the game's files by
  hand.
- **My server**: download, configure, start, stop and back up a local Tortodrone server after accepting its licence,
  with its web interface shown inside the window (system web view on macOS and Windows; the browser on Linux).
- **Updates and reports**: checks GitHub for a newer launcher (can be switched off); optional crash reports, scrubbed
  of personal details, with an off switch.
- **Test builds** from the Actions tab (`Build` workflow): a `.dmg` for macOS (Apple silicon and Intel), a `.zip` for
  Windows (x64 and ARM64) and an AppImage for Linux x64. They are unsigned: on macOS run `xattr -cr SKradle.app` once,
  on Windows choose "More info" then "Run anyway".

### Fixed

- The app crashed on exit on macOS (Qt's destructors ran with the interface still alive); it now closes in order.
- The Launching button was hard to read in the light theme; it is now an opaque brand pink with white text.
- Mod downloads are streamed to disk and verified there instead of being held in memory.
- Stopping the local server during start-up ends the wait at once; a failed start leaves nothing running.
- Several smaller safety and correctness fixes found by the code review (names that Windows cannot store, links
  inside mod archives and backups, Java 8 version numbers, Discord presence replies, `getdown.txt` comments).

### Known gaps

- Servers, Mods and News pages still show sample data in places; the real lists are being wired in.
- No installer, no code signing, no automatic update of the launcher itself yet.
- Windows: Steam libraries outside the default location are not found through the registry yet.
- Linux arm64 is not supported (the game ships no Linux arm64 runtime).
