# Logging

SKradle keeps one log, on the machine only. It is never sent anywhere automatically (see `crash-reports.md`).

## Where

`<data dir>/SKradle/logs/skradle.log`: `%APPDATA%\SKradle\logs` on Windows (`dirs::data_dir`), `~/Library/Application
Support/SKradle/logs` on macOS, `~/.local/share/SKradle/logs` on Linux. `SKRADLE_LOG_DIR` overrides the folder;
`SKRADLE_LOG=debug|info|warn|error` the level (default `info`). Self-test and screenshot runs do not write to it.

## Format and rotation

```
2026-11-22T14:03:09.123Z INFO  [launch] starting the game (direct): program ~/.../java, 41 arguments, 2 variables set (SteamAppId, SteamGameId), working folder ...
```

UTC ISO 8601, level (`ERROR`, `WARN`, `INFO`, `DEBUG`), `[target]`, message; a multi-line message continues on lines
indented by four spaces. Each start begins a new `skradle.log` and the previous run becomes `skradle.1.log`
(`skradle.2.log` before that; three files at most). A file that reaches 2 MiB rotates the same way. Writes are one small
append under a lock; they never panic and never report an error (a log that cannot be written drops lines).

A session starts with a header: SKradle version, OS and architecture, Qt version, install kind, Java found.

## What is logged

Launch steps (inputs, decisions, refusals, the Steam state and wait, the names of the environment variables set, never
their values, the program path and argument count, exit codes, the end of the game's own `launcher.log` on an early
failure), the background update check, server host events, mod operations, network failures (host and status only),
settings load problems, panics (before the crash reporter runs) and Qt's own warnings (`[qt]`, from the C++ message
handler).

## Scrubbing

Applied to every entry before it is written: the home folder becomes `~`; `password`, `token`, `secret`, `session key`,
`authorization`, `api key`, `cookie` and similar `name=value` pairs lose the value; a URL loses its query string and any
`user:password@`; long key-like strings (40 or more characters) are replaced.

## Code

`core/src/log.rs` (writer, rotation, scrubbing, `tail_of`, panic hook), `app/src/logging.rs` (start, session header),
`app/src/bridge/logs_model.rs` (the `Logs` singleton and the Qt message entry point `skradle_log_qt`).

`Logs` for QML: `folder` (string, constant), `openFolder()` (system file manager), `copyPath()` (log file path to the
clipboard), `tail(n)` (the last n lines, newline-joined).
