# The game's editors

Settings > Tools starts these:

| Tool | Comes from |
| --- | --- |
| Crucible scene editor | the game's own files |
| Scene editor | the game's own files (the class name is obfuscated and may change with a game update) |
| Silverweave | the editors pack |
| Model Viewer | the editors pack |
| Particle Editor | the editors pack |
| Interface Tester | the editors pack |

## The editors pack

The last four are not in the game's jars (only their texts are). They are open source: [Clyde](https://github.com/threerings/clyde)
(the Three Rings library the game is built on) and [spiralview](https://github.com/lucasluqui/spiralview) (a wrapper
around Clyde's editors that also holds Silverweave), both BSD-2-Clause.

- The **Editors** workflow (`.github/workflows/editors.yml`, `packaging/editors/build.sh`) runs every day. It resolves the
  newest spiralview commit, the newest released Clyde (Maven Central) and the game's current code jar, and rebuilds only
  when one of them changed. The result is the rolling `editors` release (a pre-release, so the launcher's own update
  check ignores it): `skradle-editors.jar`, `editors.json` (build id, SHA-256, size, minimum Java) and `NOTICES.txt`.
- spiralview is compiled against the game's `projectx-pcode.jar`, which is proprietary. The build downloads it from the
  game's official host for the compile only, leaves it out of the jar, and fails if any class of the game's jar ended up
  in the output (a class identical to one in the game is allowed only when an open-source dependency carries the same
  bytes).
- In the app, Settings > Tools has one row for the pack. **Download** fetches it once (about 25 MB, checked against the
  manifest's SHA-256); from then on SKradle checks every half hour at most, when Settings opens, and updates an
  installed pack by itself. It lives in the data folder, `SKradle/editors/`.
- The editors need **Java 25 or newer** (spiralview is built for it, and Silverweave starts with Java 25's simple `main`).
  They run with the pack's jar in front of the game's own class path, in the game folder, without the graphics agent.
- Clyde's own newest source (1.2.2-SNAPSHOT) cannot be built alone: it needs unreleased snapshots of four sibling
  libraries. So "newest Clyde" means the newest released one.

Run the workflow by hand with **Actions > Editors > Run workflow** (tick "force" to publish again without a change).
`packaging/editors/build.sh build <work> <out>` does the same on a computer with Java 25, Maven and Python.
