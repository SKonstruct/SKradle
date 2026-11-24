//! The sentences Rust produces for the player (launch and mod errors, folder checks, the settings warning) are looked
//! up in the same catalogues as the QML text, under the context `rust`. `tr("...")` takes the English sentence as it is
//! written in the code: `i18n/rust_strings.py` collects every literal passed to `tr` and `tr_args` into
//! `i18n/rust_strings.cpp` for `lupdate`, so write them as plain string literals. A sentence with a value in it uses
//! `%1`, `%2`, ... like QML does, filled in by `tr_args` after the lookup, so a translation can move the value.
//! Without a translator (English, unit tests) the sentence comes back as written.

use crate::bridge::glue;

/// The translation of `source`, or `source` itself.
pub fn tr(source: &str) -> String {
    glue::translate_text(source)
}

/// [`tr`], then `%1`, `%2`, ... replaced by `args` in one pass (an argument that contains `%2` is left alone).
pub fn tr_args(source: &str, args: &[&str]) -> String {
    fill(&tr(source), args)
}

fn fill(template: &str, args: &[&str]) -> String {
    let mut out = String::with_capacity(template.len() + 16);
    let mut chars = template.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '%' {
            if let Some(n) = chars
                .peek()
                .and_then(|d| d.to_digit(10))
                .filter(|n| *n >= 1)
            {
                if let Some(arg) = args.get(n as usize - 1) {
                    out.push_str(arg);
                    chars.next();
                    continue;
                }
            }
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_fill_the_placeholders_in_one_pass() {
        assert_eq!(fill("a %1 b %2 c", &["x", "%2"]), "a x b %2 c");
        assert_eq!(fill("%2 before %1", &["one", "two"]), "two before one");
        assert_eq!(fill("100% sure, %3 stays", &["a"]), "100% sure, %3 stays");
    }

    #[test]
    fn without_a_translator_the_sentence_comes_back_as_written() {
        assert_eq!(tr("Can't be used."), "Can't be used.");
        assert_eq!(tr_args("Java %1 or newer", &["25"]), "Java 25 or newer");
    }
}
