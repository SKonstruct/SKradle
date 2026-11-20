# Hosting a private server

SKradle can run a private Spiral Knights server on the player's own computer and connect the game to it. The server is
**Tortodrone**, a separate Java program by its own author (<https://github.com/N0rule/Tortodrone>). SKradle manages it
(download, start, stop, backups, connection details) and embeds the engine's own web administration page; it does not
reimplement the engine's administration. Code: `core/src/serverhost/`. Facts below come from the engine's documentation
and source (version 0.40.x) and from tests against stand-in `java` scripts. **No engine jar was available, so nothing
here has been run against the real engine**; "unverified" marks what needs a real run.

## What is managed

| Part | Module | What it does |
| --- | --- | --- |
| Folder | `layout` | Paths in the server folder; names from archives are checked (no `..`, absolute paths, backslashes, colons, links). |
| Licence | `terms` | A recorded acceptance of the engine's licence; nothing is downloaded before it. |
| Engine | `engine` | Finds the newest release, downloads it, checks the SHA-256 GitHub publishes, unpacks only what is needed, keeps the previous engine. |
| Configuration | `config` | Writes the web and port settings into `config/server.yml` without touching any other line or comment. |
| Process | `process` | Starts the engine as a child JVM, captures its output, checks readiness, stops it gracefully, serialises lifecycle operations. |
| Admin account | `admin`, `prompts` | Creates the WebUI administrator with a generated password and keeps it in a private file. |
| Ports | `ports` | Detects busy ports before a start and probes the web listener. |
| Backups | `backups` | Copies of `data/` and `config/` while stopped, restore, reset. |
| Registration | `register` | The "Local Server" entry for the launcher and the join details for other players. |

The app embeds the engine's WebUI at `http://127.0.0.1:<web port>/admin/` (`HostConfig::web_ui_url`, also in
`Status::web_ui_url` while the server runs). Accounts, knights, items and the engine's console page are the WebUI's job.

## Folder layout

```text
<server folder>/
  tortodrone.jar  tortodrone.jar.previous       the engine and the one before it
  content/  content.previous/                   maps and items; replaced as a whole on update
  webui/    webui.previous/                     the pages served under /admin/
  config/server.yml  config/combat.yml          the engine's settings (never overwritten by an update)
  plugins/                                      empty; the engine uses it
  data/                                         the engine's state: tortodrone.db, webui.sqlite, keys/, logs/
  backups/<yyyymmdd_hhmmss>[-label]/{data,config,backup.json}
  LICENSE  NOTICE  THIRD-PARTY.md  licenses/    the engine's licence texts, kept beside the jar
  engine.json                                   installed version and hashes (the launcher's record)
  terms-accepted.json                           the recorded licence acceptance
  admin-credentials.json                        the generated WebUI administrator (mode 0600)
```

The engine finds `data/`, `content/` and `webui/` beside its jar, not in the working directory. The launcher runs it
with the server folder as the working directory.

## Licence handling

Tortodrone is under the **PolyForm Noncommercial License 1.0.0**; SKradle is MIT. The two stay apart:

- SKradle contains no part of the engine: not the jar, not the content pack (which holds game-derived data its author
  does not license for redistribution), not its text. `core/src/serverhost/terms.rs` holds the launcher's own summary
  of the terms, not a copy.
- The engine is downloaded from `https://github.com/N0rule/Tortodrone/releases/download/<tag>/Tortodrone-<tag>.zip`
  (the URL must be exactly that) over HTTPS, only after the player has confirmed the terms. The confirmation is recorded
  in `terms-accepted.json` and tied to the wording version; an install without it fails with `TermsNotAccepted`.
- The archive's SHA-256 must equal the `digest` GitHub publishes for the asset. A release without one is not offered.
- `LICENSE`, `NOTICE`, `THIRD-PARTY.md` and `licenses/` from the release are kept next to the jar.
- Downloaded content stays on the player's computer. Do not commit or share `content/`.

## Engine install and update

- Releases are listed from the GitHub API; the newest numeric version wins (`0.40.17` is newer than `0.40.8`).
  Pre-releases count (the engine publishes development pre-releases); drafts do not.
- The archive is unpacked into a staging folder. Every entry name is checked first and an archive with one unsafe name,
  a link, an encrypted entry or a repeated name is refused whole. Caps: 1 GiB download, 512 MiB per file, 2 GiB unpacked,
  50 000 entries (guesses; the engine and content are tens of MiB). Byte counts are measured while copying, not taken
  from the archive's headers.
