//! Report-compatible JSON value model and encoder for tool results.
//!
//! This encoder reproduces the report-format serialization contract: the
//! output bytes are pinned against the reference implementation
//! (rust/tests/compat). Its rules are frozen — tidy internals only, never the
//! emitted bytes.
//!
//! **Temporary in-crate shim, not the real serialization crate.** DESIGN.md
//! §2.2 mandates that all report serialization route through the shared
//! `cf-gojson` crate, but `cf-gojson` does not yet expose a public indent
//! encoder, and the workspace's only implemented report-byte-compatible value
//! model (`cf-uast-node`'s) keeps its map entries private and exposes only a
//! *compact* encoder. The MCP tool output uses the *indent* profile over whole
//! trees built from node maps plus analyzer report maps, so this module
//! defines a small self-contained value model ([`JsonValue`]) and a faithful
//! [`Encoder`] (compact + indent). When `cf-gojson` lands with a public indent
//! `Encoder` and an inspectable `GoValue`, delete this file and re-point
//! [`crate::result`] to it (DESIGN rule 5).
//!
//! The frozen rules (the parts the MCP tool output exercises):
//! - **Map-key byte ordering** for map-origin objects (string-keyed maps
//!   serialize in byte-sorted key order).
//! - **Struct-origin** objects keep declaration order and honor
//!   omit-when-empty (handled by the builder simply not inserting omitted
//!   fields).
//! - **HTML escaping ON** (`<`, `>`, `&`, `U+2028`, `U+2029`) — the reference
//!   encoder's default, never disabled anywhere in the report surface.
//! - **Compact vs indent framing**: compact `{"a":1,"b":2}` (no spaces); indent
//!   `{\n  "a": 1\n}` with one space after the colon and empty containers
//!   collapsed to `{}` / `[]`.
//! - **No trailing newline** in either profile.
//! - **Float formatting** via the reference shortest-round-trip (`'g'/-1`)
//!   rules for the analyzer-report floats that flow through `tools/call`
//!   (DESIGN §2.2).

use std::collections::BTreeMap;

/// A JSON value mirroring the planned `cf-gojson::GoValue`.
///
/// Objects carry their origin: [`JsonValue::sorted_object`] (map-origin,
/// byte-sorted on encode) vs [`JsonValue::struct_object`] (declaration order
/// preserved).
#[derive(Debug, Clone, PartialEq)]
pub enum JsonValue {
    /// JSON `null`.
    Null,
    /// JSON boolean.
    Bool(bool),
    /// Signed integer (never routed through the float path).
    Int(i64),
    /// Unsigned integer (never routed through the float path).
    Uint(u64),
    /// IEEE-754 double, rendered via the frozen `'g'/-1` report rules.
    Float(f64),
    /// A string (HTML-escaped on encode).
    Str(String),
    /// A JSON array.
    Array(Vec<JsonValue>),
    /// A JSON object. The `sorted` flag selects byte-sort (map-origin) vs
    /// declaration-order (struct-origin) emission.
    Object {
        /// The (key, value) entries in declaration order.
        entries: Vec<(String, JsonValue)>,
        /// Whether keys are byte-sorted on encode (map-origin).
        sorted: bool,
    },
}

impl JsonValue {
    /// Builds a **map-origin** object (byte-sorted keys on encode).
    #[must_use]
    pub fn sorted_object(entries: Vec<(String, JsonValue)>) -> Self {
        Self::Object { entries, sorted: true }
    }

    /// Builds a **struct-origin** object (declaration order preserved).
    #[must_use]
    pub fn struct_object(entries: Vec<(String, JsonValue)>) -> Self {
        Self::Object { entries, sorted: false }
    }
}

