//! The small amount of JSON writing palettefit needs.
//!
//! palettefit only ever *writes* JSON -- the palette file has its own plain
//! text format -- so this is a handful of helpers rather than `serde` plus
//! `serde_json` in the dependency tree. The document shapes are spelled out
//! in the README.

use std::fmt::Write as _;

/// Escape a string and wrap it in double quotes, ready to splice into output.
pub fn quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            // Control characters have no short escape; the \u form is required.
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// A string field, or `null` when it is absent.
pub fn opt_str(value: Option<&str>) -> String {
    match value {
        Some(s) => quote(s),
        None => "null".to_string(),
    }
}

/// A float rounded to four decimals, or `null` when it is absent.
pub fn opt_f4(value: Option<f64>) -> String {
    match value {
        Some(v) => format!("{v:.4}"),
        None => "null".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoting() {
        assert_eq!(quote("a\"b\\c\nd"), "\"a\\\"b\\\\c\\nd\"");
        assert_eq!(quote("\u{01}"), "\"\\u0001\"");
        assert_eq!(opt_str(None), "null");
        assert_eq!(opt_f4(Some(0.123456)), "0.1235");
    }
}
