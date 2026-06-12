//! The report-format JSON encoder over [`GoValue`].
//!
//! This is the keystone the whole report layer routes through. Its encoding
//! rules are a frozen byte contract, pinned against the reference
//! implementation by `rust/tests/compat` (see `specs/rust-rewrite/DESIGN.md`
//! §2):
//!
//! * **HTML escaping ON** by default — `<`, `>`, `&` encode as `\u003c`,
//!   `\u003e`, `\u0026`, and `U+2028`/`U+2029` as `\u2028`/`\u2029`.
//! * **map keys byte-sorted**, struct fields in declaration order — decided by
//!   [`GoMap::encode_order`] via the value's [`crate::MapOrigin`].
//! * **floats** via [`crate::ftoa::format_json_float`] (the contract float
//!   layout), never Rust's `Display`.
//! * **compact** output ([`marshal`]) has no insignificant whitespace.
//! * **indented** output ([`marshal_indent`]) uses a two-space indent, `": "`
//!   after keys, and one element per line.
//!
//! Two entry styles are provided:
//!
//! * free functions [`marshal`] / [`marshal_indent`] (and `to_vec` aliases) —
//!   compact/indented, **no** trailing newline; and
//! * the [`Encoder`] builder — configurable indent and an optional trailing
//!   `\n` (the streaming one-value-per-line shape).
//!
//! [`GoMap`]: crate::value::GoMap
//! [`GoMap::encode_order`]: crate::value::GoMap::encode_order

use crate::ftoa::format_json_float;
use crate::value::GoValue;

/// The contract's default two-space indent unit.
const DEFAULT_INDENT: &str = "  ";

/// Encodes `value` as compact report-contract JSON bytes.
///
/// No insignificant whitespace, HTML escaping on, no trailing newline.
#[must_use]
pub fn marshal(value: &GoValue) -> Vec<u8> {
    let mut buf = Vec::new();
    write_compact(&mut buf, value);
    buf
}

/// Alias for [`marshal`]; some call sites read more naturally as `to_vec`.
#[must_use]
pub fn to_vec(value: &GoValue) -> Vec<u8> {
    marshal(value)
}

/// Encodes `value` as two-space-indented report-contract JSON bytes.
///
/// No trailing newline is emitted; callers add one where the report surface
/// expects it (or use [`Encoder::indented`] with
/// [`Encoder::with_trailing_newline`]).
#[must_use]
pub fn marshal_indent(value: &GoValue) -> Vec<u8> {
    let mut buf = Vec::new();
    write_indented(&mut buf, value, DEFAULT_INDENT, 0);
    buf
}

/// Alias for [`marshal_indent`].
#[must_use]
pub fn to_vec_indent(value: &GoValue) -> Vec<u8> {
    marshal_indent(value)
}

/// Formats `f` as a report-contract JSON number.
///
/// Re-exported convenience wrapper around [`crate::ftoa::format_json_float`];
/// this is the function the golden harness fuzzes against the reference
/// implementation.
#[must_use]
pub fn go_float(f: f64) -> String {
    format_json_float(f)
}

/// Owned, builder-style report-contract JSON encoder.
///
/// Construct with one of the named constructors and (optionally)
/// [`with_trailing_newline`](Encoder::with_trailing_newline), then call
/// [`encode`](Encoder::encode) / [`encode_to_vec`](Encoder::encode_to_vec) /
/// [`encode_to_string`](Encoder::encode_to_string). HTML escaping defaults to
/// on (the contract default); [`with_html_escaping`](Encoder::with_html_escaping)
/// turns it off for the one surface that requires it — the chart `option_*`
/// JSON embedded in `--format plot` pages. Every other machine-format report
/// path keeps the default.
///
/// | constructor | indent | trailing `\n` |
/// | --- | --- | --- |
/// | [`marshal`](Encoder::marshal) | none | no |
/// | [`compact`](Encoder::compact) | none | no |
/// | [`encoder`](Encoder::encoder) | none | **yes** |
/// | [`indented`](Encoder::indented) | given | no |
#[derive(Debug, Clone)]
pub struct Encoder {
    indent: Option<String>,
    trailing_newline: bool,
    html_escape: bool,
}

