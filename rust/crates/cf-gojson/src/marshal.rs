//! Go `encoding/json` byte-parity marshaller over [`GoValue`].
//!
//! This is the keystone the whole report layer routes through. It reproduces Go's
//! `encoding/json` defaults exactly so machine-format report bytes match the Go
//! binary byte-for-byte (see `specs/rust-rewrite/DESIGN.md` §2):
//!
//! * **HTML escaping ON** — `<`, `>`, `&` become `<`, `>`, `&`,
//!   and `U+2028`/`U+2029` become ` `/` `, matching `json.Marshal`'s
//!   default (and `json.Encoder` without `SetEscapeHTML(false)`).
//! * **map keys byte-sorted**, struct fields in declaration order — decided by
//!   [`GoMap::encode_order`] via the value's [`crate::MapOrigin`].
//! * **floats** via [`crate::ftoa::format_json_float`] (Go's `encoding/json`
//!   float encoder), never Rust's `Display`.
//! * **compact** output ([`marshal`]) has no insignificant whitespace.
//! * **indented** output ([`marshal_indent`]) mirrors
//!   `json.Encoder.SetIndent("", "  ")` (two-space indent, `": "` after keys,
//!   one element per line).
//!
//! Two entry styles are provided:
//!
//! * free functions [`marshal`] / [`marshal_indent`] (and `to_vec` aliases) for
//!   the `json.Marshal` shape — compact/indented, **no** trailing newline; and
//! * the [`Encoder`] builder for the `json.NewEncoder` shape — configurable
//!   indent and an optional trailing `\n` (which `Encoder.Encode` appends).
//!
//! [`GoMap`]: crate::value::GoMap
//! [`GoMap::encode_order`]: crate::value::GoMap::encode_order

use crate::ftoa::format_json_float;
use crate::value::GoValue;

/// The two-space indent unit Go's `SetIndent("", "  ")` uses by default.
const DEFAULT_INDENT: &str = "  ";

/// Encodes `value` as compact Go-JSON bytes (mirrors `json.Marshal`).
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

/// Encodes `value` as two-space-indented Go-JSON bytes.
///
/// Mirrors `json.Encoder` with `SetIndent("", "  ")` **except** for the trailing
/// newline: this function emits none, so callers add the newline where the
/// golden expects it (or use [`Encoder::indented`] with
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

/// Formats `f` exactly as Go's `encoding/json` encodes a JSON number.
///
/// Re-exported convenience wrapper around [`crate::ftoa::format_json_float`];
/// this is the function the golden harness fuzzes against Go.
#[must_use]
pub fn go_float(f: f64) -> String {
    format_json_float(f)
}

/// Owned, builder-style Go-JSON encoder mirroring `encoding/json`'s `Encoder`.
///
/// Construct with one of the named constructors and (optionally)
/// [`with_trailing_newline`](Encoder::with_trailing_newline), then call
/// [`encode`](Encoder::encode) / [`encode_to_vec`](Encoder::encode_to_vec) /
/// [`encode_to_string`](Encoder::encode_to_string). HTML escaping is always on
/// (Go's default); there is intentionally no toggle because every machine-format
/// report path in codefang uses the default.
///
/// | constructor | shape | indent | trailing `\n` |
/// | --- | --- | --- | --- |
/// | [`marshal`](Encoder::marshal) | `json.Marshal` | none | no |
/// | [`compact`](Encoder::compact) | compact | none | no |
/// | [`encoder`](Encoder::encoder) | `json.NewEncoder` | none | **yes** |
/// | [`indented`](Encoder::indented) | `SetIndent("","…")` | given | no |
#[derive(Debug, Clone)]
pub struct Encoder {
    indent: Option<String>,
    trailing_newline: bool,
}

impl Default for Encoder {
    fn default() -> Self {
        Encoder {
            indent: None,
            trailing_newline: false,
        }
    }
}

impl Encoder {
    /// Compact encoder with no trailing newline — the `json.Marshal` shape.
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

    /// Compact encoder that appends a trailing `\n` — the `json.NewEncoder`
    /// shape (`Encoder.Encode` writes one newline per value).
    #[must_use]
    pub fn encoder() -> Self {
        Encoder {
            indent: None,
            trailing_newline: true,
        }
    }

    /// Indented encoder using `indent` as the per-level unit (`"  "` reproduces
    /// `SetIndent("", "  ")`). No trailing newline unless
    /// [`with_trailing_newline`](Encoder::with_trailing_newline) is set.
    #[must_use]
    pub fn indented(indent: &str) -> Self {
        Encoder {
            indent: Some(indent.to_string()),
            trailing_newline: false,
        }
    }

    /// Returns a copy of this encoder with the trailing-newline behavior set.
    #[must_use]
    pub fn with_trailing_newline(mut self, on: bool) -> Self {
        self.trailing_newline = on;
        self
    }

