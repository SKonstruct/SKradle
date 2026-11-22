# Launching the game

SKradle can start Spiral Knights five ways. All of them only build a command (`CommandSpec`); the caller runs it.
Facts below were read from the game's own files (`getdown.txt`, `getdown-pro.jar`, `projectx-pcode.jar`) without running
the game; "unverified" marks what needs a real run.

| Path | Code | Updates and verifies | Which Java runs the game |
| --- | --- | --- | --- |
| Updater launch | `getdown::update_and_launch` | yes (only when files are missing or an update is known locally) | `java_vm/bin/java` if it exists, else the updater's own Java |
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

0. **No network on launch.** A launch never fetches the game's `getdown.txt`. "Update pending" comes only from what is
   known locally: a Steam install is never pending (Steam updates it); for a standalone install the marker left by the
   last background check (`launch::pending`, `<data dir>/SKradle/game-update-pending.txt`) is compared with the installed
   version. After a direct launch of a standalone install a background check (4 s connect, 8 s total, off the launch's
   path) refreshes the marker and logs the result, so a new game version is noticed on the next launch. Missing files
   still send the launch through the updater.
1. **An update is pending, or game files are missing:** the updater runs (after the graphics module's lines are removed
   from `extra.txt`). It verifies and repairs the game, which replaces every file a mod changed, and starts the game
   once on the game's own runtime, so mods and the graphics agent apply from the next launch. A private server is
   refused in this state (`Refusal::UpdateRequiredForPrivateServer`), because the updater would restore `config.jar` and
   the game would connect to Official.
2. **Otherwise:** apply mods, then write the server settings into `config.jar` (applying mods rebuilds that jar from
   the game's original, which discards the settings), then start the client directly. The updater must not start the
   game here: it would see the changed files and put the originals back.
3. **The graphics agent** needs a Java that can load it (24 or newer with `java.instrument`); the game's own trimmed
   runtime cannot. The direct launch uses such a runtime and passes the agent as JVM arguments, never through
   `extra.txt`. With none available the game starts without it and the plan carries a warning.

`execute` stops at the first failing step: nothing after it runs and the game is not started on a half-applied state.
`servers::update_pending` compares two `getdown.txt` versions; the launch feeds it the recorded version. Nothing recorded
(first run, offline) counts as no update, so an offline private server still launches; the safety rule is unchanged: a
private server is refused while an update is known to be pending.

## Steam installs and sign-in

A Steam install signs in through the running Steam client, and the game decides that itself: `ProjectXApp.init` calls the
Steam API (`SteamAPI_Init`). The game's Steam layer (`com.threerings.froth`, Java foreign-function calls) does
`System.loadLibrary("steam_api64")` (`steam_api` on 32-bit) and looks the functions up in it, so it needs a 64-bit Java
and `native/steam_api64.dll` on `java.library.path` (`getdown.txt` sets `-Djava.library.path=%APPDIR%/native`; the direct
launch substitutes `%APPDIR%`, quoting is not needed because arguments are passed one by one, so spaces in the path are
fine). When the API succeeds the game logs in as the Steam account (log line `Steam initialized.`); otherwise it shows
the password screen. The API needs a running Steam client of the same user, signed in, **and** the app id, which Steam
gives the games it starts as `SteamAppId`/`SteamGameId` (`99900`) and which the API also reads from `steam_appid.txt`
in the working directory.

Causes found for "Steam mode still opens the password screen" (Windows report), and what each now does:

1. **The launch never knew the install was a Steam install (the real cause, every OS).** The launch button passes only a
   folder, and `LaunchModel` built the request with kind `Custom`, which counts as standalone: no variables, no Steam
   check. Now the launcher works the kind out on its worker thread (`discovery::detect_kind`): the kind discovery gave
   the folder, else Steam when the path is `...\steamapps\common\...` (any drive, any case, spaces) or sits next to
   Steam's manifest for app 99900.
2. **Steam not running or not signed in.** Running is not enough: with no signed-in user the API fails. The client state
   is read on Windows from `HKCU\Software\Valve\Steam\ActiveProcess` (`pid`, `ActiveUser`; 0 means signed out), the
   pid confirmed with `tasklist` (a stale registry value after a crash is ignored), and the process name as fallback; on
   macOS and Linux from `pgrep` and `ActiveUser` in Steam's `registry.vdf` (no value: assumed signed in). All probes run
   without a console window (`CREATE_NO_WINDOW`).
3. **Steam is started and waited for.** If it is not running or not signed in, SKradle opens it (`cmd /C start "" steam://open/main`
   on Windows, `open` on macOS, `xdg-open` then `steam` on Linux), the launch shows the step `steam` ("Starting
   Steam...") and polls every second for up to 60 s, cancellable, then goes on without another click. Timeout gives a
   message that says what to do (start Steam, sign in, launch again). The logic is `steam::ensure_ready` with a fake clock in
   its tests.
4. **`steam_appid.txt`.** For a Steam install on Official, on Windows and Linux, `steam_appid.txt` (`99900`) is written
   into the game folder when missing (best effort, a failure is logged and never stops the launch; never on macOS, never
   for a private server). It covers a Steam API that looks for the file instead of the variable.
5. **Environment and working directory.** `SteamAppId` and `SteamGameId` are set on the child (`CommandSpec::env`) on both
   paths; the child inherits the rest of SKradle's environment. On the updater path the updater's own child inherits them
   (observed on macOS only). The working directory is the game folder on both paths. The game process is started with
   `CREATE_NO_WINDOW` on Windows (it was opening a console window for `java.exe`; output is captured through pipes).
6. **Steam run at another privilege level.** The API fails when Steam runs elevated and the game does not (or the
   reverse). Not detected; run both normally.

SKradle starts the game itself and not through `steam://rungameid/99900`, because a Steam launch cannot carry the direct
launch's JVM arguments, mods or graphics agent.

- **Private server:** no Steam variables, no file, no Steam check; the game uses the server's own accounts.
- Not set: the overlay (`DYLD_INSERT_LIBRARIES`/`LD_PRELOAD`); the sign-in does not need it.
- Verified on macOS (arm64) against a real Steam client before this round: the same game command without the variables shows
  the password screen; with them the game reports `Steam initialized.`. **Not verified: Windows and Linux**, and the new
  wait and sign-in probes anywhere. The log (`docs/logging.md`) records every decision: Steam state, the variable names
  set, the command's program path and argument count, exit codes, and the end of the game's `launcher.log` on an early failure.

## Player settings in the launch

`app/src/launch/planned.rs` maps the settings to the plan: `direct_options` (memory, `GarbageCollector`, `DisableExplicitGC`,
the player's extra arguments last) and `mod_options` (file cleaning and purging as `ApplyOptions`). A chosen collector
replaces any `-XX:+Use*GC` flag of `getdown.txt` and `extra.txt` (two collectors stop the JVM); the player's own collector
argument wins. These apply to the direct launch only. Both mod switches are documented in `docs/mods.md`.
