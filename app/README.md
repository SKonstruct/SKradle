# SKradle application (`skradle-app`)

The desktop launcher: a Qt Quick (QML) interface, embedded in the binary, on top of [`skradle-core`](../core) through
[cxx-qt](https://github.com/KDAB/cxx-qt) 0.10. The binary is called `skradle`.

## Building

Needs Qt 6.8 or newer (developed on 6.11.3) and a Rust toolchain (1.85+). The workspace's default members are only
`core`, so `cargo build` and `cargo test` without flags need no Qt; build this crate with `-p skradle-app`.

```sh
export QMAKE=/path/to/Qt/6.11.3/macos/bin/qmake      # cxx-qt finds Qt through qmake (or `qmake6` on PATH)
export PATH=/path/to/Qt/6.11.3/macos/bin:$PATH
cargo build -p skradle-app --release
cargo test  -p skradle-app
cargo run   -p skradle-app --release
```

Qt modules used: Core, Gui, Qml, Quick, QuickControls2, QuickShapes, QuickDialogs2, QuickVectorImage (the logo) and
the optional **WebView** (see below). `lrelease` from the same Qt install compiles the translations; without it the
build still works and the UI stays English (a build warning says so).

### Flags and environment

| Flag | Effect |
| --- | --- |
| `--selftest` | Starts the QML engine, visits every page, opens "My server", checks that no QML binding or type warning was raised, prints one line and exits 0 (1 on failure, also after 60 s). Use `QT_QPA_PLATFORM=offscreen` on Linux. Uses a temporary settings folder and never starts the game. |
| `--shots <dir> [--prefix name]` | Saves a screenshot of each state (about 60) into `<dir>` as `<prefix>-<state>.png`, then exits. Uses a temporary settings folder. |
| `--scheme light\|dark` | Forces the colour scheme (the Appearance setting otherwise). |
| `--pseudo` | Pseudo-localisation: every word is lengthened and accented to find clipping. |
| `--style <Name>`, `--page <0-3>`, `--collapsed`, `--size WxH` | Qt Quick Controls style (default Basic), first page, sidebar collapsed, window size. |

| Variable | Effect |
| --- | --- |
| `SKRADLE_REDUCE_MOTION=1` | Reduced motion (also a setting in Settings). |
| `SKRADLE_CONFIG_DIR` | Folder for `settings.json` and downloaded launcher updates (default: the OS config folder, `SKradle` inside it). |
| `SKRADLE_SENTRY_DSN` | Read at **build** time: the address crash reports go to. Without it nothing is ever sent, whatever the setting says. |

The old prototype's `--bench` frame-time recorder was not ported (it needed render-thread C++).

## Debugging in VS Code

`.vscode/launch.json` has debug configurations for CodeLLDB (`vadimcn.vscode-lldb`; the folder recommends it): the app,
the app in dark with reduced motion, with the software renderer, with a scratch settings and mods folder (so debugging
never touches yours), the self-test, and unit tests for `skradle-core` and `skradle-app`. Each app configuration first
runs the `build skradle (debug)` task. The build needs Qt: start VS Code from a shell where `QMAKE` is Qt's `qmake` and
Qt's `bin` folder is on `PATH` (see Building above), or the build task fails with a clear message. Breakpoints work in
the Rust code; QML errors appear in the Debug Console (`QT_FORCE_STDERR_LOGGING=1`).

## Structure

```text
app/
  build.rs            QML module (every qml/*.qml; `pragma Singleton` files become singletons), resources,
                      lrelease, QtWebView detection
  cpp/glue.{h,cpp}    the Qt calls cxx-qt-lib does not wrap (see below); the only C++ in the repository
  qml/                the UI (the pink design)
  qml-web/WebPane.qml the web view, deliberately NOT part of the QML module
  i18n/skradle_en.ts  the English catalogue (source of truth for strings)
  resources.qrc       brand assets (../assets) and WebPane.qml
  src/
    main.rs options.rs reporter.rs
    settings.rs discovery.rs selfupdate.rs server_host.rs servers.rs
    launch/{mod,plan,planned,log,memory,supervisor,winprobe}.rs
    bridge/*.rs       the QObjects
```

Everything with logic is plain Rust with unit tests (59 of them); the files in `bridge/` only copy state between those
modules and Qt properties. Long operations never run on the UI thread: they run on worker threads and post their
result back with cxx-qt's `CxxQtThread::queue`.

`#![allow(unsafe_code)]` is limited to `bridge/` (the cxx-qt macros expand to `unsafe`); the rest of the crate is under
`deny(unsafe_code)`.

### Bridge API (QML names)

| Singleton | Properties | Methods |
| --- | --- | --- |
| `AppEnv` | `selftest pseudo reduceMotion scheme styleName version qtVersion webViewAvailable initialPage initialCollapsed initialWidth initialHeight shotDir shotPrefix` (constant) | `report(text) localPath(url) grab(path) finishSelftest(ok, text)` |
| `Settings` | `theme reduceMotion language memoryMb javaPath extraJvmArgs minimizeWhilePlaying collector disableExplicitGc fileCleaning filePurging discordPresence crashReports autoUpdate graphicsEnabled renderScale sharpening antiAliasing sharpHud showFps frameGenMode targetFps loadWarning` (read/write, validated, saved in the background) | |
| `GameModel` | `state` (`searching` `ready` `notfound`) `installPaths installKinds selectedPath selectedKind gameVersion detectedMemoryMb error` | `refresh() select(path) addCustomFolder(url) forgetCustom() force(state)` |
| `LaunchModel` | `state` (`ready` `updating` `launching` `running` `error`) `step` (`checking` `downloading`) `message progress determinate` | `start(path) stop() cancel() dismiss() force(state, progress)` |
| `UpdateModel` | `state` (`idle checking uptodate none noinstaller available downloading ready installing error`) `version progress message failedDuring reason automatic` | `check() checkAutomatically() download() install() skip()` |
| `HomeFeed` | `newsJson newsState newsFetchedAt playersState players playersJson eventsJson eventsState eventsFetchedAt` | `refreshNews() refreshPlayers() refreshEvents() countdown(startsAt, endsAt) force(state) forceEvents(state) forcePlayers(state)` |
| `ReleaseNotes` | `json` (constant: the parsed changelog) | |
| `GameTools` | `javaVersion javaSource javaProblem javaFound javaNeeded agentJavaOk agentInstalled totalMemoryMb memoryMinMb memoryMaxMb editorBlockers busy notice noticeDetail noticeSerial` | `refresh(path) openFolder(path) launchEditor(i, path) resetGameFiles(path)` |
| `ModsModel` | `items tags folder folderUrl error busy loaded result packState packMessage packCurrent packTotal packName packChanges packReport` (JSON strings for lists) | `refresh(version) setEnabled(json, on) remove(json) add(json) ensureFolder() checkPack() syncPack() cancelSync() dismissReport() forcePack(state)` |
| `ServerHost` | `state url status` (JSON snapshot) | `refresh() install(accepted) updateEngine() start() stop() restart() backup() restore(name) backupsJson() setLan(on) setPorts(game, web) resetAdminPassword() adminPassword() dismiss() termsJson() force(name)` |
| `ServerBook` | `entries probes selectedId selectedName error` (JSON; probes by id) | `reload() select(id) check(draft) save(draft) draftOf(id) summaryOf(id) remove(id) portOf(address) probeAll() force(name)` |

`force(...)` exists for screenshots and does nothing else. The state properties have a `WRITE` only because cxx-qt
generates its Rust setter from it; nothing in QML assigns them.

### Home: players, events and release notes

**Players online** is an estimate: `core::steam_api` asks Steam for Live (app 99900) and Preview (99920), multiplies each
count by `STANDALONE_FACTOR` (1.4, because Steam counts only part of the player base), rounds each to the nearest whole
player and sums them. Either request failing means "not available" (never half an estimate). Refreshed at most once a
minute while Home is visible. The news shows the 3 newest items.

**Live events** (`core::live_events`, `HomeFeed.eventsJson`): read from a third-party feed that is cleartext and
optional, see `docs/live-events.md`. The Official server's events that have not ended (upcoming ones stay) are shown;
the default event only when nothing else is left. Each shows its title and description, a countdown chip ("Ends in
2d 5h", "Ends today, 5h 12m left", "Starts in ...", "Ended"; `live_events::countdown` gives numbers, `HomePage.qml`
words them, refreshed every minute), a rounded banner (`EventBanner.qml`, HTTPS PNG or JPEG only) and a **Learn more**
button (HTTPS only). The last good answer is cached in `live-events.json` and banners in `event-images/` in the config
folder; a failed request shows the cache with an offline note, and no events at all hides the card. Plain HTTP is
allowed for this one host only (`Client::get_text_insecure_for_events` in `core/src/net.rs`); the app works the same
when the feed is unreachable. Self-test and screenshot runs use fixture events relative to now.

**Release notes** (`core::changelog`, `ReleaseNotes.json`): `CHANGELOG.md` is embedded with `include_str!` (cargo rebuilds
when it changes) and parsed in Rust into versions, sections and items; inline `**bold**`, `*italic*`, `` `code` `` and
`[text](https://...)` become escaped Qt rich text (anything else is shown as text, non-https links lose the link). The
viewer (`ReleaseNotesDialog.qml`, one `VersionBlock.qml` per version) needs no network: the newest version starts open, each
block has its own minimise/maximise header and the dialog has Expand all and Collapse all. Opened from the Home header
button (which shows the version) and from Settings, About, through `Data.releaseNotes()`. Links open in the system
browser.

### Settings

`settings.json` in the OS config folder. Unknown keys are kept and written back; a value of the wrong type falls back
to its default; a file that is not a JSON object is moved to `settings.json.corrupt` and defaults are used; writes are
atomic (temporary file, then rename) and coalesced. The memory limit is `null` until the player changes it: until then
the game's own `-Xmx` in `extra.txt` is shown and left alone. When set, it is written to the game's `extra.txt`
(`-Xmx`, and `-Xms` lowered to fit) before launching. The Graphics page values are stored (and used only while "Enable graphics enhancements" is on, off by default) but **the game does not read
them unless the graphics agent is installed (see below; `TODO(graphics)` for downloading it).

### Launching and the updater's progress

`LaunchModel` asks a `Launcher` (trait, `launch/mod.rs`) to run a launch and maps its events to the states above.
`PlannedLauncher` (`launch/planned.rs`) is the implementation: on the supervisor's thread it picks a Java (the
player's choice is checked first so its problems keep their precise messages), gathers the planner's inputs and runs
`skradle_core::launch::plan` with the real effects. The planner decides (see `docs/launching.md`): a pending update or
missing files go through the game's own updater (the memory limit is written to `extra.txt` for it); otherwise mods,
then the server settings, then the game is started **directly** (memory and extra JVM arguments through
`DirectOptions`), and a failed step stops the launch before the game starts. The supervisor follows an updater launch
as described below and a direct launch as the game itself: "launching" until it has lived 3 s, then "running"; a
non-zero exit within 20 s is a failed start and carries the end of the game's output. A private server is refused while
the game needs an update. Planner warnings and mod notes go to standard error for now (no UI for them yet).

Where the inputs come from: the selected server from `servers.json` in the config folder (none is Official; written by the Servers page, `servers.rs`), the player's mods from `<data dir>/SKradle/mods` (no folder, no mods), the
graphics agent from `<data dir>/SKradle/skgfx/skgfx-agent.jar` (no file, no graphics enhancements; the Graphics page
values become `-Dskgfx.*` arguments through `skradle_core::graphics`; the anti-aliasing choice has no agent property
and is not used; **nothing downloads the agent yet**, `TODO(graphics)`), and "an update is pending" from the game's
newest `getdown.txt` (fetched with short timeouts; offline counts as no update).

What the game's updater (`getdown-pro.jar`, read in the installed game's classes; **it was not run in tests**) shows:

* Progress goes only to the updater's own window. Standard output and error are redirected into `launcher.log` in the
  game folder, and that log has phase messages only ("Application requires update.", "Downloading resource ...",
  "Running ..."), no byte counts or percentages. So **the progress ring is indeterminate**; the log only tells
  "checking" from "downloading" and "starting the game".
* After it starts the game the updater exits (about a second later; only Windows 98/ME keeps it alive). The game
  keeps running as an orphan, so the process SKradle starts is only the updater. On Unix it is started in its own
  process group, which the game inherits; "running", "stop" and "exited" are about that group (`kill -0` and
  `kill -TERM -<group>`, KILL after 5 s). On Windows the game is found by its command line with PowerShell and ended
  with `taskkill /T /F`; this code is **untested**.
* Not run silently, the updater shows its own window during the update. Hiding it (`-Dsilent=launch`) would remove the
  only progress display the player has, so that waits for a determinate progress source (e.g. from `digest.txt`).

Tests (`cargo test -p skradle-app`) include process control with fake `java` shell scripts: the full sequence
updating, downloading, launching, running, stopped; the game exiting by itself; a failing updater with its output; a
clean exit with no game; a missing program; cancelling during the update; and a game that ignores TERM (killed after
the grace period). Unix only.

### Web view ("My server")