- What is installed: the jar, `content/`, `webui/`, default `config/` files that are missing, the licence texts. The
  archive's `VERSION` must equal the release's, the jar must be a zip, and `config/server.yml`, `webui/index.html`, `LICENSE`
  and `NOTICE` must be present.
- The live files are replaced only when staging is complete. The previous jar, `content/` and `webui/` become `.previous`
  (one generation), and a failed swap puts everything back. `rollback_to_previous` swaps them again. The engine refuses a
  database schema it does not know and does not downgrade one, so after a rollback restore the backup made before the
  update if the old engine will not start.
- `ServerHost::update_engine` stops the server, makes a `before-update` backup (no backup, no update) and installs.
- The WebUI saves News edits and uploaded banners under `content/shared/news`. They are the player's: an update carries that
  folder over into the new `content/`; nothing else in `content/` is kept. **They are not part of the backups** (which hold
  `data/` and `config/` only): copy that folder by hand if the News matters.
- Replacing `webui/` is safe: per the engine's operations guide it restores locally prepared artwork from
  `data/client-artwork/` on the next start.

## Settings the launcher manages

`HostConfig::apply` rewrites exactly these lines of `config/server.yml` and leaves everything else (other keys, comments,
blank lines, line endings) as it was. A file with a line it cannot safely edit around (indentation, a list, a repeated
key) is refused and not touched. The engine refuses unknown settings, so nothing else is ever added.

The engine's own shipped `server.yml` binds every interface in "automatic" Host mode, which is not what the launcher
wants. A first install therefore rewrites it with the loopback-only defaults (keeping its comments), and `start` refuses a
file that is not in the launcher's form (`NotConfigured`: WebUI on, plain HTTP, `webui-public-url` equal to
`http://127.0.0.1:<web port>`, an explicit web port). `ServerHost::configure` writes it, while the server is stopped.

| Key | Value |
| --- | --- |
| `server-ip` | `127.0.0.1`, or `0.0.0.0` when other players may join |
| `server-port` | the game port (default 47624) |
| `webui-enabled` | `true` |
| `webui-tls-mode` | `http` |
| `webui-public-url` | `http://127.0.0.1:<web port>` |
| `webui/content_url-port` | the web port, a concrete number (default 8080) |
| `news-port` (legacy) | removed if present: the engine refuses a file where it disagrees with the line above |

### Exposure: what the engine's source says

- The game listener and the shared HTTP listener (the WebUI under `/admin/`, the News images under `/news/`) both bind
  `server-ip`. There is no way to bind the administration pages to loopback while the game stays on the LAN.
- `webui-public-url` is not a bind address. It selects which `Host` and `Origin` the admin routes accept. With the
  engine's default `0.0.0.0` ("automatic") it accepts the connection's own local IP, or `localhost` for loopback peers;
  with a concrete URL, exactly that authority. A request that fails gets `403` from every route except `/news/`.
- The `Host` header is chosen by the client, so this check protects a browser from being tricked; it is not access
  control. Access control is the WebUI login: every admin API route except first-run `setup` and `login` needs a session
  (loopback peers included), and logins are limited to ten per minute for the whole server. The traffic is plain HTTP.
- **Chosen configuration.** Without LAN hosting everything binds `127.0.0.1`. With LAN hosting the server binds
  `0.0.0.0` and `webui-public-url` stays `http://127.0.0.1:<port>`: browsers on the LAN fail the Host check, players on
  the LAN still reach `/news/` (the content URL the game uses) and the game port, and a hand-made request that spoofs
  the Host still meets the login. It is still only a password over plain HTTP, so enable LAN hosting only on a network you
  trust. The embedded view must use `127.0.0.1`, never `localhost`: mixing them breaks the Host check and cookies.
- The web port is kept as a number in the file. Port `0` would give players a different content URL on every start.
  8080 is commonly taken, so `ports::check` reports it and suggests a free one.

## Administrator account

The WebUI has its own accounts, separate from game accounts. On a fresh server the engine shows a first-run page that
anyone who can reach the port and knows the eight-digit setup code (printed on the console) can use to become administrator.
SKradle closes that window at the first start by creating the account itself over the console:

1. `admin::ensure` generates a 24-character password from the operating system's random source and writes
   `admin-credentials.json` (mode `0600` from creation on unix; a best-effort `icacls` call on Windows, unverified) *before*
   anything is sent to the engine.
2. `webui user skradle ADMINISTRATOR` is sent on the engine's stdin; its two password prompts are answered; success is the
   line `WebUI account saved: skradle`. A refusal (`WARN` line) is reported in `Status::admin`; it does not fail the start.
3. Any account the engine records closes the first-run page for good (its identity store treats any identity event as
   completed setup), so the setup code is useless afterwards. The code line is replaced by `[hidden]` in the output buffer.

The launcher owns the account `skradle` but applies it only when nothing proves the engine has it: no readable credential
file, no `admin-applied` marker (written once the engine accepted the password), or no `data/webui.sqlite` before this start
(first run, a server reset, a restored backup without it). `webui user` replaces the password and signs the account out of
every session, so doing it at every start would undo a password the player chose in the WebUI; a password the player
changes is therefore kept. If the player locks themselves out, `reset_admin_password` generates a new one (`webui password`,
falling back to `webui user` when the account is gone). Administrators created in the WebUI's own access page are never touched.

Prompts are recognised by shape (a console-thread `INFO` line ending in `:`), not wording; `PromptScript` is a small state
machine over lines with recorded-style tests. Passwords never reach the output buffer, `Debug` output or a log.

## Process

Command line (in the server folder): `java -Djava.awt.headless=true --enable-native-access=ALL-UNNAMED
-Dstdout.encoding=UTF-8 -Dstderr.encoding=UTF-8 -jar tortodrone.jar --config config/server.yml --skip-client-assets`.
`--client <game folder>` replaces the last flag when the caller wants artwork prepared from the installed game (needs a
full JDK with `javac`; unverified). The engine needs **Java 25** and, from its imports, the modules `java.sql`,
`jdk.httpserver`, `java.management` and `java.logging`; `pick_server_runtime` checks a runtime's `release` file. The
game's own trimmed runtime probably lacks them.

**Readiness.** Game: the line `Done! Listening on <addr>:<port> (TCP)`. Web: a probe of `GET /admin/api/setup` on
`127.0.0.1` (`200` means ready; `403` means the listener is up but refuses the Host, i.e. a custom `webui-public-url`).
`WebUI Ready` is not used: the engine also prints it when startup failed.

**Port conflicts.** Before a start both ports are checked (a connection to loopback, then a bind on loopback and a bind on all
interfaces one after the other; the two ports must differ; tested on macOS only, Linux and Windows unverified) and the failure names which one and suggests the next free port. The engine's own
failure lines are read as well, because a port can be taken between the check and the start: `Server startup failed: ...
Address already in use` is the game port (with the WebUI on the process then stays alive, idle, and the launcher shuts it
down), `Cannot bind shared HTTP listener` is the web port (the process exits with code 1).

**Stopping.** With the WebUI on, console `stop` only stops the game listener and the process waits for a restart, and
closing stdin does not end it. What ends it is `shutdown`, whose prompt is answered with `SHUT DOWN PROCESS`. The sequence is:
that, wait up to `shutdown_timeout` (20 s), then SIGTERM (`kill -TERM`, unix only; the engine has a shutdown hook), wait
again, then kill. `StopKind` reports which one was needed. A start that is still waiting is cancelled and killed.

**Locking.** Start, stop, restart, engine update/rollback, configure, backup, restore, reset and admin reset run under one
lifecycle lock. `status`, `log_since` and the reader threads never take it. A lifecycle call from the thread that holds the
lock (for example from a callback) fails with `Reentrant` instead of deadlocking. Inside the process module the child and
state locks are never held together, so status polling cannot deadlock with a start or stop (a test polls while cycling). `run_stopped` is the extension point for
operations that need the server stopped; backups require the `Stopped` proof it provides.

## Backups

`data/` and `config/` are copied together, only while the server is stopped (a SQLite database with WAL files copied under
a running server can be torn; the lifecycle lock and the `Stopped` token enforce this). Built in a hidden temporary folder
and renamed into place; newest 10 kept; `server.lock` and `*.tmp` are not copied; a symbolic link anywhere in the tree makes
the backup fail rather than be followed. Restore first makes a `before-restore` backup, replaces `data/` and `config/`
through a staging copy and puts the old ones back if a step fails. Restoring never prunes the backup it restores from. Reset moves `data/` into a `...-reset` backup; with
`keep_keys` the key pair is copied back so players keep the public key they configured.

