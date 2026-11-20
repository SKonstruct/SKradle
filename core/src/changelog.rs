//! The changelog (`CHANGELOG.md`, Keep a Changelog style) as data for the release-notes viewer.
//!
//! The app embeds the file at build time and parses it here, so the viewer needs no network. The subset understood:
//!
//! * `## [Unreleased]`, `## [0.1.0] - 2026-01-31` (also `## 0.1.0 - date`): one [`Version`] each;
//! * `### Added` and so on: a [`Section`] each; text before the first one is the version's `intro`;
//! * bullets (`- ` or `* `, one indent level of nesting) with continuation lines, and plain paragraphs;
//! * inline `**bold**`, `*italic*`, `` `code` `` and `[text](https://...)` links.
//!
//! Inline text is turned into escaped Qt rich text (`<b>`, `<i>`, `<tt>`, `<a href>`), so the UI can show it without
//! a web view and without trusting the file: every character outside the tags is HTML-escaped and a link survives
//! only when it is a plain `https://` address. Anything else (link reference definitions, HTML, tables) is shown as
//! text or skipped, never executed.

use serde::Serialize;

use crate::net::is_plain_https_url;

/// One release (or `Unreleased`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Version {
    /// `Unreleased` or the version number as written, e.g. `0.1.0`.
    pub title: String,
    /// The date after the version (`2026-01-31`), if any.
    pub date: Option<String>,
    /// Paragraphs before the first `###` heading (rich text).
    pub intro: Vec<String>,
    pub sections: Vec<Section>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Section {
    pub name: String,
    pub items: Vec<Item>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Item {
    /// Rich text (see the module documentation).
    pub html: String,
    /// A bullet (`false`: a plain paragraph inside the section).
    pub bullet: bool,
    /// 0, or 1 for a nested bullet.
    pub level: u8,
}

/// Parses the whole file. Text before the first `## ` heading (the title and the introduction) is ignored.
pub fn parse(markdown: &str) -> Vec<Version> {
    let mut versions: Vec<Version> = Vec::new();
    // The item being built: its raw text, bullet flag and level.
    let mut open: Option<(String, bool, u8)> = None;
    let mut in_fence = false;

    fn flush(versions: &mut [Version], open: &mut Option<(String, bool, u8)>) {
        let Some((raw, bullet, level)) = open.take() else {
            return;
        };
        let html = inline_html(raw.trim());
        if html.is_empty() {
            return;
        }
        let Some(version) = versions.last_mut() else {
            return;
        };
        match version.sections.last_mut() {
            Some(section) => section.items.push(Item {
                html,
                bullet,
                level,
            }),
            None if !bullet => version.intro.push(html),
            // A bullet before any `###`: give it a nameless section.
            None => version.sections.push(Section {
                name: String::new(),
                items: vec![Item {
                    html,
                    bullet,
                    level,
                }],
            }),
        }
    }

    for line in markdown.lines() {
        let line = line.trim_end();
        if line.trim_start().starts_with("```") {
            flush(&mut versions, &mut open);
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        if let Some(heading) = line.strip_prefix("## ") {
            flush(&mut versions, &mut open);
            versions.push(version_heading(heading));
            continue;
        }
        if let Some(heading) = line.strip_prefix("### ") {
            flush(&mut versions, &mut open);
            if let Some(version) = versions.last_mut() {
                version.sections.push(Section {
                    name: plain_text(heading),
                    items: Vec::new(),
                });
            }
            continue;
        }
        if line.starts_with('#') {
            // Another heading level (the document title, `####`): ends the current item.
            flush(&mut versions, &mut open);
            continue;
        }
        if line.trim().is_empty() {
            flush(&mut versions, &mut open);
            continue;
        }
        let rule = line.trim();
        if rule.len() >= 3 && rule.chars().all(|c| matches!(c, '-' | '*' | '_')) {
            // A horizontal rule.
            flush(&mut versions, &mut open);
            continue;
        }
        if is_reference_definition(line) {
            flush(&mut versions, &mut open);
            continue;
        }
        if versions.is_empty() {
            continue;
        }
        if let Some((level, rest)) = bullet_start(line) {
            flush(&mut versions, &mut open);
            open = Some((rest.to_owned(), true, level));
            continue;
        }
        // A continuation of the open item, or the start of a paragraph.
        match open.as_mut() {
            Some((raw, _, _)) => {
                raw.push(' ');
                raw.push_str(line.trim());
            }
            None => open = Some((line.trim().to_owned(), false, 0)),
        }
    }
    flush(&mut versions, &mut open);
    versions
}

/// `[Unreleased]`, `[0.1.0] - 2026-01-31`, `0.1.0 - 2026-01-31`, `[0.1.0] - 2026-01-31 [YANKED]`.
fn version_heading(text: &str) -> Version {
    let text = text.trim();
    let (title, rest) = match text.strip_prefix('[').and_then(|t| t.split_once(']')) {
        Some((title, rest)) => (title.trim().to_owned(), rest.trim()),
        None => match text.split_once(" - ") {
            Some((title, rest)) => (title.trim().to_owned(), rest.trim()),
            None => (text.to_owned(), ""),
        },
    };
    let rest = rest.trim_start_matches(['-', '\u{2013}', '\u{2014}', ' ']);
    let date = rest.split_whitespace().next().map(str::to_owned);
    Version {
        title: plain_text(&title),
        date: date.filter(|d| !d.is_empty()),
        intro: Vec::new(),
        sections: Vec::new(),
    }
}

/// `[name]: https://...` (a link reference definition at the end of the file).
fn is_reference_definition(line: &str) -> bool {
    let Some(rest) = line.strip_prefix('[') else {
        return false;
    };
    rest.split_once("]:")
        .is_some_and(|(label, target)| !label.contains(']') && !target.trim().is_empty())
}

/// A bullet line: `- text`, `* text`, `+ text`, with up to 1 nesting level (two or more leading spaces or a tab).
fn bullet_start(line: &str) -> Option<(u8, &str)> {
    let indent = line.len() - line.trim_start_matches([' ', '\t']).len();
    let rest = line.trim_start_matches([' ', '\t']);
    if matches!(rest, "-" | "*" | "+") {
        return Some((u8::from(indent >= 2), ""));
    }
    let body = ["- ", "* ", "+ "]
        .iter()
        .find_map(|marker| rest.strip_prefix(marker))?;
    Some((u8::from(indent >= 2), body.trim_start()))
}

/// A heading's text without any markup.
fn plain_text(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '*' | '`' => {}
            '\\' => out.extend(chars.next()),
            _ => out.push(c),
        }
    }
    out.trim().to_owned()
}