The "My server" panel embeds the server's own web interface. It is isolated so that no missing piece can break the
window:

* `qml-web/WebPane.qml` (the only file that imports `QtWebView`) is a plain resource, not part of the QML module, and
  is loaded by URL through a `Loader` in `MyServerPanel.qml`. Without the module, the `Loader` errors and the panel
  shows a button that opens the system browser.
* The decision is made at run time (`AppEnv.webViewAvailable`): the build has the web view compiled in, the platform
  is not `offscreen`/`minimal` (no foreign windows), and a plugin in `<plugins>/webview` really loads (a plugin file can
  exist without its libraries: the WebEngine plugin without WebEngine).
* `QtWebView::initialize()` must run **before** the `QGuiApplication` exists. `main` is Rust and creates the
  application object, so `glue::init_webview()` (C++, `cpp/glue.cpp`) is called first. If the Qt install has no
  QtWebView, `build.rs` does not link it and that function does nothing; one source tree builds either way.
* The address and state come from the `ServerHost` singleton, which wraps the `ServerHost` trait in `server_host.rs`
  (the real host on `core::serverhost`; see `docs/server-host.md`, "In the app"). The panel is reached with "Open web UI"
  in the server control strip and only has content while the server runs. In the self-test and screenshot runs a
  fixture host serves an inline page that sets its own title, so the embedded view is still checked.