/// Converts a [`cf_uast_node::Node`] subtree directly into a [`JsonValue`],
/// reproducing the node-map report shape byte-for-byte (DESIGN §2.2).
///
/// This bypasses the opaque `GoMap` by reading the node's public fields. The full
/// key set is `children, id, pos, props, roles, token, type` and is emitted as a
/// map-origin (byte-sorted) object, matching `tomap.rs` in `cf-uast-node`:
/// - `type` always present;
/// - `id` only when non-empty, lowercase-hex-encoded;
/// - `token` only when non-empty;
/// - `props` only when non-empty (its keys byte-sort too);
/// - `roles` always present (possibly empty array);
/// - `pos` always present (zeros when absent);
/// - `children` only when present.
#[must_use]
pub fn node_to_json(node: &cf_uast_node::Node) -> JsonValue {
    let mut entries: Vec<(String, JsonValue)> = Vec::with_capacity(7);

    entries.push(("type".to_string(), JsonValue::Str(node.node_type.clone())));

    if !node.id.is_empty() {
        use std::fmt::Write as _;
        let mut hex = String::with_capacity(node.id.len() * 2);
        for b in &node.id {
            let _ = write!(hex, "{b:02x}");
        }
        entries.push(("id".to_string(), JsonValue::Str(hex)));
    }

    if !node.token.is_empty() {
        entries.push(("token".to_string(), JsonValue::Str(node.token.clone())));
    }

    if !node.props.is_empty() {
        // props is map-origin → byte-sorted; BTreeMap gives the sorted order.
        let sorted: BTreeMap<&String, &String> = node.props.iter().collect();
        let prop_entries: Vec<(String, JsonValue)> = sorted
            .into_iter()
            .map(|(k, v)| (k.clone(), JsonValue::Str(v.clone())))
            .collect();
        entries.push(("props".to_string(), JsonValue::sorted_object(prop_entries)));
    }

    let roles: Vec<JsonValue> = node.roles.iter().map(|r| JsonValue::Str(r.clone())).collect();
    entries.push(("roles".to_string(), JsonValue::Array(roles)));

    entries.push(("pos".to_string(), position_json(node.pos.as_ref())));

    if !node.children.is_empty() {
        let children: Vec<JsonValue> = node.children.iter().map(node_to_json).collect();
        entries.push(("children".to_string(), JsonValue::Array(children)));
    }

    JsonValue::sorted_object(entries)
}

/// Builds the position sub-object. Mirrors `cf-uast-node`'s `position_map`: nil
/// position yields all-zero fields; the six keys are always present.
fn position_json(pos: Option<&cf_uast_node::Positions>) -> JsonValue {
    let p = pos.copied().unwrap_or_default();
    JsonValue::sorted_object(vec![
        ("start_line".to_string(), JsonValue::Uint(p.start_line)),
        ("start_col".to_string(), JsonValue::Uint(p.start_col)),
        ("start_offset".to_string(), JsonValue::Uint(p.start_offset)),
        ("end_line".to_string(), JsonValue::Uint(p.end_line)),
        ("end_col".to_string(), JsonValue::Uint(p.end_col)),
        ("end_offset".to_string(), JsonValue::Uint(p.end_offset)),
    ])
}

/// Encoder mode: the two report-format profiles this crate needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// Compact profile: no spaces, e.g. `{"a":1}`.
    Compact,
    /// Indent profile: newlines + `indent` per level and one space after each
    /// colon.
    Indent,
}

/// A report-format-compatible encoder over [`JsonValue`].
#[derive(Debug, Clone)]
pub struct Encoder {
    mode: Mode,
    indent: String,
}

impl Encoder {
    /// The compact profile: no spaces, HTML-escape on, no trailing newline.
    #[must_use]
    pub fn compact() -> Self {
        Self { mode: Mode::Compact, indent: String::new() }
    }

    /// The indent profile: indented, HTML-escape on, no trailing newline. The
    /// tool results use `indent = "  "` (two spaces).
    #[must_use]
    pub fn indented(indent: &str) -> Self {
        Self { mode: Mode::Indent, indent: indent.to_string() }
    }

    /// Encodes `value` to a UTF-8 byte vector. No trailing newline is appended.
    #[must_use]
    pub fn encode(&self, value: &JsonValue) -> Vec<u8> {
        self.encode_to_string(value).into_bytes()
    }

    /// Encodes `value` to a `String`.
    #[must_use]
    pub fn encode_to_string(&self, value: &JsonValue) -> String {
        let mut out = String::new();
        self.write_value(&mut out, value, 0);
        out
    }

    fn write_value(&self, out: &mut String, value: &JsonValue, depth: usize) {
        match value {
            JsonValue::Null => out.push_str("null"),
            JsonValue::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            JsonValue::Int(i) => out.push_str(&i.to_string()),
            JsonValue::Uint(u) => out.push_str(&u.to_string()),
            JsonValue::Float(f) => out.push_str(&format_go_float(*f)),
            JsonValue::Str(s) => write_string(out, s),
            JsonValue::Array(items) => self.write_array(out, items, depth),
            JsonValue::Object { entries, sorted } => {
                self.write_object(out, entries, *sorted, depth);
            }
        }
    }

