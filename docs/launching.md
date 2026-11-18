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
