//! Minimal Go-`encoding/json`-compatible value model and compact encoder.
//!
//! **This is a temporary in-crate shim, not the real serialization crate.**
//! DESIGN.md §2.2 mandates that all report serialization route through the shared
//! `cf-gojson` crate. At the time `cf-uast-node` was ported, `cf-gojson` was a
//! bare scaffold, so to keep this crate self-contained and unblocked (rewrite
//! rules 4 and 5) the byte-identity-critical pieces that [`crate::Node::to_map`]
//! needs are reproduced here behind an API ([`GoValue`] / [`GoMap`] /
//! [`Encoder`]) that mirrors the planned `cf-gojson` surface. When `cf-gojson`
//! lands, this module is deleted and call sites switch to it unchanged.
//!
//! What is reproduced (the parts `to_map` exercises):
//! - **Map-key byte ordering**: [`GoMap::from_map`] marks an object as
//!   map-origin; the encoder sorts its keys by `key.as_bytes()` before writing,
//!   matching Go's `map[string]any` encoding.
//! - **HTML escaping ON**: `<`, `>`, `&`, `U+2028`, `U+2029` are escaped, as Go's
//!   `encoding/json` does by default.
//! - **Compact framing**: `{"a":1,"b":2}` with no spaces (the `marshal`/compact
//!   profile; indent mode is out of scope for the `to_map` shim).
//! - **Integer formatting** for the `uint` position fields.
//!
//! What is intentionally NOT reproduced here (belongs in the real cf-gojson):
//! Go's `'g'/-1` float formatter, indent mode, and the struct-origin
//! declaration-order path. `to_map` never emits floats and only uses map-origin
//! objects + integers + strings, so the shim is sufficient and byte-faithful for
//! this crate's needs.

/// A JSON value mirroring the planned `cf-gojson::GoValue`.
#[derive(Debug, Clone, PartialEq)]
pub enum GoValue {
    /// JSON `null`.
    Null,
    /// JSON boolean.
    Bool(bool),
    /// Signed integer (never routed through the float path).
    Int(i64),
    /// Unsigned integer (never routed through the float path).
    Uint(u64),
    /// A string (HTML-escaped on encode).
    Str(String),
    /// A JSON array.
    Array(Vec<GoValue>),
    /// A JSON object (see [`GoMap`] for key-ordering semantics).
    Object(GoMap),
}

/// An ordered JSON object. A map-origin object byte-sorts its keys at encode
/// time (Go `map[string]any` semantics); a struct-origin object preserves
/// declaration order. `Node::to_map` only ever produces map-origin objects.
#[derive(Debug, Clone, PartialEq)]
pub struct GoMap {
    entries: Vec<(String, GoValue)>,
    map_origin: bool,
}

impl GoMap {
    /// Builds a **map-origin** object: its keys are byte-sorted on encode.
    pub fn from_map(entries: Vec<(String, GoValue)>) -> Self {
        GoMap { entries, map_origin: true }
    }

    /// Builds a **struct-origin** object: declaration order is preserved.
    pub fn from_struct(entries: Vec<(String, GoValue)>) -> Self {
        GoMap { entries, map_origin: false }
    }
}

/// A compact, HTML-escaping encoder mirroring the planned `cf-gojson::Encoder`.
#[derive(Debug, Clone, Copy)]
pub struct Encoder {
    escape_html: bool,
}

impl Encoder {
    /// The `json.Marshal` profile: compact, HTML-escape on, no trailing newline.
    pub fn marshal() -> Self {
        Encoder { escape_html: true }
    }

    /// Encodes `value` to a `String` (UTF-8; all bytes produced are ASCII-safe
    /// after escaping).
    pub fn encode_to_string(&self, value: &GoValue) -> String {
        let mut out = String::new();
        self.write_value(&mut out, value);
        out
    }