    fn write_array(&self, out: &mut String, items: &[JsonValue], depth: usize) {
        if items.is_empty() {
            out.push_str("[]");
            return;
        }
        out.push('[');
        for (i, item) in items.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            self.newline_indent(out, depth + 1);
            self.write_value(out, item, depth + 1);
        }
        self.newline_indent(out, depth);
        out.push(']');
    }

    fn write_object(
        &self,
        out: &mut String,
        entries: &[(String, JsonValue)],
        sorted: bool,
        depth: usize,
    ) {
        if entries.is_empty() {
            out.push_str("{}");
            return;
        }
        let mut order: Vec<usize> = (0..entries.len()).collect();
        if sorted {
            order.sort_by(|&a, &b| entries[a].0.as_bytes().cmp(entries[b].0.as_bytes()));
        }
        out.push('{');
        for (i, &idx) in order.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            self.newline_indent(out, depth + 1);
            let (k, v) = &entries[idx];
            write_string(out, k);
            out.push(':');
            if self.mode == Mode::Indent {
                out.push(' ');
            }
            self.write_value(out, v, depth + 1);
        }
        self.newline_indent(out, depth);
        out.push('}');
    }

    fn newline_indent(&self, out: &mut String, depth: usize) {
        if self.mode == Mode::Indent {
            out.push('\n');
            for _ in 0..depth {
                out.push_str(&self.indent);
            }
        }
    }
}

