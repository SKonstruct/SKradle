# Crash reports

SKradle can report its own crashes to Sentry so they can be fixed. It is on by default and can be turned off.

## What is sent

When SKradle itself panics (a bug in the launcher's Rust code), one report is sent: the panic message, the backtrace,
the source location, the launcher version (`skradle@<version>`), the operating system name and the CPU architecture.

Nothing else is sent: no logs, no settings, no game files, no account or server details, no mod names.

Before anything is built, the text is scrubbed: home folders become `~`, user names become `<user>`, IPv4 addresses,
e-mail addresses and long keys or hashes are replaced. File and module paths in the backtrace are kept so the report
is useful. Sentry itself also sees the connection's IP address, so the project should have IP storage turned off (below).

SKradle's own log (`docs/logging.md`) stays on the machine and is never sent automatically; a panic is written to it
whether or not crash reports are on.

Not covered: crashes inside Qt's native code, and crashes of the game itself.

## Turning it off

In the app: Settings, "Send crash reports" (the `crashReports` key in `settings.json`). Builds without a Sentry DSN
never send anything, and neither do the self-test and screenshot runs.

## Setting up the Sentry project (maintainers)

1. In Sentry, create a project: platform **Rust**, name `skradle`.
2. Project Settings, Client Keys (DSN): copy the DSN. It looks like `https://<key>@<host>/<project id>`.
3. Project Settings, Security & Privacy: turn on **Prevent Storing of IP Addresses**; keep **Data Scrubber** and
   **Use Default Scrubbers** on.
4. Client Keys, Configure: set a **rate limit** (for example 30 events per hour). The DSN is compiled into a public
   program, so anyone can read it; a rate limit keeps spam cheap to ignore.
5. Add the DSN as a GitHub Actions secret named `SKRADLE_SENTRY_DSN` so release builds compile it in.
6. For a local build with reporting: `SKRADLE_SENTRY_DSN=https://... cargo build -p skradle-app --release`.

To check that the project accepts events, send one test event:

```
SKRADLE_SENTRY_TEST_DSN=https://<key>@<host>/<project id> \
  cargo test -p skradle-core --test live sends_a_test_event_to_sentry -- --ignored --nocapture
```

The event titled "SKradle test event: safe to delete" should appear in the project's Issues within a minute.
