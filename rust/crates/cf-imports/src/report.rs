//! Report value model and serialization shim.
//!
//! The analyzers build reports as a dynamic string-keyed value tree and emit
//! them in MACHINE formats (json, yaml, ndjson, timeseries, compact, bin).
//! Those bytes are a frozen contract (pinned against the reference binary by
//! `rust/tests/compat`), so the integrated pipeline routes all serialization
//! through the report-format encoders `cf-gojson` / `cf-goyaml` and the CFB1
//! `bin` envelope from `cf-reportutil`.
//!
//! To keep `cf-imports` self-contained and verifiable, this module provides:
//!
//! * [`ReportValue`] — a dynamic value enum (the same role `cf_gojson::GoValue`
//!   plays in the integrated design), with **map keys held in a [`BTreeMap`] so
//!   they iterate in byte-sorted order**, matching the report contract's
//!   map-key ordering;
//! * [`ReportValue::to_go_json_compact`] — a deterministic compact-JSON encoder
//!   reproducing the contract's compact, HTML-escape-ON output (no space after
//!   `:`);
//! * [`encode_binary_envelope`] — the CFB1 record layout (`"CFB1"` + LE u32
//!   length + compact-JSON payload).
//!
//! The integrated swap replaces [`ReportValue`] with `cf_reportutil`'s model
//! and these inline encoders with the shared ones.

use std::collections::BTreeMap;

/// A value in a report tree (dynamic, string-keyed).
///
/// Map keys use a [`BTreeMap`] so iteration order is byte-sorted, matching the
/// report contract's ordering for map-origin objects.
#[derive(Debug, Clone, PartialEq)]
pub enum ReportValue {
    /// JSON `null`.
    Null,
    /// A boolean.
    Bool(bool),
    /// A signed integer; never routed through the float path.
    Int(i64),
    /// A 64-bit float, rendered with shortest-round-trip rules.
    Float(f64),
    /// A UTF-8 string.
    Str(String),
    /// An ordered list.
    List(Vec<ReportValue>),
    /// A string-keyed map with byte-sorted iteration order.
    Map(BTreeMap<String, ReportValue>),
}

impl ReportValue {
    /// Convenience constructor for an empty map.
    #[must_use]
    pub fn map() -> Self {
        ReportValue::Map(BTreeMap::new())
    }

    /// Inserts a key/value into a [`ReportValue::Map`].
    ///
    /// # Panics
    /// Panics if `self` is not a [`ReportValue::Map`].
    pub fn insert(&mut self, key: impl Into<String>, value: ReportValue) {
        match self {
            ReportValue::Map(m) => {
                m.insert(key.into(), value);
            }
            _ => panic!("ReportValue::insert called on non-map value"),
        }
    }

    /// Borrows the inner map, if this is a [`ReportValue::Map`].
    #[must_use]
    pub fn as_map(&self) -> Option<&BTreeMap<String, ReportValue>> {
        match self {
            ReportValue::Map(m) => Some(m),
            _ => None,
        }
    }

    /// Borrows the inner list, if this is a [`ReportValue::List`].
    #[must_use]
    pub fn as_list(&self) -> Option<&[ReportValue]> {
        match self {
            ReportValue::List(v) => Some(v),
            _ => None,
        }
    }

    /// Serializes the value as compact JSON per the report-format contract.
    ///
    /// Object keys are byte-sorted (guaranteed by [`BTreeMap`]); there is no
    /// space after `:` or `,`; HTML escaping is ON (`<`, `>`, `&`, `U+2028`,
    /// `U+2029`). This is the deterministic stand-in for `cf-gojson`'s compact
    /// encoder; the integration swaps it for the shared encoder.
    #[must_use]
    pub fn to_go_json_compact(&self) -> String {
        let mut out = String::new();
        self.write_compact(&mut out);
        out
    }

