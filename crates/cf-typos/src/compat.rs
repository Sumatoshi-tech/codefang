//! Self-contained foundation types: Git hash and a report-contract JSON value
//! + encoder.
//!
//! The integrated pipeline routes report serialization through the shared
//! `cf-gojson` crate (and `cf-gitlib` for the hash type). This module provides
//! the minimal local slice of those contracts so the typos analyzer stands
//! alone and verifiable; consumers convert the local [`GoValue`] to the shared
//! encoder types at the boundary. The value variants and the hash API mirror
//! the shared crates, so a swap is mechanical.

use std::fmt;

/// Length of a Git SHA-1 hash in bytes.
pub const HASH_SIZE: usize = 20;

/// A Git object hash (SHA-1), a 20-byte array.
///
/// Mirrors `cf_gitlib::Hash`. The zero value is all-zero bytes;
/// [`Hash::string`] renders lowercase hex.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, PartialOrd, Ord)]
pub struct Hash(pub [u8; HASH_SIZE]);

impl Hash {
    /// Returns the lowercase hex string representation.
    #[must_use]
    pub fn string(&self) -> String {
        use std::fmt::Write;
        let mut s = String::with_capacity(HASH_SIZE * 2);
        for b in &self.0 {
            let _ = write!(s, "{b:02x}");
        }
        s
    }
}

impl fmt::Display for Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.string())
    }
}

/// A report-contract JSON value.
///
/// Mirrors `cf_gojson::GoValue` for the variants the typos report uses. Object
/// keys follow the contract's dual-mode rule:
///
/// * [`GoValue::Map`] — **map-origin**: keys sorted by raw UTF-8 bytes at
///   encode time (dynamic maps).
/// * [`GoValue::Struct`] — **struct-origin**: field declaration order
///   preserved (typed records).
#[derive(Debug, Clone, PartialEq)]
pub enum GoValue {
    /// JSON `null`.
    Null,
    /// Signed integer; rendered with no decimal point.
    Int(i64),
    /// UTF-8 string.
    Str(String),
    /// JSON array.
    Array(Vec<GoValue>),
    /// Map-origin object (sorted keys at encode time).
    Map(Vec<(String, GoValue)>),
    /// Struct-origin object (declaration order preserved).
    Struct(Vec<(String, GoValue)>),
}

impl GoValue {
    /// Convenience constructor for a map-origin object.
    #[must_use]
    pub fn map<I, K>(entries: I) -> GoValue
    where
        I: IntoIterator<Item = (K, GoValue)>,
        K: Into<String>,
    {
        GoValue::Map(entries.into_iter().map(|(k, v)| (k.into(), v)).collect())
    }

    /// Serializes to compact JSON, per the report contract (no whitespace, map
    /// keys sorted, HTML escaping on, no trailing newline).
    #[must_use]
    pub fn to_json(&self) -> String {
        let mut out = String::new();
        self.write_json(&mut out, None, 0);
        out
    }

    /// Serializes to two-space-indented JSON, per the report contract's
    /// indented mode (a single space after each colon; empty containers
    /// collapse to `{}` / `[]`). No trailing newline is added.
    #[must_use]
    pub fn to_json_indent(&self) -> String {
        let mut out = String::new();
        self.write_json(&mut out, Some("  "), 0);
        out
    }

    fn write_json(&self, out: &mut String, indent: Option<&str>, depth: usize) {
        match self {
            GoValue::Null => out.push_str("null"),
            GoValue::Int(n) => out.push_str(&n.to_string()),
            GoValue::Str(s) => write_json_string(s, out),
            GoValue::Array(items) => {
                write_seq(out, indent, depth, items.iter(), |out, item, ind, d| {
                    item.write_json(out, ind, d)
                })
            }
            GoValue::Map(entries) => {
                // map-origin: sort keys by raw UTF-8 bytes at encode time.
                let mut sorted: Vec<&(String, GoValue)> = entries.iter().collect();
                sorted.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
                write_obj(out, indent, depth, sorted.into_iter());
            }
            GoValue::Struct(entries) => {
                // struct-origin: preserve declaration order.
                write_obj(out, indent, depth, entries.iter());
            }
        }
    }
}

