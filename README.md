# SKradle

A desktop launcher for Spiral Knights on Windows, macOS and Linux, with a native look on each.

Work in progress: nothing is released yet (see the [changelog](CHANGELOG.md)). Test builds for macOS, Windows and Linux
come from the **Build** workflow (Actions tab > Build > Run workflow); they are unsigned, so on macOS run
`xattr -cr SKradle.app` once and on Windows choose "More info" > "Run anyway".

The launcher logic is in [`core/`](core) (no Qt, `cargo test` builds it); the Qt Quick application is in
[`app/`](app/README.md) (build with `-p skradle-app`; needs Qt).

> Unofficial fan project. Not affiliated with Grey Havens. Spiral Knights (c) Grey Havens, LLC.

## License

[MIT](LICENSE)
