# Launching the game

SKradle can start Spiral Knights five ways. All of them only build a command (`CommandSpec`); the caller runs it.
Facts below were read from the game's own files (`getdown.txt`, `getdown-pro.jar`, `projectx-pcode.jar`) without running
the game; "unverified" marks what needs a real run.

| Path | Code | Updates and verifies | Which Java runs the game |
| --- | --- | --- | --- |
| Updater launch | `getdown::update_and_launch` | yes | `java_vm/bin/java` if it exists, else the updater's own Java |
| Direct launch | `launch::direct` | no | the one the caller passes |
| Alt mode | `launch::alt` | no | the one the caller passes |
| Editors | `editors` | no | the one the caller passes |
| Fresh install | `install::fresh_install_command` | downloads everything | the updater's own Java (no `java_vm` yet) |

## Updater launch

`java -jar getdown-pro.jar . client` in the game folder. The updater takes `gettingdown.lock`, checks the version,
downloads and verifies files against the signed digests, then starts the game. Use it by default and after every game
update. It builds the game's command from `getdown.txt` (`code`, `jvmarg`, `<appid>.class`, `extra.txt`).

## Direct launch

Builds the same game command the updater would (`java -classpath <jars> <jvmargs> <extra.txt> <main class>`, with
`%APPDIR%`/`%VERSION%`/`%ENV.X%` substituted and platform qualifiers evaluated) and runs it without the updater. It
**skips the update check, the digest check and file repair**, so use it only for a game that was just updated or is
known good. `launch::direct::missing_code_files` catches a half-installed game. The main class is
`com.threerings.projectx.client.ProjectXApp` (`class`/`client.class` in `getdown.txt`); the game takes `+connect=` as an
argument and `username`, `password`, `encrypted`, `knight`, `action`, `arg`, `sessionKey` as system properties.

Memory can be overridden (`DirectOptions::memory` removes every heap flag from `getdown.txt` and `extra.txt` first, so
a small `-Xmx` never clashes with a larger `-Xms`). Extra lines go through `jvm_args`, system properties through
`system_props`.

## Alt mode

A second client next to the main one, started the direct way with a capped heap (default 256 MiB initial, 1 GiB maximum;
a guess) and without the graphics agent's `extra.txt` lines. It must use a different game account.

What stops a second client, from inspection only:

- The updater holds an exclusive lock on `gettingdown.lock` while it lives and refuses to run twice
  ("another getdown running"). It only waits for the game on Windows 98/ME (`mustMonitorChildren`), so on current
  systems it should exit soon after starting the game, but this was not observed. Direct launch never takes the lock.
- No class in `projectx-pcode.jar` uses `FileLock`, `ServerSocket` or `ServerSocketChannel` apart from the embedded
  updater and a server-side connection manager, so the game has no single-instance guard of its own.
- Unverified: whether Steam refuses a second start, both clients writing `projectx.log` in the game folder, shared
  client preferences, and the server logging an account out when it logs in elsewhere.

## Editors

`editors::editor_command` runs an editor with the client's classpath and JVM arguments in the game folder:

- Crucible scene editor: app id `crucible-editor`, `com.threerings.projectx.tools.CrucibleSceneEditor`. Needs
  `-Dcrucible.dir`; `getdown.txt` sets `../crucible` (relative to the folder above the game) but `crucible.jar` unpacks
  to `crucible/` inside the game folder, so the command overrides it with the real path.
- Scene editor: `com.threerings.projectx.tools.h`, has a `main`, no app id. Obfuscated name, may change in any update.

Not found: a model viewer, a particle editor, Silverweave. No class with those names has a `main` in the game's jars.

## Fresh install

`getdown.txt` plus `code/getdown-pro-new.jar` in an empty folder, then the updater with `-Dsilent=true`. The app base must
be on the game's own hosts. See `install`.

## Which Java, and the `java_vm` question

The game's own `java_vm` is a trimmed runtime with no `java.instrument` module (its `release` file lists `java.base`,
`java.datatransfer`, `java.xml`, `java.prefs`, `java.desktop`, `java.logging`, `jdk.accessibility`, `jdk.crypto.ec`,
`jdk.unsupported`), so the graphics agent cannot load in it. `getdown.txt` asks for `java_version = 25000000` and gives a
`java_location` jar per platform that the updater unpacks to `java_vm`.

What the updater does (`Application.haveValidJavaVersion`, `Application.createProcess`, `LaunchUtil.getJVMBinaryPath`):

1. It logs `Checking Java version [current=... wantMin=... wantMax=...]`. If `java_vm` holds a valid runtime it reads that
   runtime's `release` file for "current"; otherwise "current" is the Java running the updater. If it is below
   `java_version`, the updater downloads and unpacks `java_vm`.
2. It starts the game with `java_vm/bin/java` **if that file exists**, and falls back to the `java.home` of the running
   updater only when it does not. `java_vm` is not listed in the digests, so a missing one is not "repaired".

So the design insight holds only in part. The updater does not always use its own Java for the game: with the usual
installed `java_vm` present, the game starts in the trimmed runtime whatever Java ran the updater. The launcher can use
its own full Java for the game in two ways: the direct launch (never looks at `java_vm`, recommended), or an updater
launch with `java_vm` absent and a full Java 25+ running the updater (then step 2 falls back to it). The second needs
`java_vm` moved away from the game folder, which Steam's file verification may restore; that behaviour, and the
updater actually exiting cleanly in that setup, are unverified. Neither path modifies a file the updater verifies.

