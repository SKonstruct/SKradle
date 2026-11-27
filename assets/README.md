# Assets

- `brand/`: the shared SK app icon (`icon-1024.png`, a full-bleed square) and the vector glyph (`logo.svg`).
  These are the same files the Android and iOS apps use.
- `icons/`: the desktop icons made from `brand/icon-1024.png`.
  - `skradle.icns`: macOS, built on Apple's icon template (824 of 1024 px, rounded corners, soft shadow).
  - `skradle.ico`: Windows, 16 to 256 px.
  - `skradle-<size>.png`: Linux (freedesktop `hicolor` sizes), also used by the window and tray.
  - `skradle-macos-1024.png`: the macOS artwork at full size.
- `tools/`: the icons on the Open buttons of the Tools page (64 px). `sk-ah.png`, `kozmas.png` and `gatemap.png` are the
  sites' own favicons (Gate map viewer: SirDank's). `spiralview.png`, `hammer.png` and `silverweave.png` are the app icons
  of [spiralview](https://github.com/lucasluqui/spiralview) (BSD-2-Clause, lucasluqui), the editors SKradle downloads.
