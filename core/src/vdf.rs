//! A small reader for Valve's text KeyValues format, as used by Steam's `libraryfolders.vdf` and
//! `appmanifest_*.acf`. Input comes from files on the user's machine, so it is parsed defensively
//! (bounded nesting, no panics on malformed text).

const MAX_DEPTH: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Str(String),
    Map(Vec<(String, Value)>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub offset: usize,
    pub message: &'static str,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} at byte {}", self.message, self.offset)
    }
}

impl std::error::Error for ParseError {}

impl Value {
    /// Looks up a key in a map; keys are matched case-insensitively, like Steam does.
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Map(entries) => entries
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(key))
                .map(|(_, v)| v),
            Value::Str(_) => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            Value::Map(_) => None,
        }
    }

    /// The entries of a map, or an empty slice for a string.
    pub fn entries(&self) -> &[(String, Value)] {
        match self {
            Value::Map(entries) => entries,
            Value::Str(_) => &[],
        }
    }
}

/// Parses a document into a map of its top-level entries.
pub fn parse(input: &str) -> Result<Value, ParseError> {
    // Some Windows tools save a UTF-8 byte order mark; it is not part of the first key.
    let input = input.strip_prefix('\u{feff}').unwrap_or(input);
    let mut parser = Parser {
        src: input.as_bytes(),
        pos: 0,
    };
    let entries = parser.map(0, false)?;
    Ok(Value::Map(entries))
}

struct Parser<'a> {
    src: &'a [u8],
    pos: usize,
}

impl Parser<'_> {
    fn err<T>(&self, message: &'static str) -> Result<T, ParseError> {
        Err(ParseError {
            offset: self.pos,
            message,
        })
    }

    fn skip_trivia(&mut self) {
        while let Some(&b) = self.src.get(self.pos) {
            if b.is_ascii_whitespace() {
                self.pos += 1;
            } else if b == b'/' && self.src.get(self.pos + 1) == Some(&b'/') {
                while self.pos < self.src.len() && self.src[self.pos] != b'\n' {
                    self.pos += 1;
                }
            } else {
                break;
            }
        }
    }

    fn map(&mut self, depth: usize, nested: bool) -> Result<Vec<(String, Value)>, ParseError> {
        if depth > MAX_DEPTH {
            return self.err("nesting too deep");
        }
        let mut entries = Vec::new();
        loop {
            self.skip_trivia();
            match self.src.get(self.pos) {
                None if nested => return self.err("unclosed brace"),
                None => return Ok(entries),
                Some(b'}') if nested => {
                    self.pos += 1;
                    return Ok(entries);
                }
                Some(b'}') => return self.err("unexpected closing brace"),
                Some(b'{') => return self.err("brace without a key"),
                Some(_) => {}
            }
            let key = self.string()?;
            self.skip_trivia();
            let value = if self.src.get(self.pos) == Some(&b'{') {
                self.pos += 1;
                Value::Map(self.map(depth + 1, true)?)
            } else if self.pos >= self.src.len() {
                return self.err("key without a value");
            } else {
                Value::Str(self.string()?)
            };
            entries.push((key, value));
        }
    }

    fn string(&mut self) -> Result<String, ParseError> {
        let mut out = Vec::new();
        if self.src.get(self.pos) == Some(&b'"') {
            self.pos += 1;
            loop {
                match self.src.get(self.pos) {
                    None => return self.err("unterminated string"),
                    Some(b'"') => {
                        self.pos += 1;
                        break;
                    }
                    Some(b'\\') => {
                        let escaped = match self.src.get(self.pos + 1) {
                            Some(b'n') => Some(b'\n'),
                            Some(b't') => Some(b'\t'),
                            Some(b'\\') => Some(b'\\'),
                            Some(b'"') => Some(b'"'),
                            _ => None,
                        };
                        match escaped {
                            Some(b) => {
                                out.push(b);
                                self.pos += 2;
                            }
                            None => {
                                out.push(b'\\');
                                self.pos += 1;
                            }
                        }
                    }
                    Some(&b) => {
                        out.push(b);
                        self.pos += 1;
                    }
                }
            }
        } else {
            while let Some(&b) = self.src.get(self.pos) {
                if b.is_ascii_whitespace() || b == b'{' || b == b'}' || b == b'"' {
                    break;
                }
                out.push(b);
                self.pos += 1;
            }
            if out.is_empty() {
                return self.err("expected a string");
            }
        }
        Ok(String::from_utf8_lossy(&out).into_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_leading_byte_order_mark_is_ignored() {
        let with_bom = format!("\u{feff}{LIBRARIES}");
        assert_eq!(parse(&with_bom).unwrap(), parse(LIBRARIES).unwrap());
    }

    const LIBRARIES: &str = r#"
"libraryfolders"
{
    "0"
    {
        "path"      "/Users/me/Library/Application Support/Steam"
        "label"     ""
        "apps"
        {
            "99900"     "123456"
        }
    }
    "1"
    {
        "path"      "D:\\Games\\SteamLibrary"   // second drive
    }
}
"#;

    #[test]
    fn reads_nested_maps_and_strings() {
        let doc = parse(LIBRARIES).unwrap();
        let folders = doc.get("libraryfolders").unwrap();
        assert_eq!(folders.entries().len(), 2);
        let first = folders.get("0").unwrap();
        assert_eq!(
            first.get("path").unwrap().as_str(),
            Some("/Users/me/Library/Application Support/Steam")
        );
        assert_eq!(
            first.get("apps").unwrap().get("99900").unwrap().as_str(),
            Some("123456")
        );
    }

    #[test]
    fn unescapes_windows_paths_and_skips_comments() {
        let doc = parse(LIBRARIES).unwrap();
        let second = doc.get("libraryfolders").unwrap().get("1").unwrap();
        assert_eq!(
            second.get("path").unwrap().as_str(),
            Some(r"D:\Games\SteamLibrary")
        );
    }

    #[test]
    fn keys_are_case_insensitive() {
        let doc = parse(r#""AppState" { "InstallDir" "Spiral Knights" }"#).unwrap();
        assert_eq!(
            doc.get("appstate")
                .unwrap()
                .get("installdir")
                .unwrap()
                .as_str(),
            Some("Spiral Knights")
        );
    }

    #[test]
    fn accepts_unquoted_words() {
        let doc = parse("key value\nother { a b }").unwrap();
        assert_eq!(doc.get("key").unwrap().as_str(), Some("value"));
        assert_eq!(
            doc.get("other").unwrap().get("a").unwrap().as_str(),
            Some("b")
        );
    }

    #[test]
    fn rejects_malformed_input_without_panicking() {
        for bad in [
            r#""a" {"#,
            r#""a" "b"#,
            "}",
            "{",
            r#""a""#,
            r#""a" { "b" } }"#,
        ] {
            assert!(parse(bad).is_err(), "should reject {bad:?}");
        }
    }

    #[test]
    fn caps_nesting_depth() {
        let deep = "\"a\" {".repeat(100) + &"}".repeat(100);
        assert_eq!(parse(&deep).unwrap_err().message, "nesting too deep");
    }

    #[test]
    fn empty_input_is_an_empty_map() {
        assert_eq!(parse("  \n// nothing\n").unwrap(), Value::Map(vec![]));
    }
}
