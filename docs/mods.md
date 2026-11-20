# Mods

How SKradle reads, applies and syncs Spiral Knights mods. The code is `core/src/mods/` plus `core/src/jar.rs` (safe
archives) and `core/src/digest.rs` (the updater's checksums).

## Mod formats

A mod is a zip archive. The file extension only decides how it is read.

| File | Meaning |
| --- | --- |
| `.zip`, `.jar` | One mod. The archive root is the game's `rsrc/` folder: `item/x/model.dat` replaces `rsrc/item/x/model.dat`. |
| `.modpack` | Several mods. Every `.zip` or `.jar` at the root of the archive is a mod of its own. A modpack inside a modpack is ignored. |

`.jar` mods are read exactly like `.zip` ones. No `.jar` layout different from `.zip` is known; this is unverified.

### `mod.json`

Optional, at the archive root (or, failing that, the first one inside a folder). Every field is optional:

```json
{
  "mod": {
    "name": "Acheron Trail",
    "description": "...", "author": "...", "version": "1.0", "compatibility": "1.2.2",
    "type": "class",
    "pxVersion": "20261110044614",
    "locale": { "messages.properties": { "some.key": "New text" } }
  }
}
```

- The fields may also sit at the top level instead of under `"mod"`.
- Real mods have a comma before a closing `}` or `]`, a byte order mark, or CRLF line ends; all are accepted.
- `type: "class"` makes a class mod (below). Anything else, or nothing, is a resource mod.
- `pxVersion` is the game version a class mod was built for. A class mod whose `pxVersion` differs from the installed
  version (`getdown.txt`) is skipped. `compatibility` is only kept as information.
- `locale` maps a `.properties` file name inside `rsrc/i18n/` of `code/projectx-config.jar` to the keys to set.
- A `mod.json` that is unreadable, too large (over 1 MiB) or not an object is ignored with a warning; the mod is then
  applied as a resource mod without metadata.
- `mod.json` and `mod.png` (anywhere in the archive) are never written to the game.

### What each kind changes

| Kind | Where its files go |
| --- | --- |
| Resource mod | Loose files in `rsrc/`. |
| Class mod | Merged into `code/config.jar`; a mod file replaces the jar's entry of the same name. |
| Locale changes (any mod) | Merged into the named `.properties` file inside `code/projectx-config.jar`. Only the named keys change; other lines stay byte for byte. A bundle name that is not a plain relative path, or that the game does not have, is ignored with a warning. |

Mods apply in file-name order (case-insensitive) and a later mod wins. A mod whose files are all under `rsrc/` gets a
warning: they would be written to `rsrc/rsrc/`, which the game is not known to read.

### Protected files

A mod may not replace the 52 `config/<name>.dat` files (`config/item.dat`, `config/wall.dat`, ...). These are exactly
the top-level `config/` files that the game's resource bundles contain (checked against a real install), and a game
update replaces them, so a mod's copy would go stale and then override newer data. Matching is case-insensitive. The
rest of such a mod is still applied; the apply report lists what was left out. There is no `.xml` counterpart in the
game, so none is protected.

### The mods folder

Every `.zip`, `.jar` and `.modpack` directly in the mods folder is a mod. Names starting with a dot, other files and
sub folders are ignored. A mod is enabled unless `.skradle-mods.json` in the same folder lists it:

```json
{ "disabled": ["Some Mod.zip"] }
```

A damaged state file is an error, not "everything enabled".

## Pristine state and the updater

The game's updater downloads `digest2.txt`, which lists the SHA-256 every file must have. Two facts matter:

1. A `.jar` digest is **not** the hash of the file. It is the SHA-256 over the uncompressed contents of every entry,
   in entry-name order (UTF-16 order, as Java sorts), with names, folders and zip layout left out. Rewriting a jar
   with other compression gives the same digest. All 32 unmodified jars of a real install verify this way (the 33rd had been edited by another tool); none
   verifies by file bytes. Every other file is hashed by its bytes. The last line of `digest2.txt` is the hash of the text before it.
2. The updater compares every file in the list on each run and downloads any that differ. A modified `config.jar` or
   `projectx-config.jar` therefore differs on purpose, and the updater would replace it.

So: run the updater first, apply right before starting the game, and apply again after each update. Never apply while
the game runs. Whether the updater's `.jarv` marker files let a modified jar pass unchecked was not verified.

**Pristine** means "matches `digest2.txt`". The two code jars are rebuilt from a pristine copy on every apply, never
from the previous output. The copy is, in order:

1. the jar in `code/`, if it matches its digest (a copy is then kept in `.skradle/pristine/`);
2. else the kept copy, if it still matches;
3. else a download from the game's own servers for the installed version (HTTPS, only the game's hosts, size
   capped), accepted only if it matches the digest.