    /// Encodes `value`, returning the bytes (infallible for finite numbers).
    #[must_use]
    pub fn encode(&self, value: &GoValue) -> Vec<u8> {
        let mut buf = Vec::new();
        match &self.indent {
            Some(ind) => write_indented(&mut buf, value, ind, 0),
            None => write_compact(&mut buf, value),
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
        String::from_utf8(self.encode(value)).expect("Go-JSON output is valid UTF-8")
    }
}

/// Writes `value` to `out` in compact form.
fn write_compact(out: &mut Vec<u8>, value: &GoValue) {
    match value {
        // A nil slice marshals as `null` in `encoding/json` (the YAML encoder
        // renders it `[]` instead).
        GoValue::Null | GoValue::NilSlice => out.extend_from_slice(b"null"),
        GoValue::Bool(true) => out.extend_from_slice(b"true"),
        GoValue::Bool(false) => out.extend_from_slice(b"false"),
        GoValue::Int(i) => out.extend_from_slice(i.to_string().as_bytes()),
        GoValue::Uint(u) => out.extend_from_slice(u.to_string().as_bytes()),
        GoValue::Float(f) => out.extend_from_slice(format_json_float(*f).as_bytes()),
        GoValue::Str(s) => write_go_json_string(out, s),
        GoValue::Array(items) => {
            out.push(b'[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(b',');
                }
                write_compact(out, item);
            }
            out.push(b']');
        }
        GoValue::Map(m) => {
            out.push(b'{');
            for (i, (k, v)) in m.encode_order().iter().enumerate() {
                if i > 0 {
                    out.push(b',');
                }
                write_go_json_string(out, k);
                out.push(b':');
                write_compact(out, v);
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
/// Matches Go's indented encoder: empty objects/arrays stay `{}`/`[]` on one
/// line; non-empty containers put one element per line, a `": "` separator after
/// object keys, and the closing bracket at the parent indent. Scalars are
/// identical to the compact form.
fn write_indented(out: &mut Vec<u8>, value: &GoValue, unit: &str, depth: usize) {
    match value {
        GoValue::Array(items) if !items.is_empty() => {
            out.extend_from_slice(b"[\n");
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.extend_from_slice(b",\n");
                }
                write_indent(out, unit, depth + 1);
                write_indented(out, item, unit, depth + 1);
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
                write_go_json_string(out, k);
                out.extend_from_slice(b": ");
                write_indented(out, v, unit, depth + 1);
            }
            out.push(b'\n');
            write_indent(out, unit, depth);
            out.push(b'}');
        }
        // Empty containers and all scalars render exactly like the compact form.
        other => write_compact(out, other),
    }
}

/// Writes `s` as a Go `encoding/json` quoted string with HTML escaping on.
///
/// Byte-for-byte reproduction of `encoding/json`'s `encodeState.string` with the
/// default `escapeHTML=true`:
///
/// * `"` → `\"`, `\` → `\\`;
/// * `\n` → `\n`, `\r` → `\r`, `\t` → `\t`;
/// * `<` → `<`, `>` → `>`, `&` → `&` (HTML safety);
/// * other control bytes `< 0x20` → `\u00xx` (lowercase hex), so `\b`/`\f`
///   become ``/`` exactly as Go emits them (Go uses **no** `\b`/`\f`
///   shortcuts);
/// * `U+2028`/`U+2029` → ` `/` ` (the JS line/paragraph separators Go
///   escapes for the same browser-safety reason).
///
/// Rust `&str` is always valid UTF-8, so Go's invalid-rune `�` path is
/// unreachable here.
pub fn write_go_json_string(out: &mut Vec<u8>, s: &str) {
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
            // Go's encoding/json emits the short escapes \b (0x08) and \f (0x0c),
            // not the generic  /  forms (verified against json.Marshal).
            '\u{0008}' => (Some(b"\\b"), 1),
            '\u{000c}' => (Some(b"\\f"), 1),
            '<' => (Some(b"\\u003c"), 1),
            '>' => (Some(b"\\u003e"), 1),
            '&' => (Some(b"\\u0026"), 1),
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
        // Go HTML-escapes <, >, & to < > & by default.
        assert_eq!(
            st(marshal(&GoValue::Str("a<b>c&d".into()))),
            r#""a\u003cb\u003ec\u0026d""#
        );
        assert_eq!(st(marshal(&GoValue::Str("\"q\"".into()))), r#""\"q\"""#);
        assert_eq!(st(marshal(&GoValue::Str("a\\b".into()))), r#""a\\b""#);
        assert_eq!(st(marshal(&GoValue::Str("x\ny\tz".into()))), r#""x\ny\tz""#);
        // 0x08 and 0x0c use the short escapes \b and \f (Go json.Marshal).
        assert_eq!(st(marshal(&GoValue::Str("\u{0008}\u{000c}".into()))), r#""\b\f""#);
        // line/paragraph separators.
        assert_eq!(st(marshal(&GoValue::Str("\u{2028}\u{2029}".into()))), r#""\u2028\u2029""#);
        // forward slash is NOT escaped by Go.
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
        // `encoding/json` writes a nil slice as `null` (the YAML encoder writes
        // `[]`); an initialized-but-empty slice stays `[]` in both.
        assert_eq!(st(marshal(&GoValue::NilSlice)), "null");
        assert_eq!(st(marshal_indent(&GoValue::NilSlice)), "null");
    }

    #[test]
    fn indent_matches_go_two_space() {
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