fn escape(text: &str, out: &mut String) {
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            c if c.is_control() => out.push(' '),
            c => out.push(c),
        }
    }
}

/// Markdown inline text to escaped Qt rich text.
pub fn inline_html(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    inline(&chars, &mut out, 0);
    out
}

/// Nesting is bounded so a hostile file cannot recurse deeply.
const MAX_DEPTH: usize = 4;

fn find_seq(chars: &[char], from: usize, seq: &[char]) -> Option<usize> {
    (from..chars.len().saturating_sub(seq.len() - 1)).find(|&i| chars[i..].starts_with(seq))
}

fn inline(chars: &[char], out: &mut String, depth: usize) {
    let mut i = 0;
    let mut literal = String::new();
    macro_rules! flush_literal {
        () => {
            if !literal.is_empty() {
                escape(&literal, out);
                literal.clear();
            }
        };
    }
    while i < chars.len() {
        let c = chars[i];
        match c {
            '\\' if chars.get(i + 1).is_some_and(|n| n.is_ascii_punctuation()) => {
                literal.push(chars[i + 1]);
                i += 2;
            }
            '`' => {
                let ticks = chars[i..].iter().take_while(|&&c| c == '`').count();
                let fence = vec!['`'; ticks];
                match find_seq(chars, i + ticks, &fence) {
                    Some(end) if end > i + ticks => {
                        flush_literal!();
                        let code: String = chars[i + ticks..end].iter().collect();
                        out.push_str("<tt>");
                        escape(code.trim(), out);
                        out.push_str("</tt>");
                        i = end + ticks;
                    }
                    _ => {
                        literal.extend(std::iter::repeat_n('`', ticks));
                        i += ticks;
                    }
                }
            }
            '*' if depth < MAX_DEPTH => {
                let stars = chars[i..].iter().take_while(|&&c| c == '*').count();
                let (tag, width) = if stars >= 2 { ("b", 2) } else { ("i", 1) };
                let marker = vec!['*'; width];
                let close = find_closing_emphasis(chars, i + width, &marker);
                match close {
                    Some(end) if end > i + width => {
                        flush_literal!();
                        out.push_str(&format!("<{tag}>"));
                        inline(&chars[i + width..end], out, depth + 1);
                        out.push_str(&format!("</{tag}>"));
                        i = end + width;
                    }
                    _ => {
                        literal.extend(std::iter::repeat_n('*', stars));
                        i += stars;
                    }
                }
            }
            '[' if depth < MAX_DEPTH => match parse_link(chars, i) {
                Some((label, url, next)) => {
                    flush_literal!();
                    match url {
                        Some(url) => {
                            out.push_str("<a href=\"");
                            escape(&url, out);
                            out.push_str("\">");
                            inline(label, out, depth + 1);
                            out.push_str("</a>");
                        }
                        // A link that is not a plain https address shows only its label.
                        None => inline(label, out, depth + 1),
                    }
                    i = next;
                }
                None => {
                    literal.push('[');
                    i += 1;
                }
            },
            _ => {
                literal.push(c);
                i += 1;
            }
        }
    }
    flush_literal!();
}