With none of these the apply stops before changing anything. Loose files in `rsrc/` are restored from the game's
bundles (`rsrc/*.jar` in `digest2.txt`, read only, not re-verified on each apply): `.skradle/applied.json` records the
paths mods wrote, and the next apply puts each back from a bundle or deletes it when no bundle has it, and removes
folders that were left empty (except the game's own). `ApplyOptions::rebuild_resources` instead rebuilds every loose
file from the bundles, which clears files other tools left, at the cost of unpacking everything.

`code/config.jar` also holds per-server settings. Because apply rebuilds that jar from the original, any edit of it
(for example by the servers module) must come after apply.

## Applying and removing

```rust
let report = skradle_core::mods::apply(game_root, mods_dir)?;      // enabled mods
let report = skradle_core::mods::restore_pristine(game_root)?;     // remove all mods
```

`apply` scans every mod first, gets the pristine jars, then changes files in one transaction:

- Before a file is replaced or deleted, the original is renamed into `.skradle/txn/b/` and the intent is appended to
  `.skradle/txn/journal`. New content goes to `<file>.skradle-tmp`, is synced and renamed into place.
- Any error rolls every file back and removes the folders the apply created. After a crash the journal is still
  there, and the next apply (or `txn::recover`) rolls back first. The journal is renamed to `committed` before the
  backups are deleted, so an interrupted clean-up is never mistaken for an unfinished change.
- A mod that scans fine but turns out corrupt when its data is read is excluded and the apply starts again without it.
- Not guaranteed: after a power cut, journal lines still in the operating system's cache may be lost. The next apply
  works from the digests and corrects the files.

The report lists applied mods, skipped mods with the reason (disabled, unreadable, unsafe, wrong game version),
protected files left out, warnings, counts and what happened to each code jar.

## Archive safety

All archives and listings are untrusted. When an archive is opened (`jar::Archive`):

- at most 50,000 entries per mod, 256 MiB per entry and 1 GiB in total (declared sizes); code jars 64 MiB per entry
  and 128 MiB in total. A size that a header understates is caught while reading: an entry is read at most its
  declared size plus one byte and must be exactly that size.
- an entry name must be a safe relative path: no `..` or `.` component, no empty component, no leading `/`, no
  backslash or control character, no `:` (drive letters, alternate streams), no trailing dot or space, no Windows
  device name (`CON`, `NUL`, `COM1`, ...), at most 1024 bytes. The checks are the same on every platform.
- no symlink entries, no encrypted entries, no repeated names (for mods also none differing only in case), no entry
  that is both a file and the folder of another.
- one violation rejects the whole mod, with the reason; nothing of it is applied.

When writing: every path is checked again; no existing folder on the way may be a link; nothing is written through a
link (a link at the target is replaced, never followed); a file where a folder is needed is an error.

## Modpack sync

`mods::sync::sync_pack(mods_dir, progress)` keeps the mods folder in step with `mods/` of
`github.com/SirDank/Spiral-Knights-Modpack`.

- The listing comes from the GitHub contents API (`api.github.com`) and gives each file's git blob SHA-1
  (`sha1("blob <length>\0" + bytes)`). A local file with that hash is skipped; anything else is downloaded.
- Each download is checked against the published hash before it replaces anything; it is written beside the mod and
  renamed, so no truncated mod is ever left.
- Only `.zip`, `.jar` and `.modpack` files with a plain file name (one component, no leading dot) are taken, and only
  from `https://raw.githubusercontent.com/...` (no other host, no user info, no port, no HTTP). The HTTP client
  follows redirects; the hash check covers a redirect target.
- `.modpack-synced` lists the files a sync wrote. A sync deletes a file only if that list names it and the pack no
  longer has it. A file the player added is never deleted, and a file that merely matched the pack is not claimed. A
  player's file with the same name as a pack file is replaced by the pack's version.
- An empty or unreadable listing is an error and changes nothing. One failing file is reported and does not stop the
  others.
- GitHub's unauthenticated API allows 60 requests per hour per address; a refused request is reported as an error.

## In the app

The Mods page (`ModsPage.qml`, bridge `ModsModel`, logic in `app/src/mods_host.rs`) manages the folder
`<data dir>/SKradle/mods` (`SKRADLE_MODS_DIR` overrides it for tests); apply still happens at launch.

- **List:** every mod with `mod.json` name, version, author and description, file name, container and size. A mod
  that does not scan is shown with its error. The switch writes `.skradle-mods.json` (`list::set_enabled`).
- **Tags** come from the scan only (`mods::tags`): modpack, code (class mod), textures (images), audio, text (locale
  changes), game files (other loose files), problem (unreadable). The filter chips list only tags that exist, with counts.
- **Adding** (drop anywhere on the page, or a file dialog): each file goes through `mods::import`: a regular file
  (no folder, no link) with a mod extension and a visible name, at most 1 GiB, that scans and holds files. It is
  copied to a hidden temporary name, scanned again, then linked into place; a taken name gets ` (2)`, never an
  overwrite. Refusals are listed per file with the reason, plus one summary toast.
- **Selection and keys:** click, Ctrl/Cmd+click, Shift+click; Ctrl/Cmd+A, Esc; Up/Down, Shift+Up/Down, Home/End;
  Space switches the selected (else focused) mods, Delete removes after a confirmation. The list is one tab stop.
- **Remove** deletes the file and its switch entry. A file the modpack sync wrote returns at the next sync while the pack still lists it.
- **Modpack:** `sync::status` (one listing request) decides "install", "update (N files)" or "up to date"; the button
  runs `sync::sync_cancellable` on a worker thread with file n of total, then shows the report (downloaded, current,
  removed, failed with reasons). Cancel stops before the next file (the running download completes), deletes nothing
  and keeps earlier claims. No network gives an error state with a retry button.
- All actions are off while no game is found.

## Limits

- Behaviour inside the game (whether the game loads what apply writes, including the `rsrc/rsrc/` case and `.jar`
  mods) is not verified here; the tests use synthetic game folders.
- The updater's handling of `.jarv` markers is not verified.
- Apply does not check that the game is not running.
