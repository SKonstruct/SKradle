# Live events

The Home page shows the game's live events from a feed that other community launchers read too. SKradle has no events
backend of its own.

- Address: `http://flamingo.knightlauncher.com:6060/server-list/?machineId=skradle-desktop` (constants in
  `core/src/net.rs`; the fixed `machineId` identifies nothing about the player).
- **Third-party, cleartext and optional.** The service has no TLS listener, so this is the only plain HTTP request
  SKradle makes. If the feed is unreachable, changed or gone, everything else works the same; the card shows the last
  saved events with an offline note, or hides.
- Reading it (`core/src/live_events.rs`): the `Official` server's events (`banner`, `content`, `link`, `startsAt`,
  `endsAt`; times in milliseconds) that have not ended, upcoming ones included; `defaultEvent*` only when none is left.
  `content` is `title|description`, `banner` is `url|dimming`. The service writes a trailing comma after the last field
  of every event; it is tolerated.

## The plain-HTTP exception

`Client::get_text_insecure_for_events` is separate from the general client, which still refuses every `http://` URL.
It accepts only `http://flamingo.knightlauncher.com:6060/server-list/` (plus a plain query) and refuses any other
host, port, path or scheme before connecting. It caps the answer at 256 KiB, times out after 4 s to connect and 8 s in
total, refuses redirects and sends no cookies or credentials.

## Untrusted data

The answer can be rewritten by anyone on the network path, so it is only used as text and links: control characters
are removed, lengths are capped (title 120, description 600, address 512 characters, 8 events), and links and images
are kept only when they are plain `https://` addresses. Learn more opens the link in the system browser. Banners are
fetched over HTTPS only, at most 1 MiB, PNG or JPEG only (checked by their header, at most 4096 px a side and 8
megapixels), cached under a file name derived from the address, and drawn by Qt only after that check.