## Connecting the game

After the first start the engine has written `data/keys/public.txt` (`modulus#exponent` in hex; only this file is ever read
from `data/keys/`). `register::local_server` builds the `servers::Server` entry: id `local-server`, name "Local Server",
host `127.0.0.1`, the game port, the public key and the content URL `http://127.0.0.1:<web port>/`. `register::join_info`
gives other players the host (the computer's primary address, found without sending anything; the player can override it
for a VPN), the port, the key and the content URL. It says plainly when the server only listens on this computer.

## In the app

The Servers page (`app/qml/ServersPage.qml`) has the **control strip** "Your server" above the list. Code:
`app/src/server_host.rs` (trait `ServerHost`, `RealHost` on top of `core::serverhost`), bridge `ServerHost` in
`app/src/bridge/server_host_model.rs`.

- Every call (install, start, stop, restart, back up, restore, update, ports, password) runs on a worker thread of its own;
  one task at a time (Stop may cancel a Start that is waiting). A poller reads `snapshot()` three times a second and the
  strip shows the state: checking, not installed, stopped, starting, running, stopping, failed or busy. The engine's
  SHA-256 is computed on a worker at start and after an install, never on the UI thread.
- **Install** shows the licence summary (`terms::SUMMARY`) and a checkbox; `terms::accept` is called only after it, then
  `update_engine` does the download. A second **Update engine** uses the same call. Backup, restore and update stop a running
  server and start it again afterwards.
- **Java 25+** is found with `runtime::find_runtimes` (SKradle's runtimes, `JAVA_HOME`, `PATH`; the game's own `java_vm` is
  not used). None: Start is disabled and the strip says so, with a link to get Java.
- **Ports and network access** are applied with `configure` while stopped; before a start the core checks both ports.
  A busy port is named and the next free one is offered as a button. Nothing is ever left spinning: a failed start,
  an offline download, a rate limit, a checksum mismatch and a locked data folder each have their own sentence.
- **Registration.** After a start, after a port change and at launch of the app (once a key exists) the "Local Server"
  entry is written to `servers.json` through the same writer lock the Servers page uses (`register::register`). A saved
  server already called "Local Server" is reported, not overwritten.
- **Web UI** opens from the strip only while the server runs (embedded where a web view exists, else the system browser).
  The generated administrator is shown as user plus a hidden password with Show, Copy and New password.
- **Quit.** `main` calls `server_host::shutdown()` after the event loop ends: a running engine gets the graceful stop
  sequence; an install or backup in progress is not waited for.
- Test runs (`--selftest`, `--shots`) use `FixtureHost`: fixed states, no Java, no network, no files. The real host is
  covered by unit tests (state mapping, error wording, an empty folder, refused licence and ports).
- Folder: `SKRADLE_SERVER_DIR`, else `<data dir>/SKradle/server`.

Not verified: everything with a real engine and Java 25 (no jar was available), the clipboard copy on Windows and Linux,
and the embedded view against the real WebUI.

## What the server cannot do

This is the engine's current scope, not SKradle's: it is a development release. Login, the Ready Room, Haven, the supported
interiors and many town systems work; the full opening route, general missions, guilds, PvP and Arcade do not. The terminal
console and the WebUI have no kick/ban beyond the WebUI's session kick, no whitelist and no backup command; SKradle
therefore does its own backups and offers none of those. Accounts use usernames and passwords only; Steam and guest login
are not supported.

## Unverified

- Everything about the running engine: the log lines, prompts and their exact wording are taken from its source, not from a
  run. The tests use stand-in scripts that print those lines.
- That the JVM ends after the confirmed `shutdown` and after SIGTERM (read from the engine's lifecycle code).
- That `--skip-client-assets` leaves `/admin/` fully usable with fallback icons (the engine's operations guide says so).
- That the engine starts with `webui/` replaced by the release's copy and with the web port set as above; the engine's
  Host/Origin checks with `127.0.0.1` (read from source).
- The GitHub release listing's `digest` field on the engine's real releases: the format is GitHub's documented one and the
  parser is tested on generated listings, not on a live response.
- The Windows `icacls` call and the Windows process paths (no SIGTERM step there; the kill is the fallback).
- The size caps are guesses.