/// The closing `marker` of an emphasis that starts before `from`: not preceded by a space, and (for a single `*`)
/// not part of a `**`. Code spans are skipped so a `*` inside them does not close anything.
fn find_closing_emphasis(chars: &[char], from: usize, marker: &[char]) -> Option<usize> {
    if chars.get(from).is_none_or(|c| c.is_whitespace()) {
        return None;
    }
    let mut i = from;
    while i < chars.len() {
        if chars[i] == '`' {
            let ticks = chars[i..].iter().take_while(|&&c| c == '`').count();
            let fence = vec!['`'; ticks];
            i = match find_seq(chars, i + ticks, &fence) {
                Some(end) => end + ticks,
                None => i + ticks,
            };
            continue;
        }
        if chars[i] == '\\' {
            i += 2;
            continue;
        }
        if chars[i] == '*' {
            let run = chars[i..].iter().take_while(|&&c| c == '*').count();
            let after_text = !chars[i - 1].is_whitespace();
            if run >= marker.len() && after_text && (marker.len() == 2 || run == 1) {
                return Some(i);
            }
            // A run of stars that cannot close here (`**` inside a single-star emphasis, or after a space).
            i += run;
            continue;
        }
        i += 1;
    }
    None
}

/// `[label](url)` at `at` (which is a `[`): the label's characters, the url when it is a plain https address, and
/// the index after the `)`.
fn parse_link(chars: &[char], at: usize) -> Option<(&[char], Option<String>, usize)> {
    let mut depth = 0usize;
    let mut close = None;
    for (offset, &c) in chars[at..].iter().enumerate() {
        match c {
            '\\' => {}
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    close = Some(at + offset);
                    break;
                }
            }
            _ => {}
        }
    }
    let close = close?;
    if chars.get(close + 1) != Some(&'(') {
        return None;
    }
    // The address may contain balanced parentheses.
    let mut open = 1usize;
    let end = (close + 2..chars.len()).find(|&i| {
        match chars[i] {
            '(' => open += 1,
            ')' => open -= 1,
            _ => {}
        }
        open == 0
    })?;
    let target: String = chars[close + 2..end].iter().collect();
    // Drop an optional title: `(url "title")`.
    let url = target.split_whitespace().next().unwrap_or("");
    let url = is_plain_https_url(url).then(|| url.to_owned());
    Some((&chars[at + 1..close], url, end + 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    const REAL: &str = include_str!("../../CHANGELOG.md");

    #[test]
    fn the_real_changelog_parses_into_versions_sections_and_items() {
        let versions = parse(REAL);
        assert!(!versions.is_empty());
        assert_eq!(versions[0].title, "Unreleased");
        assert_eq!(versions[0].date, None);
        let names: Vec<&str> = versions[0]
            .sections
            .iter()
            .map(|s| s.name.as_str())
            .collect();
        assert!(names.contains(&"Added"), "{names:?}");
        for version in &versions {
            assert!(!version.title.is_empty());
            for section in &version.sections {
                assert!(!section.items.is_empty(), "{}: empty section", section.name);
                for item in &section.items {
                    assert!(!item.html.is_empty());
                    assert!(!item.html.contains("**"), "{}", item.html);
                    assert!(!item.html.contains('`'), "{}", item.html);
                    assert!(!item.html.contains("]("), "{}", item.html);
                }
            }
        }
        // A multi-line bullet of the real file is one item, with its bold lead-in.
        let launcher = versions[0].sections[0]
            .items
            .iter()
            .find(|i| i.html.starts_with("<b>Launcher app</b>"))
            .expect("the launcher bullet");
        assert!(launcher.bullet && launcher.level == 0);
        assert!(launcher.html.contains("light and dark theme"));
        assert!(!launcher.html.contains('\n'));
    }

    #[test]
    fn versions_dates_and_sections_in_the_usual_shapes() {
        let md = "# Changelog\n\nIntro text.\n\n\
                  ## [Unreleased]\n\n### Added\n\n- One.\n\n\
                  ## [1.2.0] - 2026-03-04\n\n### Fixed\n- Two.\n\n### Changed\n- Three.\n\n\
                  ## 1.1.0 - 2026-02-01 [YANKED]\n\n### Removed\n- Four.\n\n\
                  ## [1.0.0]\n- Five.\n\n\
                  [Unreleased]: https://example.org/compare\n[1.2.0]: https://example.org/x\n";
        let v = parse(md);
        let heads: Vec<_> = v
            .iter()
            .map(|v| (v.title.as_str(), v.date.as_deref()))
            .collect();
        assert_eq!(
            heads,
            [
                ("Unreleased", None),
                ("1.2.0", Some("2026-03-04")),
                ("1.1.0", Some("2026-02-01")),
                ("1.0.0", None)
            ]
        );
        assert_eq!(v[1].sections.len(), 2);
        assert_eq!(v[1].sections[1].name, "Changed");
        assert_eq!(v[1].sections[1].items[0].html, "Three.");
        // A bullet before any `###` gets a nameless section; reference definitions are not items.
        assert_eq!(v[3].sections[0].name, "");
        assert_eq!(v[3].sections[0].items.len(), 1);
        assert!(v[3].sections[0].items[0].html.contains("Five."));
        assert_eq!(v[3].sections[0].items.len(), 1, "no `[x]: url` item");
    }

    #[test]
    fn multi_line_bullets_nesting_paragraphs_and_the_intro() {
        let md = "## [1.0.0]\nA paragraph about the release\ncontinuing here.\n\n### Added\n\
                  - First line\n  second line\n  third line.\n\
                  - Next:\n  - nested one\n  - nested two\n* Star bullet\n\n\
                  A closing paragraph.\n";
        let v = parse(md);
        assert_eq!(
            v[0].intro,
            ["A paragraph about the release continuing here."]
        );
        let items = &v[0].sections[0].items;
        assert_eq!(items[0].html, "First line second line third line.");
        assert_eq!(items[1].html, "Next:");
        assert_eq!((items[2].html.as_str(), items[2].level), ("nested one", 1));
        assert_eq!(items[3].level, 1);
        assert_eq!(items[4].html, "Star bullet");
        assert!(items[4].bullet && items[4].level == 0);
        assert_eq!(items[5].html, "A closing paragraph.");
        assert!(!items[5].bullet);
    }

    #[test]
    fn inline_markdown_becomes_escaped_rich_text() {
        assert_eq!(
            inline_html("**bold** and *it* and `c<d>`"),
            "<b>bold</b> and <i>it</i> and <tt>c&lt;d&gt;</tt>"
        );
        assert_eq!(
            inline_html("[the site](https://example.org/a?b=1&c=2) now"),
            "<a href=\"https://example.org/a?b=1&amp;c=2\">the site</a> now"
        );
        assert_eq!(
            inline_html("**bold with `code` and [a link](https://example.org)**"),
            "<b>bold with <tt>code</tt> and <a href=\"https://example.org\">a link</a></b>"
        );
        assert_eq!(
            inline_html("*italic **inner bold** more*"),
            "<i>italic <b>inner bold</b> more</i>"
        );
        assert_eq!(
            inline_html("a < b & c > d \"q\""),
            "a &lt; b &amp; c &gt; d &quot;q&quot;"
        );
        assert_eq!(
            inline_html(r"\*not italic\* and \[x\]"),
            "*not italic* and [x]"
        );
        assert_eq!(
            inline_html("``double `tick` code``"),
            "<tt>double `tick` code</tt>"
        );
    }

    #[test]
    fn unsafe_links_lose_the_link_and_markup_cannot_inject_tags() {
        for url in [
            "http://example.org",
            "javascript:alert(1)",
            "file:///etc/passwd",
            "steam://run/1",
            "https://user@evil.example",
            "/relative",
        ] {
            let html = inline_html(&format!("[label]({url})"));
            assert_eq!(html, "label", "{url}");
        }
        assert_eq!(
            inline_html("[l](https://example.org/x \"title\")"),
            "<a href=\"https://example.org/x\">l</a>"
        );
        let html = inline_html("<script>alert(1)</script> <a href=\"x\">y</a> <b>z</b>");
        assert!(!html.contains('<') || !html.contains("<script"), "{html}");
        assert_eq!(
            html,
            "&lt;script&gt;alert(1)&lt;/script&gt; &lt;a href=&quot;x&quot;&gt;y&lt;/a&gt; &lt;b&gt;z&lt;/b&gt;"
        );
        // A quote cannot break out of the attribute.
        assert_eq!(inline_html("[l](https://example.org/\"onclick=1)"), "l");
    }

    #[test]
    fn unbalanced_or_odd_markup_stays_text_and_never_panics() {
        assert_eq!(inline_html("a * b * c"), "a * b * c");
        assert_eq!(inline_html("5 * 3 = 15 and 2*3"), "5 * 3 = 15 and 2*3");
        assert_eq!(inline_html("**unclosed"), "**unclosed");
        assert_eq!(inline_html("*unclosed"), "*unclosed");
        assert_eq!(inline_html("`unclosed"), "`unclosed");
        assert_eq!(inline_html("[unclosed"), "[unclosed");
        assert_eq!(inline_html("[label] no link"), "[label] no link");
        assert_eq!(
            inline_html("[a](https://example.org"),
            "[a](https://example.org"
        );
        assert_eq!(inline_html("****"), "****");
        assert_eq!(inline_html(""), "");
        assert_eq!(inline_html("\\"), "\\");
        // Deep nesting is cut off, not recursed into.
        let deep = "*a ".repeat(200) + &"b*".repeat(200);
        let _ = inline_html(&deep);
        let deep_links = "[".repeat(500) + "x" + &"](https://example.org)".repeat(500);
        let _ = inline_html(&deep_links);
    }

    #[test]
    fn edge_documents() {
        assert!(parse("").is_empty());
        assert!(parse("# Changelog\n\nOnly an introduction.\n").is_empty());
        let v = parse("## [1.0.0]\n");
        assert_eq!(v.len(), 1);
        assert!(v[0].sections.is_empty() && v[0].intro.is_empty());
        // Code fences are skipped; Windows line endings and a missing final newline work.
        let v = parse("## [1.0.0]\r\n### Added\r\n- One\r\n```\r\n- not an item\r\n```\r\n- Two");
        let texts: Vec<_> = v[0].sections[0]
            .items
            .iter()
            .map(|i| i.html.as_str())
            .collect();
        assert_eq!(texts, ["One", "Two"]);
        // A heading with markup and an empty bullet.
        let v = parse("## [1.0.0]\n### **Added** `x`\n-\n- \n- Real\n");
        assert_eq!(v[0].sections[0].name, "Added x");
        assert_eq!(v[0].sections[0].items.len(), 1);
    }

    #[test]
    fn the_json_has_the_shape_the_viewer_reads() {
        let json =
            serde_json::to_value(parse("## [1.0.0] - 2026-01-01\n### Added\n- **x**\n")).unwrap();
        assert_eq!(
            json,
            serde_json::json!([{
                "title": "1.0.0", "date": "2026-01-01", "intro": [],
                "sections": [{"name": "Added", "items": [{"html": "<b>x</b>", "bullet": true, "level": 0}]}]
            }])
        );
    }
}