    fn write_value(&self, out: &mut String, value: &GoValue) {
        match value {
            GoValue::Null => out.push_str("null"),
            GoValue::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            GoValue::Int(i) => out.push_str(&i.to_string()),
            GoValue::Uint(u) => out.push_str(&u.to_string()),
            GoValue::Str(s) => self.write_string(out, s),
            GoValue::Array(items) => {
                out.push('[');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    self.write_value(out, item);
                }
                out.push(']');
            }
            GoValue::Object(map) => self.write_object(out, map),
        }
    }

    fn write_object(&self, out: &mut String, map: &GoMap) {
        out.push('{');
        // Determine emission order: map-origin sorts by raw key bytes.
        let mut order: Vec<usize> = (0..map.entries.len()).collect();
        if map.map_origin {
            order.sort_by(|&a, &b| map.entries[a].0.as_bytes().cmp(map.entries[b].0.as_bytes()));
        }
        for (i, &idx) in order.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            let (k, v) = &map.entries[idx];
            self.write_string(out, k);
            out.push(':');
            self.write_value(out, v);
        }
        out.push('}');
    }

    /// Writes a JSON string reproducing Go's `encodeState.string` escaping
    /// (quotes, backslash, control chars as `\u00XX` with `\n \r \t` shortcuts,
    /// and — when `escape_html` — `<`, `>`, `&`, `U+2028`, `U+2029`).
    fn write_string(&self, out: &mut String, s: &str) {
        out.push('"');
        for ch in s.chars() {
            match ch {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                '<' if self.escape_html => out.push_str("\\u003c"),
                '>' if self.escape_html => out.push_str("\\u003e"),
                '&' if self.escape_html => out.push_str("\\u0026"),
                '\u{2028}' if self.escape_html => out.push_str("\\u2028"),
                '\u{2029}' if self.escape_html => out.push_str("\\u2029"),
                c if (c as u32) < 0x20 => {
                    out.push_str(&format!("\\u{:04x}", c as u32));
                }
                c => out.push(c),
            }
        }
        out.push('"');
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_keys_byte_sorted() {
        let m = GoMap::from_map(vec![
            ("type".into(), GoValue::Str("F".into())),
            ("children".into(), GoValue::Array(vec![])),
            ("id".into(), GoValue::Str("x".into())),
        ]);
        let s = Encoder::marshal().encode_to_string(&GoValue::Object(m));
        assert_eq!(s, r#"{"children":[],"id":"x","type":"F"}"#);
    }

    #[test]
    fn struct_keys_keep_order() {
        let m = GoMap::from_struct(vec![
            ("type".into(), GoValue::Str("F".into())),
            ("id".into(), GoValue::Str("x".into())),
        ]);
        let s = Encoder::marshal().encode_to_string(&GoValue::Object(m));
        assert_eq!(s, r#"{"type":"F","id":"x"}"#);
    }

    #[test]
    fn html_escaping_on() {
        // Go's encoding/json escapes `<`, `>`, `&` as `<`, `>`,
        // `&` by default (DESIGN §2.2 — HTML escaping ON). The escaped bytes
        // are the byte-correct output, not the literal characters.
        let s = Encoder::marshal().encode_to_string(&GoValue::Str("a<b>&c".into()));
        assert_eq!(s, r#""a\u003cb\u003e\u0026c""#);
    }

    #[test]
    fn control_and_shortcut_escapes() {
        let s = Encoder::marshal().encode_to_string(&GoValue::Str("a\nb\t\u{0001}".into()));
        assert_eq!(s, r#""a\nb\t\u0001""#);
    }

    #[test]
    fn integers_and_bools() {
        assert_eq!(Encoder::marshal().encode_to_string(&GoValue::Uint(42)), "42");
        assert_eq!(Encoder::marshal().encode_to_string(&GoValue::Int(-7)), "-7");
        assert_eq!(Encoder::marshal().encode_to_string(&GoValue::Bool(true)), "true");
        assert_eq!(Encoder::marshal().encode_to_string(&GoValue::Null), "null");
    }
}
