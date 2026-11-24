//! Checks the translation catalogues (`i18n/skradle_*.ts`, Qt Linguist XML) for the mistakes a machine translation
//! makes, so a bad file fails CI instead of breaking a screen:
//!
//! * a finished translation keeps every `%1`, `%2`, ... of its source text (and adds none);
//! * no translation of a finished message is empty;
//! * the catalogues and the English one have the same messages (run `i18n/update.sh` after changing strings);
//! * a translation does not contain the markup characters of a different format (stray `<` or `{{`).
//!
//! Unfinished messages (empty, `type="unfinished"`) are fine: the UI shows English for them.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

struct Message {
    context: String,
    source: String,
    /// Every text form of the translation (one, or one per plural form); empty when there is none.
    forms: Vec<String>,
    unfinished: bool,
}

fn i18n_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("i18n")
}

fn unescape(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// The text between `<tag ...>` and `</tag>` in `block`, if the tag is there.
fn between<'a>(block: &'a str, tag: &str) -> Option<&'a str> {
    let open = block.find(&format!("<{tag}"))?;
    let start = block[open..].find('>')? + open + 1;
    let end = block[start..].find(&format!("</{tag}>"))? + start;
    Some(&block[start..end])
}

fn parse(text: &str) -> Vec<Message> {
    let mut out = Vec::new();
    for context in text.split("<context>").skip(1) {
        let name = between(context, "name").map(unescape).unwrap_or_default();
        for block in context.split("<message").skip(1) {
            let block = block.split("</message>").next().unwrap_or_default();
            let source = between(block, "source").map(unescape).unwrap_or_default();
            let translation_tag = block
                .find("<translation")
                .map(|i| &block[i..block[i..].find('>').map_or(block.len(), |j| i + j)]);
            let unfinished = translation_tag.is_some_and(|t| t.contains("unfinished"));
            let body = between(block, "translation").unwrap_or_default();
            let forms: Vec<String> = if body.contains("<numerusform") {
                body.split("<numerusform")
                    .skip(1)
                    .filter_map(|f| {
                        let from = f.find('>')? + 1;
                        let to = f.find("</numerusform>")?;
                        Some(unescape(&f[from..to]))
                    })
                    .collect()
            } else if body.trim().is_empty() {
                Vec::new()
            } else {
                vec![unescape(body)]
            };
            out.push(Message {
                context: name.clone(),
                source,
                forms,
                unfinished,
            });
        }
    }
    out
}

/// `%1`, `%2` ... in `text` (not `%n`, which plural forms may leave out).
fn placeholders(text: &str) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '%' {
            let mut j = i + 1;
            while j < chars.len() && chars[j].is_ascii_digit() {
                j += 1;
            }
            if j > i + 1 {
                found.insert(chars[i..j].iter().collect());
            }
            i = j;
        } else {
            i += 1;
        }
    }
    found
}

fn catalogues() -> Vec<(String, Vec<Message>)> {
    let mut files: Vec<_> = fs::read_dir(i18n_dir())
        .expect("the i18n folder")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "ts"))
        .collect();
    files.sort();
    files
        .into_iter()
        .map(|p| {
            let text = fs::read_to_string(&p).expect("a readable catalogue");
            (
                p.file_name().unwrap().to_string_lossy().into_owned(),
                parse(&text),
            )
        })
        .collect()
}

#[test]
fn translations_keep_the_placeholders_of_their_source() {
    let mut problems = Vec::new();
    for (file, messages) in catalogues() {
        for m in &messages {
            if m.unfinished {
                continue;
            }
            let wanted = placeholders(&m.source);
            for form in &m.forms {
                if form.trim().is_empty() {
                    problems.push(format!("{file}: empty translation of \"{}\"", m.source));
                    continue;
                }
                let got = placeholders(form);
                if got != wanted {
                    problems.push(format!(
                        "{file} [{}]: \"{}\" has {:?}, the translation \"{}\" has {:?}",
                        m.context, m.source, wanted, form, got
                    ));
                }
            }
        }
    }
    assert!(
        problems.is_empty(),
        "{} problem(s):\n{}",
        problems.len(),
        problems.join("\n")
    );
}

#[test]
fn every_catalogue_has_the_messages_of_the_english_one() {
    let all = catalogues();
    let english = all
        .iter()
        .find(|(f, _)| f == "skradle_en.ts")
        .expect("the English catalogue");
    let want: BTreeSet<(String, String)> = english
        .1
        .iter()
        .map(|m| (m.context.clone(), m.source.clone()))
        .collect();
    assert!(!want.is_empty(), "the English catalogue has no messages");
    for (file, messages) in &all {
        let have: BTreeSet<(String, String)> = messages
            .iter()
            .map(|m| (m.context.clone(), m.source.clone()))
            .collect();
        let missing: Vec<_> = want.difference(&have).take(5).collect();
        let extra: Vec<_> = have.difference(&want).take(5).collect();
        assert!(
            missing.is_empty() && extra.is_empty(),
            "{file} is out of step with skradle_en.ts (run app/i18n/update.sh): missing {missing:?}, extra {extra:?}"
        );
    }
}

#[test]
fn translations_do_not_carry_markup_of_another_format() {
    for (file, messages) in catalogues() {
        for m in messages.iter().filter(|m| !m.unfinished) {
            for form in &m.forms {
                // Qt messages here are plain text: no HTML and no template braces.
                assert!(
                    !form.contains("{{") && !form.contains("}}") && !form.contains("</"),
                    "{file}: markup in the translation \"{form}\" of \"{}\"",
                    m.source
                );
            }
        }
    }
}

#[test]
fn the_parser_reads_plain_and_plural_messages() {
    let xml = r#"<context><name>Page</name>
        <message><source>Hello %1</source><translation>Hallo %1</translation></message>
        <message numerus="yes"><source>%n mod(s)</source><translation type="unfinished"><numerusform></numerusform></translation></message>
        <message numerus="yes"><source>%n mod(s)</source><translation><numerusform>Ein Mod</numerusform><numerusform>%n Mods</numerusform></translation></message>
        </context>"#;
    let messages = parse(xml);
    assert_eq!(messages.len(), 3);
    assert_eq!(messages[0].forms, ["Hallo %1"]);
    assert!(messages[1].unfinished);
    assert_eq!(messages[2].forms, ["Ein Mod", "%n Mods"]);
    assert_eq!(
        placeholders("%1 of %2 and 100%"),
        BTreeSet::from(["%1".to_owned(), "%2".to_owned()])
    );
}