/// Writes an array/sequence with optional indentation.
fn write_seq<T, I, F>(out: &mut String, indent: Option<&str>, depth: usize, items: I, mut f: F)
where
    I: Iterator<Item = T>,
    F: FnMut(&mut String, T, Option<&str>, usize),
{
    let items: Vec<T> = items.collect();
    if items.is_empty() {
        out.push_str("[]");
        return;
    }
    out.push('[');
    for (i, item) in items.into_iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        newline_indent(out, indent, depth + 1);
        f(out, item, indent, depth + 1);
    }
    newline_indent(out, indent, depth);
    out.push(']');
}

/// Writes an object with optional indentation, in the given entry order.
fn write_obj<'a, I>(out: &mut String, indent: Option<&str>, depth: usize, entries: I)
where
    I: Iterator<Item = &'a (String, GoValue)>,
{
    let entries: Vec<&(String, GoValue)> = entries.collect();
    if entries.is_empty() {
        out.push_str("{}");
        return;
    }
    out.push('{');
    for (i, (k, v)) in entries.into_iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        newline_indent(out, indent, depth + 1);
        write_json_string(k, out);
        out.push(':');
        if indent.is_some() {
            out.push(' '); // Indent mode: one space after the colon.
        }
        v.write_json(out, indent, depth + 1);
    }
    newline_indent(out, indent, depth);
    out.push('}');
}

/// Emits a newline plus `depth` copies of the indent unit, in indent mode only.
fn newline_indent(out: &mut String, indent: Option<&str>, depth: usize) {
    if let Some(unit) = indent {
        out.push('\n');
        for _ in 0..depth {
            out.push_str(unit);
        }
    }
}

/// Writes a report-contract quoted JSON string (HTML escaping on).
fn write_json_string(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '<' => out.push_str("\\u003c"),
            '>' => out.push_str("\\u003e"),
            '&' => out.push_str("\\u0026"),
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_hex_zero() {
        assert_eq!(Hash::default().string(), "0".repeat(40));
    }

    #[test]
    fn hash_hex_nonzero() {
        let mut b = [0u8; 20];
        b[0] = 0xab;
        b[19] = 0x0f;
        let s = Hash(b).string();
        assert!(s.starts_with("ab"));
        assert!(s.ends_with("0f"));
        assert_eq!(s.len(), 40);
    }

    #[test]
    fn map_sorts_keys_compact() {
        let v = GoValue::map([
            ("typos", GoValue::Array(vec![])),
            ("total_typos", GoValue::Int(0)),
            ("total_count", GoValue::Int(0)),
        ]);
        assert_eq!(
            v.to_json(),
            r#"{"total_count":0,"total_typos":0,"typos":[]}"#
        );
    }

    #[test]
    fn struct_preserves_order_compact() {
        let v = GoValue::Struct(vec![
            ("wrong".to_string(), GoValue::Str("tets".to_string())),
            ("correct".to_string(), GoValue::Str("test".to_string())),
            ("line".to_string(), GoValue::Int(10)),
        ]);
        assert_eq!(
            v.to_json(),
            r#"{"wrong":"tets","correct":"test","line":10}"#
        );
    }

    #[test]
    fn empty_containers_collapse() {
        assert_eq!(GoValue::Array(vec![]).to_json_indent(), "[]");
        assert_eq!(GoValue::Map(vec![]).to_json_indent(), "{}");
    }

    #[test]
    fn indent_mode_layout() {
        let v = GoValue::Struct(vec![
            ("a".to_string(), GoValue::Int(1)),
            ("b".to_string(), GoValue::Int(2)),
        ]);
        assert_eq!(v.to_json_indent(), "{\n  \"a\": 1,\n  \"b\": 2\n}");
    }

    #[test]
    fn escapes_html() {
        // HTML escaping is ON in the report contract: <, >, & become their
        // \u00xx escape sequences.
        assert_eq!(
            GoValue::Str("a<b>&c".to_string()).to_json(),
            "\"a\\u003cb\\u003e\\u0026c\""
        );
    }
}