An `-Dsilent=noupdate` property makes the updater skip the update step and the lock (`SysProps.noUpdate`); it was read
from the bytecode and never tried.

## The launch planner

`skradle_core::launch::plan` decides how a launch runs and `execute` runs it through an `Effects` implementation
(`RealEffects` is the real one). The order matters, and each wrong order fails silently, so it is fixed in one place:

1. **An update is pending, or game files are missing:** the updater runs (after the graphics module's lines are removed
   from `extra.txt`). It verifies and repairs the game, which replaces every file a mod changed, and starts the game
   once on the game's own runtime, so mods and the graphics agent apply from the next launch. A private server is
   refused in this state (`Refusal::UpdateRequiredForPrivateServer`), because the updater would restore `config.jar` and
   the game would connect to Official.
2. **Otherwise:** apply mods, then write the server settings into `config.jar` (applying mods rebuilds that jar from
   the game's original, which discards the settings), then start the client directly. The updater must not start the
   game here: it would see the changed files and put the originals back.
3. **The graphics agent** is behind a master switch (`graphicsEnabled`, Settings > Graphics, **off by default**, also
   for settings files written before it existed). Off: `app/src/launch/planned.rs` passes no agent arguments, so the
   planner picks the first runtime that satisfies the game, which is the game's own `java_vm` (or the player's Java
   path override, which always wins). On: the agent needs a Java that can load it (24 or newer with
   `java.instrument`); the game's own trimmed runtime cannot. The direct launch uses such a runtime and passes the agent as JVM arguments, never through
   `extra.txt`. With none available the game starts without it and the plan carries a warning.

`execute` stops at the first failing step: nothing after it runs and the game is not started on a half-applied state.
The pending-update check (`servers::update_pending`) treats an unreachable server as no update, so an offline private
server still launches.

## Steam installs and sign-in

A Steam install signs in through the running Steam client, and the game decides that itself: `ProjectXApp.init` calls the
Steam API (`SteamAPI_Init`, `native/libsteam_api`, found through the `java.library.path` that `getdown.txt` sets). When
it succeeds the game logs in as the Steam account (log line `Steam initialized.`) and shows the character screen; when it
fails the game shows the password screen. The API needs a running Steam client of the same user **and** the app id, which
Steam gives the games it starts as the environment variables `SteamAppId` and `SteamGameId` (`99900`). A game started any
other way had no app id, so Steam was running and the password screen came up anyway.

`launch::steam` therefore adds `SteamAppId=99900` and `SteamGameId=99900` to the command (`CommandSpec::env`) for an
install of kind Steam on Official, on both paths. The updater path sets them on the updater, whose child inherits them
(observed). The working directory is the game folder on both paths, as Steam's. SKradle starts the game itself and not
through `steam://rungameid/99900`, because a Steam launch cannot carry the direct launch's JVM arguments, mods or graphics
agent (Steam only takes launch options, and the game then runs on its own trimmed runtime).

- **Steam not running:** the plan is refused (`Refusal::SteamNotRunning`) before anything is applied or started, and the
  app starts Steam (`steam://open/main`) and says so. Detection: process `steam_osx` (macOS), `steam` (Linux),
  `steam.exe` (Windows, `tasklist`).
- **Private server:** no Steam variables and no Steam check; the game uses the server's own accounts and the Steam identity
  is not sent. A standalone install is unchanged.
- Not set: the overlay (`DYLD_INSERT_LIBRARIES`/`LD_PRELOAD`), so no Steam overlay in the game; the sign-in does not need it.
- Verified on macOS (arm64) against a real Steam client: the same game command without the variables shows the password
  screen; with them the game reports `Steam initialized.` and reaches the character screen, direct and updater paths. Not
  verified: Windows and Linux, and a Steam account other than the one signed in here.

## Player settings in the launch

`app/src/launch/planned.rs` maps the settings to the plan: `direct_options` (memory, `GarbageCollector`, `DisableExplicitGC`,
the player's extra arguments last) and `mod_options` (file cleaning and purging as `ApplyOptions`). A chosen collector
replaces any `-XX:+Use*GC` flag of `getdown.txt` and `extra.txt` (two collectors stop the JVM); the player's own collector
argument wins. These apply to the direct launch only. Both mod switches are documented in `docs/mods.md`.

## Launcher updates

`core/src/update.rs` reads the project's release list (`releases?per_page=20`, not `releases/latest`, which answers 404
while nothing is published) and keeps the highest plain `x.y.z` release that is neither a draft nor a pre-release. The
check ends in one of: no releases yet (empty list, only hidden releases, or 404; a calm answer), up to date (not newer
or skipped), a newer release without an installer and SHA-256 for this OS and CPU, or an update to offer. Failures are
sorted into rate limit (403/429, "try again in about an hour"; GitHub's header is not read), offline or timeout, server
error, malformed data and checksum mismatch. "Update automatically" (`autoUpdate`; the old `autoUpdateCheck` is read
when the new key is absent and kept in step) checks a moment after the window appears, never in self-test or screenshot
runs, downloads a found update in the background (SHA-256 verified) and shows an "Update ready" banner with the install
action; an automatic run never shows an error. "Check for updates" in Settings > About always works.

## Tools

The Tools section starts only editors that the game's own jars contain (Crucible scene editor, ProjectX scene editor);
they are delivered and updated by the game itself, so there is no download system. The Model Viewer, Particle Editor and
Interface Tester exist in the game only as message bundles; their classes are not in the jars. See `core/src/editors.rs`.