* A native web view is drawn above Qt Quick and cannot be clipped by a scroll view: the panel is a page of its own
  (reached with the "My server" button on the Servers page), not a card in a list. Toasts and dialogs would appear
  behind it. `grabWindow` (the screenshots) does not capture it.

Findings for Qt 6.11.3 (checked by listing the packages `aqt` downloads, and on macOS by running):

| Platform | `qtwebview` add-on in the Qt repository | Backend plugins inside | Embeds? |
| --- | --- | --- | --- |
| macOS (arm64/x64) | yes (`clang_64`) | `darwin` (WKWebView), `webengine` | **yes, verified**: the stub page loaded and its script ran |
| Windows x64 | yes (`win64_msvc2022_64`) | `webview2`, `webengine` | expected (WebView2 runtime needed on the machine); not run |
| Windows arm64 | yes (`win64_msvc2022_arm64`, also the cross-compiled build) | `webview2`, `webengine` | expected; not run |
| Linux x64 | yes (`linux_gcc_64`) | `webengine` only | **no** without QtWebEngine and its dependencies installed; the browser fallback applies |

Installing the add-on locally: `aqt install-qt mac desktop 6.11.3 clang_64 -m qtwebview --archives qtwebview -O <Qt dir>`
(`--archives qtwebview` adds only that package; the plugin and QML module are in it, no WebEngine needed on macOS and
Windows). CI's `install-qt-action` step needs `modules: qtwebview` once CI builds this crate.

### Translations

Every visible string goes through `qsTr` (context = the QML file's name); strings in fake-data blocks use
`QT_TRANSLATE_NOOP("Data", ...)` and `qsTranslate("Data", ...)`. `i18n/skradle_en.ts` is the catalogue, regenerated
with Qt's `lupdate`:

```sh
cd app && lupdate qml qml-web -source-language en -ts i18n/skradle_en.ts -no-obsolete -locations none
```

`build.rs` compiles every `i18n/skradle_*.ts` with `lrelease` into the binary (`:/i18n/skradle_<lang>.qm`);
`glue::set_language` installs the one for the Language setting ("system" follows the OS) and retranslates the running
UI. `--pseudo` installs a generated pseudo-localising translator instead. Gaps: sentences produced in Rust
(`LaunchError`, folder errors, the settings warning) are English and bypass `qsTr`; they should become error kinds with
the wording in QML before a second language is added. Only English exists, so the loader is exercised through the
pseudo mode.

### What is real and what is placeholder

Real: every page. The Servers page (saved list, selection, probes, the hosted server), Home (Steam news and player
count), Mods, game discovery and the custom folder, Java choice and the launch state machine, every Settings option
(memory, collector, explicit GC, file cleaning and purging, reset game files, graphics values, Discord switch, editors,
update check, crash reports, theme, motion, language). Self-test and screenshot runs use compiled-in fixtures.

Notes: memory, collector, explicit GC and extra arguments reach the game on a direct launch; a launch that first runs the
game's updater only passes the memory limit (through `extra.txt`). Anti-aliasing has no agent property and is not offered.
Discord presence uses the application id in `core::presence::DISCORD_APPLICATION_ID` and stays opt-in (the Settings switch, off by default); it does nothing while that constant is empty. Reset game files uses `core::reset`.