/// Writes a JSON string with the frozen report escaping rules: quotes,
/// backslash, control chars as `\u00XX` with `\n \r \t` shortcuts, plus HTML
/// and line/paragraph separators — escaping is always ON.
fn write_string(out: &mut String, s: &str) {
    out.push('"');
    for ch in s.chars() {
        match ch {
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
            c if (c as u32) < 0x20 => {
                use std::fmt::Write as _;
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// Renders an `f64` with the frozen report float-formatting rules (shortest
/// round-trip, `'g'/-1` style, with the JSON exponent-threshold tweak):
/// integer-valued floats print without a decimal point, the exponential form
/// is chosen when `exp < -4 || exp >= 21`, and the exponent is rendered `e±NN`
/// with a sign and ≥2 digits.
///
/// This is a pragmatic subset (DESIGN §2.2 calls for the full millions-value
/// fuzz against the real `cf-gojson::go_float`). It is correct for the common
/// finite values analyzer reports emit; full edge-case parity moves to
/// `cf-gojson`. Non-finite values are rendered as `null` defensively (the
/// reference encoder errors out; reports never contain NaN/Inf).
#[must_use]
pub fn format_go_float(f: f64) -> String {
    if !f.is_finite() {
        return "null".to_string();
    }
    if f == 0.0 {
        // Both 0.0 and -0.0 print as "0" (frozen report rule).
        return "0".to_string();
    }
    // Use Rust's shortest round-trip ("{}") to get the unique digit sequence,
    // then adjust to the contract's exponent thresholds and rendering.
    let shortest = format!("{f}");
    go_style_from_shortest(&shortest, f)
}

/// Adjusts Rust's shortest representation to the frozen `'g'/-1` rendering.
fn go_style_from_shortest(shortest: &str, f: f64) -> String {
    // Rust never emits exponent for moderate magnitudes and prints e.g. "1" for
    // 1.0, which already matches the contract for the common case. The
    // remaining divergence is the exponential threshold and the `e±NN`
    // exponent shape.
    let abs = f.abs();
    // Decimal exponent of the value (floor(log10(abs))).
    let exp = abs.log10().floor() as i32;
    // Frozen JSON float threshold: exponential when exp < -4 || exp >= 21.
    // Written as the two explicit bounds (not a `!Range::contains`) to mirror
    // the contract condition verbatim.
    #[allow(clippy::manual_range_contains)]
    let use_exp = exp < -4 || exp >= 21;

    if !use_exp {
        // Rust may itself choose exponent for very small/large; force plain.
        if shortest.contains('e') || shortest.contains('E') {
            return format!("{f:.0}");
        }
        return shortest.to_string();
    }

    // Build mantissa + contract-style exponent.
    let mut mantissa = abs;
    let mut e = 0i32;
    while mantissa >= 10.0 {
        mantissa /= 10.0;
        e += 1;
    }
    while mantissa < 1.0 {
        mantissa *= 10.0;
        e -= 1;
    }
    let mant_str = {
        let s = format!("{mantissa}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    };
    let sign = if e >= 0 { '+' } else { '-' };
    let sign_prefix = if f < 0.0 { "-" } else { "" };
    format!("{sign_prefix}{mant_str}e{sign}{:02}", e.abs())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn smap(entries: &[(&str, JsonValue)]) -> JsonValue {
        JsonValue::sorted_object(
            entries.iter().map(|(k, v)| ((*k).to_string(), v.clone())).collect(),
        )
    }

    #[test]
    fn compact_object_byte_sorts_keys() {
        let v = smap(&[
            ("type", JsonValue::Str("F".into())),
            ("children", JsonValue::Array(vec![])),
            ("id", JsonValue::Str("x".into())),
        ]);
        assert_eq!(
            Encoder::compact().encode_to_string(&v),
            r#"{"children":[],"id":"x","type":"F"}"#
        );
    }

    #[test]
    fn indent_two_space_no_trailing_newline() {
        let v = smap(&[("a", JsonValue::Int(1)), ("b", JsonValue::Int(2))]);
        let s = Encoder::indented("  ").encode_to_string(&v);
        assert_eq!(s, "{\n  \"a\": 1,\n  \"b\": 2\n}");
        assert!(!s.ends_with('\n'));
    }

    #[test]
    fn indent_empty_containers_collapse() {
        let v = smap(&[
            ("arr", JsonValue::Array(vec![])),
            ("obj", JsonValue::sorted_object(vec![])),
        ]);
        assert_eq!(
            Encoder::indented("  ").encode_to_string(&v),
            "{\n  \"arr\": [],\n  \"obj\": {}\n}"
        );
    }

    #[test]
    fn html_escaping_on() {
        // The report contract escapes <, >, & (HTML-escape ON, never disabled).
        // DESIGN §2.1. So "a<b>&c" encodes with < / > / & escapes.
        let v = JsonValue::Str("a<b>&c".into());
        assert_eq!(
            Encoder::compact().encode_to_string(&v),
            "\"a\\u003cb\\u003e\\u0026c\""
        );
    }

    #[test]
    fn struct_object_keeps_declaration_order() {
        let v = JsonValue::struct_object(vec![
            ("type".to_string(), JsonValue::Str("F".into())),
            ("id".to_string(), JsonValue::Str("x".into())),
        ]);
        assert_eq!(
            Encoder::compact().encode_to_string(&v),
            r#"{"type":"F","id":"x"}"#
        );
    }

    #[test]
    fn node_to_json_matches_node_map_key_order() {
        // Reproduces cf-uast-node's to_map_basic_fields expectation.
        let n = cf_uast_node::Builder::new()
            .with_type("Function")
            .with_token("foo")
            .with_roles(vec!["Declaration".into(), "Name".into()])
            .with_position(Some(cf_uast_node::Positions {
                start_line: 1,
                start_col: 2,
                start_offset: 3,
                end_line: 4,
                end_col: 5,
                end_offset: 6,
            }))
            .build();
        let s = Encoder::compact().encode_to_string(&node_to_json(&n));
        assert_eq!(
            s,
            r#"{"pos":{"end_col":5,"end_line":4,"end_offset":6,"start_col":2,"start_line":1,"start_offset":3},"roles":["Declaration","Name"],"token":"foo","type":"Function"}"#
        );
    }

    #[test]
    fn node_to_json_hex_encodes_id_and_sorts_props() {
        let mut props = std::collections::HashMap::new();
        props.insert("b".to_string(), "2".to_string());
        props.insert("a".to_string(), "1".to_string());
        let mut n = cf_uast_node::Builder::new()
            .with_type("Function")
            .with_props(props)
            .build();
        n.id = vec![0xde, 0xad];
        let s = Encoder::compact().encode_to_string(&node_to_json(&n));
        assert!(s.contains(r#""id":"dead""#));
        assert!(s.contains(r#""props":{"a":"1","b":"2"}"#));
    }

    #[test]
    fn float_integer_valued_has_no_decimal() {
        assert_eq!(format_go_float(1.0), "1");
        assert_eq!(format_go_float(10.0), "10");
    }

    #[test]
    fn float_small_decimal() {
        assert_eq!(format_go_float(0.5), "0.5");
        assert_eq!(format_go_float(0.8), "0.8");
    }

    #[test]
    fn float_zero() {
        assert_eq!(format_go_float(0.0), "0");
    }
}