impl Default for Encoder {
    fn default() -> Self {
        Encoder {
            indent: None,
            trailing_newline: false,
            html_escape: true,
        }
    }
}

impl Encoder {
    /// Compact encoder with no trailing newline — the plain-marshal shape.
    #[must_use]
    pub fn marshal() -> Self {
        Encoder::default()
    }

    /// Compact encoder with no trailing newline.
    ///
    /// Identical to [`Encoder::marshal`]; named for call sites that pair it with
    /// [`with_trailing_newline`](Encoder::with_trailing_newline).
    #[must_use]
    pub fn compact() -> Self {
        Encoder::default()
    }

    /// Compact encoder that appends a trailing `\n` — the streaming shape
    /// (one newline per encoded value).
    // The name is established public API across the workspace; renaming it
    // would break consumers for zero benefit.
    #[allow(clippy::self_named_constructors)]
    #[must_use]
    pub fn encoder() -> Self {
        Encoder {
            indent: None,
            trailing_newline: true,
            html_escape: true,
        }
    }

    /// Indented encoder using `indent` as the per-level unit (`"  "` is the
    /// contract's two-space form). No trailing newline unless
    /// [`with_trailing_newline`](Encoder::with_trailing_newline) is set.
    #[must_use]
    pub fn indented(indent: &str) -> Self {
        Encoder {
            indent: Some(indent.to_string()),
            trailing_newline: false,
            html_escape: true,
        }
    }

    /// Returns a copy of this encoder with the trailing-newline behavior set.
    #[must_use]
    pub fn with_trailing_newline(mut self, on: bool) -> Self {
        self.trailing_newline = on;
        self
    }

    /// Returns a copy of this encoder with HTML escaping set. With escaping
    /// off, `<`, `>`, and `&` are written verbatim; everything else (including
    /// the unconditional `U+2028`/`U+2029` escapes) is unchanged.
    #[must_use]
    pub fn with_html_escaping(mut self, on: bool) -> Self {
        self.html_escape = on;
        self
    }

    /// Encodes `value`, returning the bytes (infallible for finite numbers).
    #[must_use]
    pub fn encode(&self, value: &GoValue) -> Vec<u8> {
        let mut buf = Vec::new();
        match &self.indent {
            Some(ind) => write_indented_opts(&mut buf, value, ind, 0, self.html_escape),
            None => write_compact_opts(&mut buf, value, self.html_escape),
        }
        if self.trailing_newline {
            buf.push(b'\n');
        }
        buf
    }

    /// Alias for [`encode`](Encoder::encode).
    #[must_use]
    pub fn encode_to_vec(&self, value: &GoValue) -> Vec<u8> {
        self.encode(value)
    }

    /// Encodes `value` to a `String` (output is always valid UTF-8).
    #[must_use]
    pub fn encode_to_string(&self, value: &GoValue) -> String {
        String::from_utf8(self.encode(value)).expect("encoder output is valid UTF-8")
    }
}

/// Writes `value` to `out` in compact form (HTML escaping on — the contract default).
fn write_compact(out: &mut Vec<u8>, value: &GoValue) {
    write_compact_opts(out, value, true);
}

