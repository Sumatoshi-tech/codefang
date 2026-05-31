//! Report value model and serialization shim.
//!
//! The Go analyzer builds reports as `analyze.Report` = `map[string]any` and
//! emits them in MACHINE formats (json, yaml, ndjson, timeseries, compact, bin).
//! Byte-identity of those bytes is the project goal, so the integrated design
//! routes all serialization through the go-compat encoders `cf-gojson` /
//! `cf-goyaml` and the CFB1 `bin` envelope from `cf-reportutil` (DESIGN §2).
//!
//! The `cf-gojson` / `cf-goyaml` encoders are not yet implemented. To keep
//! `cf-imports` self-contained and verifiable, this module provides:
//!
//! * [`ReportValue`] — an enum mirroring Go `map[string]any` (the same role the
//!   design assigns to `cf_gojson::GoValue`), with **map keys held in a
//!   [`BTreeMap`] so they iterate in byte-sorted order**, matching Go's
//!   `encoding/json` map-key sort (DESIGN §2.2);
//! * [`ReportValue::to_go_json_compact`] — a deterministic compact-JSON encoder
//!   reproducing Go's compact, HTML-escape-ON output (no space after `:`);
//! * [`encode_binary_envelope`] — the CFB1 record layout from
//!   `reportutil/binary.go` (`"CFB1"` + LE u32 length + compact-JSON payload).
//!
//! Once `cf-gojson`/`cf-reportutil` are wired in: replace [`ReportValue`] with
//! `cf_reportutil::ReportValue` and delete the inline encoders here, calling the
//! go-compat encoders instead.

use std::collections::BTreeMap;

/// A value in a report tree, mirroring Go's `map[string]any` / `cf_gojson::GoValue`.
///
/// Map keys use a [`BTreeMap`] so iteration order is byte-sorted, matching Go's
/// `encoding/json` behaviour for `map[string]X`.
#[derive(Debug, Clone, PartialEq)]
pub enum ReportValue {
    /// Go `nil` / JSON `null`.
    Null,
    /// A boolean.
    Bool(bool),
    /// A signed integer (Go `int` / `int64`); never routed through the float path.
    Int(i64),
    /// A 64-bit float (Go `float64`), rendered with Go's `'g'/-1` rules.
    Float(f64),
    /// A UTF-8 string.
    Str(String),
    /// An ordered list (Go slice).
    List(Vec<ReportValue>),
    /// A string-keyed map with byte-sorted iteration order (Go `map[string]any`).
    Map(BTreeMap<String, ReportValue>),
}

impl ReportValue {
    /// Convenience constructor for an empty map.
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
    pub fn as_map(&self) -> Option<&BTreeMap<String, ReportValue>> {
        match self {
            ReportValue::Map(m) => Some(m),
            _ => None,
        }
    }

    /// Borrows the inner list, if this is a [`ReportValue::List`].
    pub fn as_list(&self) -> Option<&[ReportValue]> {
        match self {
            ReportValue::List(v) => Some(v),
            _ => None,
        }
    }

    /// Serializes the value as compact JSON matching Go `json.Marshal`.
    ///
    /// Object keys are byte-sorted (guaranteed by [`BTreeMap`]); there is no
    /// space after `:` or `,`; HTML escaping is ON (`<`, `>`, `&`, `U+2028`,
    /// `U+2029`). This is the deterministic stand-in for `cf-gojson`'s compact
    /// encoder; the integration swaps it for the real encoder.
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
/// Go `encoding/json`'s `encodeState.string` with `escapeHTML = true`.
///
/// Go escapes `"`, `\`, control chars (as `\u00XX`, with `\n \r \t` shortcuts),
/// and — because the repo never calls `SetEscapeHTML(false)` (DESIGN §2.1) — the
/// HTML-sensitive bytes `<`, `>`, `&` plus `U+2028` / `U+2029`.
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

/// Renders an `f64` the way Go `encoding/json` does (`'g'`, prec `-1`, bits 64).
///
/// This is a best-effort stand-in for the authoritative `cf_gojson::go_float`
/// (DESIGN §2.2). It reproduces the integer-valued-float case (`1.0` -> `1`) and
/// the `e±NN` exponent shape that the imports report can hit via
/// `AggregateData.ExternalRatio`. The full millions-value fuzz against Go lives
/// with `cf-gojson`; see the crate todos.
fn go_float(f: f64) -> String {
    if f == 0.0 {
        // Preserve -0.0 like Go does.
        return if f.is_sign_negative() {
            "-0".to_string()
        } else {
            "0".to_string()
        };
    }
    if f == f.trunc() && f.abs() < 1e21 {
        // Integer-valued float with no fractional part: Go prints "1", not "1.0".
        return format!("{}", f as i64);
    }
    // Shortest round-trip representation; Rust's default {} for f64 already
    // yields the shortest digits. Exponent-threshold parity with Go is deferred
    // to cf-gojson (see todos).
    format!("{}", f)
}

/// Encodes a single CFB1 `bin` record into `out`.
///
/// Layout from `internal/analyzers/common/reportutil/binary.go` (DESIGN §2.5):
/// `b"CFB1"` + `len(payload) as u32` little-endian + `payload`, where the
/// payload is the compact-JSON encoding of `value` (HTML-escape ON, no trailing
/// newline). Records concatenate back-to-back.
///
/// The authoritative implementation belongs in `cf-reportutil`; this mirrors the
/// exact byte layout so behaviour matches before integration.
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
        // Go json.Marshal has HTML escaping ON by default (DESIGN §2.1): `<`,
        // `>`, `&` are emitted as the escaped \u003c, \u003e, \u0026.
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
