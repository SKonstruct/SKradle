//! A mod's `mod.json` and the locale (`.properties`) edits it can ask for.
//!
//! `mod.json` is written by hand by mod authors, so the reader is forgiving about what real mods do (a UTF-8 byte
//! order mark, a comma before a closing `}` or `]`) and strict about everything that matters for safety: the file is
//! size-capped, parsed by a depth-limited parser, only strings and numbers are read from it, and a bundle name is
//! checked like an archive path before it names a file.
//!
//! Layout (from the mods in use): `{"mod": {"name": ..., "description": ..., "author": ..., "version": ...,
//! "compatibility": ..., "type": "class", "pxVersion": ..., "locale": {"<bundle>.properties": {"<key>": "<text>"}}}}`;
//! the fields may also sit at the top level. Every field is optional. `type` is `class` for a class mod and anything
//! else (or nothing) for a resource mod. `pxVersion` is the game version a class mod was built for. `locale` maps a
//! bundle name inside `rsrc/i18n/` of `code/projectx-config.jar` to the properties it overrides.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use serde_json::Value;

use crate::jar::safe_entry_path;

/// Longest `mod.json` read.
pub const MAX_MOD_JSON_BYTES: u64 = 1024 * 1024;

/// The metadata of one mod (or of one mod inside a modpack).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ModInfo {
    pub name: Option<String>,
    pub description: Option<String>,
    pub author: Option<String>,
    pub version: Option<String>,
    /// Read from `compatibility`: the mod tool's own version note. Not compared with anything.
    pub compatibility: Option<String>,
    /// Read from `type`.
    pub mod_type: Option<String>,
    /// Read from `pxVersion`.
    pub px_version: Option<String>,
    /// Bundle name to (key to text).
    pub locale: BTreeMap<String, BTreeMap<String, String>>,
}

impl ModInfo {
    pub fn is_class(&self) -> bool {
        self.mod_type
            .as_deref()
            .is_some_and(|t| t.trim().eq_ignore_ascii_case("class"))
    }

    pub fn locale_changes(&self) -> usize {
        self.locale.values().map(BTreeMap::len).sum()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MetaError {
    TooLarge,
    /// Not JSON, even after the tolerated deviations.
    Json(String),
    /// JSON, but not an object.
    NotAnObject,
}

impl std::fmt::Display for MetaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MetaError::TooLarge => f.write_str("mod.json is too large"),
            MetaError::Json(why) => write!(f, "mod.json is not valid JSON: {why}"),
            MetaError::NotAnObject => f.write_str("mod.json is not a JSON object"),
        }
    }
}

impl std::error::Error for MetaError {}