/// Writes `value` to `out` in compact form with the given HTML-escaping mode.
fn write_compact_opts(out: &mut Vec<u8>, value: &GoValue, escape_html: bool) {
    match value {
        // A nil slice marshals as `null` in JSON (the YAML encoder renders
        // `[]` instead).
        GoValue::Null | GoValue::NilSlice => out.extend_from_slice(b"null"),
        GoValue::Bool(true) => out.extend_from_slice(b"true"),
        GoValue::Bool(false) => out.extend_from_slice(b"false"),
        GoValue::Int(i) => out.extend_from_slice(i.to_string().as_bytes()),
        GoValue::Uint(u) => out.extend_from_slice(u.to_string().as_bytes()),
        GoValue::Float(f) => out.extend_from_slice(format_json_float(*f).as_bytes()),
        GoValue::Str(s) => write_go_json_string_opts(out, s, escape_html),
        GoValue::Array(items) => {
            out.push(b'[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(b',');
                }
                write_compact_opts(out, item, escape_html);
            }
            out.push(b']');
        }
        GoValue::Map(m) => {
            out.push(b'{');
            for (i, (k, v)) in m.encode_order().iter().enumerate() {
                if i > 0 {
                    out.push(b',');
                }
                write_go_json_string_opts(out, k, escape_html);
                out.push(b':');
                write_compact_opts(out, v, escape_html);
            }
            out.push(b'}');
        }
    }
}

/// Writes the indent prefix for `depth` levels of `unit`.
fn write_indent(out: &mut Vec<u8>, unit: &str, depth: usize) {
    for _ in 0..depth {
        out.extend_from_slice(unit.as_bytes());
    }
}

/// Writes `value` to `out` indented with `unit`, current nesting `depth`.
///
/// Contract layout: empty objects/arrays stay `{}`/`[]` on one line; non-empty
/// containers put one element per line, a `": "` separator after object keys,
/// and the closing bracket at the parent indent. Scalars are identical to the
/// compact form.
fn write_indented(out: &mut Vec<u8>, value: &GoValue, unit: &str, depth: usize) {
    write_indented_opts(out, value, unit, depth, true);
}

/// [`write_indented`] with the HTML-escaping mode threaded through.
fn write_indented_opts(out: &mut Vec<u8>, value: &GoValue, unit: &str, depth: usize, escape_html: bool) {
    match value {
        GoValue::Array(items) if !items.is_empty() => {
            out.extend_from_slice(b"[\n");
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.extend_from_slice(b",\n");
                }
                write_indent(out, unit, depth + 1);
                write_indented_opts(out, item, unit, depth + 1, escape_html);
            }
            out.push(b'\n');
            write_indent(out, unit, depth);
            out.push(b']');
        }
        GoValue::Map(m) if !m.is_empty() => {
            out.extend_from_slice(b"{\n");
            for (i, (k, v)) in m.encode_order().iter().enumerate() {
                if i > 0 {
                    out.extend_from_slice(b",\n");
                }
                write_indent(out, unit, depth + 1);
                write_go_json_string_opts(out, k, escape_html);
                out.extend_from_slice(b": ");
                write_indented_opts(out, v, unit, depth + 1, escape_html);
            }
            out.push(b'\n');
            write_indent(out, unit, depth);
            out.push(b'}');
        }
        // Empty containers and all scalars render exactly like the compact form.
        other => write_compact_opts(out, other, escape_html),
    }
}

/// Writes `s` as a contract-quoted JSON string with HTML escaping on.
///
/// Byte-for-byte reproduction of the reference string encoder with its default
/// HTML-escaping mode:
///
/// * `"` → `\"`, `\` → `\\`;
/// * `\n` → `\n`, `\r` → `\r`, `\t` → `\t`;
/// * `0x08`/`0x0c` → the short escapes `\b`/`\f` (verified against the
///   reference binary);
/// * `<` → `\u003c`, `>` → `\u003e`, `&` → `\u0026` (HTML safety);
/// * every other control byte `< 0x20` → `\u00xx` (lowercase hex);
/// * `U+2028`/`U+2029` → `\u2028`/`\u2029` (the JS line/paragraph separators,
///   escaped for the same browser-safety reason).
///
/// Rust `&str` is always valid UTF-8, so the reference encoder's invalid-rune
/// replacement path is unreachable here.
pub fn write_go_json_string(out: &mut Vec<u8>, s: &str) {
    write_go_json_string_opts(out, s, true);
}

