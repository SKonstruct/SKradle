# Translating SKradle

SKradle's text lives in Qt catalogues, one per language, in [`app/i18n/`](../app/i18n):

| File | Language |
| --- | --- |
| `skradle_en.ts` | English (the source text, nothing to translate) |
| `skradle_ar.ts` | Arabic (right to left: the layouts mirror) |
| `skradle_de.ts`, `_es`, `_et`, `_fr`, `_it`, `_pl`, `_ru` | German, Spanish, Estonian, French, Italian, Polish, Russian |
| `skradle_ja.ts` | Japanese |
| `skradle_pt_BR.ts` | Brazilian Portuguese |
| `skradle_zh_CN.ts`, `skradle_zh_TW.ts` | Simplified and Traditional Chinese |

Every catalogue has every message. A message with no translation yet is `type="unfinished"` and the app shows the English
text for it, so a language can be released while it is still being filled in.

## The workflow

1. **Refresh** after the interface text changed: `app/i18n/update.sh` (needs Qt's `lupdate`). New text appears in every
   catalogue as unfinished; text that is gone disappears; finished translations stay.
2. **Translate** the unfinished messages of one catalogue (by hand, or with a model: see the prompt below). Fill in
   `<translation>` and remove `type="unfinished"` from the ones you finished. Plural messages have one
   `<numerusform>` per plural form of the language.
3. **Check**: `cargo test -p skradle-app translations` fails when a finished translation loses or adds a `%1`, is empty,
   contains markup, or when a catalogue is out of step with the English one. CI runs the same test.
4. **Review** the diff of the catalogue (see the checklist below), then look at the language in the app: Settings >
   General > Language, and `skradle --shots <folder>` for screenshots to look for cut-off text.

## Rules for the text

- Keep `%1`, `%2`, ... exactly (they are replaced by values); `%n` in a plural message is the number.
- Keep the English product and game names as they are: **SKradle**, **Spiral Knights**, **Steam**, **Java**, **Discord**,
  **Tortodrone**, **Silverweave**. Game terms follow the game's own translation when it has one (Crystal Energy, Mist
  Energy, Clockworks, Haven, Heat, Arsenal); when it has none, keep the English term.
- Short strings are buttons and tabs: translate to the shortest natural form. Long strings are explanations: translate
  to plain, friendly sentences, not word for word.
- No HTML and no template braces. Quotation marks, ellipses and spacing follow the language's own rules.

## Prompt for a model

> You translate the user interface of a game launcher from English to **<language>**. The input is a Qt Linguist `.ts`
> file. Fill in the `<translation>` of every message that has `type="unfinished"` and remove that attribute from the ones
> you finish. Change nothing else: keep every `<source>`, the file structure and the order. Keep `%1`, `%2`, `%n`
> exactly as they are, and use one `<numerusform>` per plural form of the language. Do not translate: SKradle, Spiral
> Knights, Steam, Java, Discord, Tortodrone, Silverweave. Keep buttons short. Write natural, friendly UI text, not word
> for word. Return only the complete file.

Give it one catalogue at a time (about 700 messages is fine for a long-context model; split it in two if the answer is cut
off). The English text is also the best context: `app/qml/` shows where each string appears.

## Review checklist

- `cargo test -p skradle-app translations` passes.
- Placeholders and names are intact; no message is a different length by an order of magnitude (a one-word button that
  became a sentence will be cut off).
- Consistent words for the same thing across the file (Mods, Servers, Settings, Launch, Tools).
- For Arabic: the interface mirrors (sidebar on the right) and numbers and Latin names inside sentences still read well.
- For Japanese and Chinese: no spaces added between words, full-width punctuation where the language uses it.

## Text that comes from Rust

Launch and mod error sentences, the folder check and the settings-file warning are written in Rust with `tr("...")` /
`tr_args("... %1 ...", &[value])` (`app/src/i18n.rs`) and live in the catalogues under the context `rust`.
`update.sh` collects them (`i18n/rust_strings.py` writes `i18n/rust_strings.cpp`; CI fails when it is stale), so they
are translated like every other message. Keep `%1`, `%2` in place; a translation may reorder them.

## What is not translated

Text from the game's own updater, the engines' raw error details shown after a colon, and the log. The live events and
the news come from their sources in their own language. The mod, server and file names are the player's own.