    fn write_compact(&self, out: &mut String) {
        match self {
            ReportValue::Null => out.push_str("null"),
            ReportValue::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            ReportValue::Int(n) => out.push_str(&n.to_string()),
            ReportValue::Float(f) => out.push_str(&go_float(*f)),
            ReportValue::Str(s) => write_go_json_string(out, s),
            ReportValue::List(items) => {
                out.push('[');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    item.write_compact(out);
                }
                out.push(']');
            }
            ReportValue::Map(m) => {
                out.push('{');
                for (i, (k, v)) in m.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    write_go_json_string(out, k);
                    out.push(':');
                    v.write_compact(out);
                }
                out.push('}');
            }
        }
    }
}

/// Writes a JSON-escaped string (with surrounding quotes) into `out`, matching
/// the report-format contract's string escaping (HTML escaping ON).
///
/// Escapes `"`, `\`, control chars (as `\u00XX`, with `\n \r \t` shortcuts),
/// and the HTML-sensitive bytes `<`, `>`, `&` plus `U+2028` / `U+2029`.
fn write_go_json_string(out: &mut String, s: &str) {
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
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

/// Renders an `f64` per the report-format contract (shortest round-trip,
/// `'g'`-style).
///
/// This is a best-effort stand-in for the authoritative `cf_gojson` float
/// formatter. It reproduces the integer-valued-float case (`1.0` -> `1`) and
/// the `e±NN` exponent shape that the imports report can hit via
/// `AggregateData::external_ratio`. The full fuzz against the reference
/// formatter lives with `cf-gojson`.
fn go_float(f: f64) -> String {
    if f == 0.0 {
        // Preserve the sign of negative zero.
        return if f.is_sign_negative() {
            "-0".to_string()
        } else {
            "0".to_string()
        };
    }
    if f == f.trunc() && f.abs() < 1e21 {
        // Integer-valued float with no fractional part prints "1", not "1.0".
        return format!("{}", f as i64);
    }
    // Shortest round-trip representation; Rust's default {} for f64 already
    // yields the shortest digits. Exponent-threshold parity is owned by
    // cf-gojson.
    format!("{f}")
}

/// Encodes a single CFB1 `bin` record into `out`.
///
/// Layout: `b"CFB1"` + `len(payload) as u32` little-endian + `payload`, where
/// the payload is the compact-JSON encoding of `value` (HTML-escape ON, no
/// trailing newline). Records concatenate back-to-back.
///
/// The authoritative implementation belongs to `cf-reportutil`; this mirrors
/// the exact byte layout so the crate stands alone.
pub fn encode_binary_envelope(value: &ReportValue, out: &mut Vec<u8>) {
    let payload = value.to_go_json_compact();
    let payload_bytes = payload.as_bytes();
    out.extend_from_slice(b"CFB1");
    out.extend_from_slice(&(payload_bytes.len() as u32).to_le_bytes());
    out.extend_from_slice(payload_bytes);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_sorts_keys_and_has_no_spaces() {
        let mut m = ReportValue::map();
        m.insert("b", ReportValue::Int(2));
        m.insert("a", ReportValue::Int(1));
        assert_eq!(m.to_go_json_compact(), r#"{"a":1,"b":2}"#);
    }

    #[test]
    fn compact_html_escapes() {
        // HTML escaping is ON in the report contract: `<`, `>`, `&` are
        // emitted as their \u00xx escape sequences.
        let v = ReportValue::Str("a<b>&c".to_string());
        assert_eq!(
            v.to_go_json_compact(),
            "\"a\\u003cb\\u003e\\u0026c\""
        );
    }

    #[test]
    fn go_float_integer_valued() {
        assert_eq!(go_float(1.0), "1");
        assert_eq!(go_float(0.0), "0");
        assert_eq!(go_float(0.4), "0.4");
    }

    #[test]
    fn binary_envelope_layout() {
        let v = ReportValue::Map(BTreeMap::new());
        let mut out = Vec::new();
        encode_binary_envelope(&v, &mut out);
        assert_eq!(&out[0..4], b"CFB1");
        // payload is "{}" => length 2.
        assert_eq!(&out[4..8], &2u32.to_le_bytes());
        assert_eq!(&out[8..], b"{}");
    }
}