/// [`write_go_json_string`] with the HTML-escaping mode threaded through —
/// with `escape_html=false`, `<`, `>`, and `&` pass through verbatim while
/// every other escape (including the unconditional `U+2028`/`U+2029` pair) is
/// unchanged.
pub fn write_go_json_string_opts(out: &mut Vec<u8>, s: &str, escape_html: bool) {
    out.push(b'"');
    let bytes = s.as_bytes();
    let mut start = 0usize;
    for (i, ch) in s.char_indices() {
        let (esc, adv): (Option<&[u8]>, usize) = match ch {
            '"' => (Some(b"\\\""), 1),
            '\\' => (Some(b"\\\\"), 1),
            '\n' => (Some(b"\\n"), 1),
            '\r' => (Some(b"\\r"), 1),
            '\t' => (Some(b"\\t"), 1),
            // The contract uses the short escapes \b (0x08) and \f (0x0c) here,
            // not the generic \u00xx forms (verified against the reference
            // binary).
            '\u{0008}' => (Some(b"\\b"), 1),
            '\u{000c}' => (Some(b"\\f"), 1),
            '<' if escape_html => (Some(b"\\u003c"), 1),
            '>' if escape_html => (Some(b"\\u003e"), 1),
            '&' if escape_html => (Some(b"\\u0026"), 1),
            '\u{2028}' => (Some(b"\\u2028"), 3),
            '\u{2029}' => (Some(b"\\u2029"), 3),
            c if (c as u32) < 0x20 => {
                if start < i {
                    out.extend_from_slice(&bytes[start..i]);
                }
                const HEX: &[u8; 16] = b"0123456789abcdef";
                let b = c as u8;
                out.extend_from_slice(b"\\u00");
                out.push(HEX[((b >> 4) & 0xF) as usize]);
                out.push(HEX[(b & 0xF) as usize]);
                start = i + 1;
                continue;
            }
            _ => (None, 0),
        };
        if let Some(esc) = esc {
            if start < i {
                out.extend_from_slice(&bytes[start..i]);
            }
            out.extend_from_slice(esc);
            start = i + adv;
        }
    }
    if start < bytes.len() {
        out.extend_from_slice(&bytes[start..]);
    }
    out.push(b'"');
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::{GoMap, MapOrigin};

    fn st(b: Vec<u8>) -> String {
        String::from_utf8(b).unwrap()
    }

    #[test]
    fn scalars_compact() {
        assert_eq!(st(marshal(&GoValue::Null)), "null");
        assert_eq!(st(marshal(&GoValue::Bool(true))), "true");
        assert_eq!(st(marshal(&GoValue::Bool(false))), "false");
        assert_eq!(st(marshal(&GoValue::Int(42))), "42");
        assert_eq!(st(marshal(&GoValue::Int(-7))), "-7");
        assert_eq!(st(marshal(&GoValue::Uint(0))), "0");
        assert_eq!(st(marshal(&GoValue::Float(3.14))), "3.14");
        assert_eq!(st(marshal(&GoValue::Float(1e21))), "1e+21");
        assert_eq!(st(marshal(&GoValue::Float(1e-5))), "0.00001");
        assert_eq!(st(marshal(&GoValue::Str("hello".into()))), "\"hello\"");
    }

    #[test]
    fn html_and_special_escaping_matches_go() {
        // <, >, & HTML-escape to \u003c \u003e \u0026 by default.
        assert_eq!(
            st(marshal(&GoValue::Str("a<b>c&d".into()))),
            r#""a\u003cb\u003ec\u0026d""#
        );
        assert_eq!(st(marshal(&GoValue::Str("\"q\"".into()))), r#""\"q\"""#);
        assert_eq!(st(marshal(&GoValue::Str("a\\b".into()))), r#""a\\b""#);
        assert_eq!(st(marshal(&GoValue::Str("x\ny\tz".into()))), r#""x\ny\tz""#);
        // 0x08 and 0x0c use the short escapes \b and \f (contract behavior).
        assert_eq!(st(marshal(&GoValue::Str("\u{0008}\u{000c}".into()))), r#""\b\f""#);
        // line/paragraph separators.
        assert_eq!(st(marshal(&GoValue::Str("\u{2028}\u{2029}".into()))), r#""\u2028\u2029""#);
        // forward slash is NOT escaped.
        assert_eq!(st(marshal(&GoValue::Str("a/b".into()))), r#""a/b""#);
    }

    #[test]
    fn map_origin_sorts_keys_struct_keeps_order() {
        let mut map = GoMap::new(MapOrigin::Map);
        map.push("zebra", GoValue::Int(1));
        map.push("apple", GoValue::Int(2));
        map.push("Mango", GoValue::Int(3));
        // byte order: 'M'(0x4d) < 'a'(0x61) < 'z'(0x7a).
        assert_eq!(st(marshal(&GoValue::Map(map))), r#"{"Mango":3,"apple":2,"zebra":1}"#);

        let mut s = GoMap::new_struct();
        s.push("score", GoValue::Float(0.5));
        s.push("name", GoValue::Str("x".into()));
        assert_eq!(st(marshal(&GoValue::Map(s))), r#"{"score":0.5,"name":"x"}"#);
    }

    #[test]
    fn empty_containers() {
        assert_eq!(st(marshal(&GoValue::Array(vec![]))), "[]");
        assert_eq!(st(marshal(&GoValue::Map(GoMap::new_map()))), "{}");
        assert_eq!(st(marshal_indent(&GoValue::Array(vec![]))), "[]");
        assert_eq!(st(marshal_indent(&GoValue::Map(GoMap::new_map()))), "{}");
    }

    #[test]
    fn nil_slice_marshals_as_null() {
        // JSON writes a nil slice as `null` (the YAML encoder writes `[]`);
        // an initialized-but-empty slice stays `[]` in both.
        assert_eq!(st(marshal(&GoValue::NilSlice)), "null");
        assert_eq!(st(marshal_indent(&GoValue::NilSlice)), "null");
    }

    #[test]
    fn indent_matches_contract_two_space() {
        let mut inner = GoMap::new(MapOrigin::Map);
        inner.push("x", GoValue::Int(1));
        inner.push("y", GoValue::Int(2));
        let mut top = GoMap::new(MapOrigin::Map);
        top.push("b", GoValue::Map(inner));
        top.push("a", GoValue::Array(vec![GoValue::Int(1), GoValue::Int(2)]));
        let expected = "{\n  \"a\": [\n    1,\n    2\n  ],\n  \"b\": {\n    \"x\": 1,\n    \"y\": 2\n  }\n}";
        assert_eq!(st(marshal_indent(&GoValue::Map(top))), expected);
    }

    #[test]
    fn encoder_builders() {
        let v = GoValue::Int(7);
        assert_eq!(Encoder::marshal().encode(&v), b"7");
        assert_eq!(Encoder::compact().encode(&v), b"7");
        assert_eq!(Encoder::encoder().encode(&v), b"7\n");
        assert_eq!(Encoder::compact().with_trailing_newline(true).encode(&v), b"7\n");
        assert_eq!(
            Encoder::marshal().encode_to_string(&GoValue::Str("a<b".into())),
            r#""a\u003cb""#
        );
        // indented with trailing newline (the FORMAT_JSON path).
        let mut m = GoMap::new(MapOrigin::Map);
        m.push("a", GoValue::Int(1));
        assert_eq!(
            Encoder::indented("  ").with_trailing_newline(true).encode_to_string(&GoValue::Map(m)),
            "{\n  \"a\": 1\n}\n"
        );
    }
}
