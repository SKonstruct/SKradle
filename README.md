# SKradle

A fast, good-looking desktop launcher for [Spiral Knights](https://www.spiralknights.com/) on Windows, macOS and Linux.
It finds your Steam or standalone copy, applies your mods, picks the server and starts the game. It can also host a
server of your own.

![Home: launch card, players online, live events and news](docs/images/home-dark.png)

> Work in progress: nothing is released yet (see the [changelog](CHANGELOG.md)). Screenshots show sample data.

## What it does

- **Launch**: finds a Steam or standalone install (or add a folder), signs in through Steam automatically, starts the
  game directly on a suitable Java, and shows live events, news and an estimate of the players online on Home.
- **Servers**: a list of servers with live status and ping, add your own, pick the one the game uses. Install, start,
  stop, back up and update a server on your own computer, with its web interface and a console inside the window.
- **Mods**: your mods folder with an on/off switch each, pictures, tags, filters and search; drag and drop to add,
  select several at once, and install or update the maintained modpack in one click.
- **Tools**: the game's editors (Crucible, scene editor, Silverweave, Model Viewer, Particle Editor, Interface Tester;
  kept up to date) and two Spiral Knights websites, the Auction House and Kozmas, that open inside the launcher and keep
  your sign-in.
- **Settings**: memory, garbage collector and Java, file cleaning and reset, optional graphics enhancements (render
  scale, sharpening, frame generation, FPS counter), Discord Rich Presence, light and dark
  theme, reduced motion, and automatic updates of SKradle itself.

<table>
  <tr>
    <td><img src="docs/images/servers.png" alt="Servers"></td>
    <td><img src="docs/images/mods.png" alt="Mods"></td>
  </tr>
  <tr>
    <td><img src="docs/images/tools.png" alt="Tools"></td>
    <td><img src="docs/images/settings.png" alt="Settings"></td>
  </tr>
</table>

## Download

Test builds come from the **CI** workflow: open a run on the Actions tab (any push to main, or Run workflow) and download the
package for your system under Artifacts. Each download is one zip holding one
folder with the package and a `SHA256SUMS.txt`: a `.dmg` for macOS (Apple silicon and Intel), the app folder for
Windows (x64 and ARM64; run `skradle.exe`) and an AppImage for Linux x64. They are unsigned: on macOS run
`xattr -cr SKradle.app` once, on Windows choose "More info" > "Run anyway".

Something wrong? [Open an issue](../../issues/new/choose); the form asks for the log (Settings > About > Open log
folder).

## Build from source

The launcher logic is in [`core/`](core) (no Qt; `cargo test` builds it). The Qt Quick application is in
[`app/`](app/README.md) (`cargo build -p skradle-app --release`, needs Qt 6.8 or newer and Rust 1.85 or newer; the app
README has the setup, flags and debugging).

More: [launching](docs/launching.md) · [mods](docs/mods.md) · [hosting a server](docs/server-host.md) ·
[editors](docs/editors.md) · [websites](docs/web-tools.md) · [live events](docs/live-events.md) · [logging](docs/logging.md) · [crash reports](docs/crash-reports.md) ·
[design system](docs/design-system.md)

> Unofficial fan project. Not affiliated with Grey Havens. Spiral Knights (c) Grey Havens, LLC.

## License

[MIT](LICENSE)