/// Drops a comma that directly precedes a closing `}` or `]`, outside strings.
pub fn strip_trailing_commas(json: &str) -> String {
    let chars: Vec<char> = json.chars().collect();
    let mut out = String::with_capacity(json.len());
    let (mut in_string, mut escaped) = (false, false);
    for (i, &c) in chars.iter().enumerate() {
        if in_string {
            out.push(c);
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
            }
        } else if c == '"' {
            in_string = true;
            out.push(c);
        } else if c == ',' {
            let next = chars[i + 1..].iter().find(|c| !c.is_whitespace());
            if !matches!(next, Some('}') | Some(']')) {
                out.push(c);
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn text_of(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

/// Parses `mod.json`. Returns the metadata plus notes about parts that were ignored (a locale entry that is not
/// text, for example). Never panics, whatever the bytes.
pub fn parse_mod_json(bytes: &[u8]) -> Result<(ModInfo, Vec<String>), MetaError> {
    if bytes.len() as u64 > MAX_MOD_JSON_BYTES {
        return Err(MetaError::TooLarge);
    }
    let text = String::from_utf8_lossy(bytes);
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    let value: Value = serde_json::from_str(&strip_trailing_commas(text))
        .map_err(|e| MetaError::Json(e.to_string()))?;
    let root = value.as_object().ok_or(MetaError::NotAnObject)?;
    let obj = root.get("mod").and_then(Value::as_object).unwrap_or(root);
    let mut warnings = Vec::new();
    let mut info = ModInfo {
        name: text_of(obj.get("name")).filter(|s| !s.trim().is_empty()),
        description: text_of(obj.get("description")),
        author: text_of(obj.get("author")),
        version: text_of(obj.get("version")),
        compatibility: text_of(obj.get("compatibility")),
        mod_type: text_of(obj.get("type")),
        px_version: text_of(obj.get("pxVersion")).filter(|s| !s.trim().is_empty()),
        locale: BTreeMap::new(),
    };
    match obj.get("locale") {
        None | Some(Value::Null) => {}
        Some(Value::Object(bundles)) => {
            for (bundle, changes) in bundles {
                let Some(changes) = changes.as_object() else {
                    warnings.push(format!(
                        "locale bundle {bundle:?} is not an object, ignored"
                    ));
                    continue;
                };
                let mut map = BTreeMap::new();
                for (key, value) in changes {
                    match value {
                        Value::String(s) => {
                            map.insert(key.clone(), s.clone());
                        }
                        _ => warnings.push(format!(
                            "locale entry {bundle:?}/{key:?} is not text, ignored"
                        )),
                    }
                }
                if !map.is_empty() {
                    info.locale.insert(bundle.clone(), map);
                }
            }
        }
        Some(_) => warnings.push("locale is not an object, ignored".to_owned()),
    }
    Ok((info, warnings))
}

/// The path inside `code/projectx-config.jar` of a locale bundle named in `mod.json`, or `None` if the name is not a
/// plain relative path.
pub fn locale_jar_path(bundle: &str) -> Option<String> {
    match safe_entry_path(bundle) {
        Ok((path, false)) => Some(format!("rsrc/i18n/{path}")),
        _ => None,
    }
}

/// Sets `changes` in the text of a `.properties` file and leaves every other line byte for byte as it was. A key that
/// is defined already is replaced where it stands (every definition of it); a new key is appended. Non-ASCII text in
/// the new lines is written as `\uXXXX`, which every reader of the format understands. The file is treated as bytes
/// (ISO-8859-1), so untouched lines survive whatever encoding they use.
pub fn set_properties(existing: &[u8], changes: &BTreeMap<String, String>) -> Vec<u8> {
    let text: String = existing.iter().map(|&b| b as char).collect();
    let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let mut pending: BTreeMap<&str, &str> = changes
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let mut out = String::with_capacity(text.len() + 64);
    let mut group = String::new();
    let flush = |group: &mut String, out: &mut String, pending: &mut BTreeMap<&str, &str>| {
        if group.is_empty() {
            return;
        }
        match logical_key(group).and_then(|k| changes.get(&k).map(|v| (k, v))) {
            Some((key, value)) => {
                let _ = write!(
                    out,
                    "{}={}{newline}",
                    escape(&key, true),
                    escape(value, false)
                );
                pending.remove(key.as_str());
            }
            None => out.push_str(group),
        }
        group.clear();
    };
    for line in text.split_inclusive('\n') {
        // A comment or blank line is never continued, whatever it ends with.
        let alone = group.is_empty() && {
            let first = line.trim_start_matches([' ', '\t', '\u{c}']);
            first.starts_with(['#', '!']) || first.trim().is_empty()
        };
        group.push_str(line);
        let body = line.trim_end_matches(['\n', '\r']);
        let slashes = body.chars().rev().take_while(|&c| c == '\\').count();
        if alone || slashes % 2 == 0 {
            flush(&mut group, &mut out, &mut pending);
        }
    }
    flush(&mut group, &mut out, &mut pending);
    if !pending.is_empty() && !out.is_empty() && !out.ends_with('\n') {
        out.push_str(newline);
    }
    for (key, value) in pending {
        let _ = write!(
            out,
            "{}={}{newline}",
            escape(key, true),
            escape(value, false)
        );
    }
    out.chars().map(|c| c as u32 as u8).collect()
}

/// The key a logical properties line defines, or `None` for blank and comment lines.
fn logical_key(group: &str) -> Option<String> {
    let first = group.trim_start_matches([' ', '\t', '\u{c}']);
    if first.starts_with(['#', '!']) || first.trim().is_empty() {
        return None;
    }
    let mut key = String::new();
    let mut chars = first.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.next() {
                Some('u') => {
                    let hex: String = chars.by_ref().take(4).collect();
                    match u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                        Some(ch) => key.push(ch),
                        None => key.push_str(&format!("\\u{hex}")),
                    }
                }
                Some('t') => key.push('\t'),
                Some('n') => key.push('\n'),
                Some('r') => key.push('\r'),
                Some('f') => key.push('\u{c}'),
                Some('\r') | Some('\n') | None => break,
                Some(other) => key.push(other),
            },
            '=' | ':' | ' ' | '\t' | '\u{c}' | '\r' | '\n' => break,
            other => key.push(other),
        }
    }
    Some(key)
}

fn escape(s: &str, is_key: bool) -> String {
    let mut out = String::with_capacity(s.len());
    for (i, c) in s.chars().enumerate() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\u{c}' => out.push_str("\\f"),
            ' ' if is_key || i == 0 => out.push_str("\\ "),
            '=' | ':' | '#' | '!' if is_key => {
                out.push('\\');
                out.push(c);
            }
            c if (' '..='~').contains(&c) => out.push(c),
            c => {
                let mut units = [0u16; 2];
                for unit in c.encode_utf16(&mut units) {
                    let _ = write!(out, "\\u{unit:04x}");
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(json: &str) -> ModInfo {
        parse_mod_json(json.as_bytes()).unwrap().0
    }

    #[test]
    fn reads_the_nested_layout() {
        let info = parse(
            r#"{"mod": {"name": "Kurikara", "description": "d", "author": "a", "version": "0.0.0",
                "compatibility": "1.2.2"}}"#,
        );
        assert_eq!(info.name.as_deref(), Some("Kurikara"));
        assert_eq!(info.compatibility.as_deref(), Some("1.2.2"));
        assert!(!info.is_class());
        assert!(info.locale.is_empty());
    }

    #[test]
    fn reads_the_flat_layout_and_a_class_mod() {
        let info = parse(r#"{"name": "X", "type": " Class ", "pxVersion": "20261110044614"}"#);
        assert!(info.is_class());
        assert_eq!(info.px_version.as_deref(), Some("20261110044614"));
    }

    #[test]
    fn tolerates_trailing_commas_bom_and_crlf() {
        let json = "\u{feff}{\r\n  \"mod\": {\r\n    \"name\": \"A\",\r\n    \"version\": \"1.0\",\r\n  },\r\n}";
        let info = parse(json);
        assert_eq!(info.name.as_deref(), Some("A"));
        assert_eq!(info.version.as_deref(), Some("1.0"));
    }

    #[test]
    fn commas_inside_strings_and_between_values_are_kept() {
        let json = r#"{"a": "x, }", "b": [1, 2,], "c": "q\", ]"}"#;
        assert_eq!(
            strip_trailing_commas(json),
            r#"{"a": "x, }", "b": [1, 2], "c": "q\", ]"}"#
        );
        let info = parse(r#"{"name": "a, }", "version": 2}"#);
        assert_eq!(info.name.as_deref(), Some("a, }"));
        assert_eq!(info.version.as_deref(), Some("2"));
    }

    #[test]
    fn reads_locale_changes_and_skips_what_is_not_text() {
        let (info, warnings) = parse_mod_json(
            br#"{"mod": {"locale": {"a.properties": {"k1": "v1", "k2": 5}, "b.properties": [], "c.properties": {"k": "v"}}}}"#,
        )
        .unwrap();
        assert_eq!(info.locale_changes(), 2);
        assert_eq!(info.locale["a.properties"]["k1"], "v1");
        assert_eq!(info.locale["c.properties"]["k"], "v");
        assert_eq!(warnings.len(), 2, "{warnings:?}");
        let (info, warnings) = parse_mod_json(br#"{"locale": 3}"#).unwrap();
        assert!(info.locale.is_empty());
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn garbage_is_an_error_never_a_panic() {
        for bad in [
            &b""[..],
            b"not json",
            b"{",
            b"[1,2]",
            b"\"str\"",
            b"null",
            b"\xff\xfe\x00{",
            b"{\"mod\": {\"name\": }}",
        ] {
            assert!(parse_mod_json(bad).is_err(), "{bad:?}");
        }
        let deep = format!("{}{}", "[".repeat(100_000), "]".repeat(100_000));
        assert!(parse_mod_json(deep.as_bytes()).is_err());
        let huge = vec![b' '; MAX_MOD_JSON_BYTES as usize + 1];
        assert_eq!(parse_mod_json(&huge), Err(MetaError::TooLarge));
    }

    #[test]
    fn wrong_types_in_known_fields_are_ignored() {
        let info =
            parse(r#"{"mod": {"name": ["x"], "author": {"a": 1}, "type": null, "version": 1.5}}"#);
        assert_eq!(info.name, None);
        assert_eq!(info.author, None);
        assert_eq!(info.version.as_deref(), Some("1.5"));
        assert!(!info.is_class());
    }

    #[test]
    fn locale_bundle_names_are_confined() {
        assert_eq!(
            locale_jar_path("messages.properties").as_deref(),
            Some("rsrc/i18n/messages.properties")
        );
        assert_eq!(
            locale_jar_path("sub/a.properties").as_deref(),
            Some("rsrc/i18n/sub/a.properties")
        );
        for bad in ["../../x", "/etc/x", "a\\b", "", "dir/", "C:x", "a/../b"] {
            assert_eq!(locale_jar_path(bad), None, "{bad:?}");
        }
    }

    fn changes(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn set(existing: &str, pairs: &[(&str, &str)]) -> String {
        let out = set_properties(existing.as_bytes(), &changes(pairs));
        out.iter().map(|&b| b as char).collect()
    }

    #[test]
    fn replaces_a_key_where_it_stands_and_keeps_other_lines() {
        let existing = "# header\n\nm.a = old\nm.b=keep\n! note\nm.c:also keep\n";
        let out = set(existing, &[("m.a", "new")]);
        assert_eq!(
            out,
            "# header\n\nm.a=new\nm.b=keep\n! note\nm.c:also keep\n"
        );
    }

    #[test]
    fn appends_missing_keys_and_adds_a_final_newline() {
        assert_eq!(set("a=1", &[("b", "2")]), "a=1\nb=2\n");
        assert_eq!(set("", &[("b", "2")]), "b=2\n");
        assert_eq!(set("a=1\r\n", &[("b", "2")]), "a=1\r\nb=2\r\n");
    }

    #[test]
    fn handles_continuations_and_every_definition_of_a_key() {
        let existing = "k = one \\\n    two\nother=1\nk=again\n";
        let out = set(existing, &[("k", "new")]);
        assert_eq!(out, "k=new\nother=1\nk=new\n");
        // An escaped backslash at the end of a line does not continue it.
        assert_eq!(set("a=x\\\\\nb=2\n", &[("b", "3")]), "a=x\\\\\nb=3\n");
    }

    #[test]
    fn a_comment_ending_in_a_backslash_does_not_swallow_the_next_property() {
        // Comments are never continued; a `#` that starts a continuation line is part of the value.
        let out = set("# path C:\\\nk=old\n", &[("k", "new")]);
        assert_eq!(out, "# path C:\\\nk=new\n");
        let out = set("! note \\\n\nk=old\n", &[("k", "new")]);
        assert_eq!(out, "! note \\\n\nk=new\n");
        let out = set("a=x\\\n# not a comment\nk=old\n", &[("k", "new")]);
        assert_eq!(out, "a=x\\\n# not a comment\nk=new\n");
    }

    #[test]
    fn escapes_what_the_format_needs() {
        assert_eq!(
            set("", &[("a b=c", " lead\nline\\ \u{e9}\u{1f600}")]),
            "a\\ b\\=c=\\ lead\\nline\\\\ \\u00e9\\ud83d\\ude00\n"
        );
        assert_eq!(set("", &[("#k", "v")]), "\\#k=v\n");
    }

    #[test]
    fn matches_escaped_keys_and_leaves_bytes_it_does_not_touch() {
        // Bytes above 0x7f in untouched lines (a UTF-8 caf\u{e9}) must survive unchanged.
        let existing = b"caf=caf\xc3\xa9\na\\ b=1\n";
        let out = set_properties(existing, &changes(&[("a b", "2")]));
        assert_eq!(out, b"caf=caf\xc3\xa9\na\\ b=2\n");
    }

    #[test]
    fn set_properties_never_panics_on_odd_input() {
        for bytes in [
            &b"\\"[..],
            b"\\u",
            b"\\u12",
            b"=\n=",
            b"\xff\xfe\\\n\\\n",
            b"a\\\r\n",
        ] {
            let _ = set_properties(bytes, &changes(&[("a", "1"), ("\\u12", "x")]));
        }
    }
}
