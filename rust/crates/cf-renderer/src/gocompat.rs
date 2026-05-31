//! Minimal Go `encoding/json`-byte-compatible encoder, shaped after the
//! design's tier-0 `cf-gojson` crate (DESIGN.md sec 2.2).
//!
//! Per the design, machine-format report bytes must be byte-identical to Go's
//! `encoding/json`, which differs from `serde_json` on four points: map-key
//! ordering, HTML escaping (on by default), float formatting
//! (`strconv.AppendFloat(_, 'g', -1, 64)` with the `21` exponent threshold),
//! and compact-vs-indent semantics. `cf-gojson` is still a scaffold in this
//! workspace, so this module provides the subset the renderer needs with the
//! same [`GoValue`]/[`Encoder`] API shape. When `cf-gojson` lands, delete this
//! module and depend on it (see this crate's `Cargo.toml`).
//!
//! Scope ported here (sufficient for the renderer's JSON models):
//! - struct-origin objects emit fields in declaration order, honoring
//!   `omitempty` (modeled by callers choosing which entries to push);
//! - map-origin objects byte-sort their keys;
//! - HTML escaping of `<`, `>`, `&` and control characters;
//! - Go float formatting for the integer-valued and short-decimal cases used by
//!   scores/percentages.

use std::collections::BTreeMap;

/// A JSON value with Go `encoding/json` semantics.
///
/// Mirrors `cf-gojson::GoValue`. [`GoValue::Object`] preserves the
/// caller-provided field order (struct-origin: declaration order). [`GoValue::Map`]
/// byte-sorts its keys at encode time (map-origin), matching Go's
/// `map[string]X` marshaling.
#[derive(Debug, Clone, PartialEq)]
pub enum GoValue {
    /// Go `nil` / JSON `null`.
    Null,
    /// Go `bool`.
    Bool(bool),
    /// Go integer (`int`); never routed through the float formatter.
    Int(i64),
    /// Go `float64`; formatted via [`go_float`].
    Float(f64),
    /// Go `string`.
    Str(String),
    /// Go slice / JSON array.
    Array(Vec<GoValue>),
    /// Struct-origin object: fields emitted in the given (declaration) order.
    Object(Vec<(String, GoValue)>),
    /// Map-origin object: keys byte-sorted at encode time.
    Map(BTreeMap<String, GoValue>),
}

/// Encoder configuration mirroring `cf-gojson::Encoder` (DESIGN.md sec 2.2).
#[derive(Debug, Clone)]
pub struct Encoder {
    /// `None` = compact (`json.Marshal`); `Some("  ")` = `SetIndent("", "  ")`.
    pub indent: Option<&'static str>,
    /// HTML escaping of `<`, `>`, `&`. Default `true`, matching Go.
    pub escape_html: bool,
    /// Append exactly one trailing `\n` (Encoder.Encode paths). `false` for
    /// Marshal paths.
    pub trailing_newline: bool,
}

impl Default for Encoder {
    /// Compact, HTML-escape on, no trailing newline — matching `json.Marshal`.
    fn default() -> Self {
        Encoder {
            indent: None,
            escape_html: true,
            trailing_newline: false,
        }
    }
}

impl Encoder {
    /// Encodes a [`GoValue`] to a byte-compatible JSON string.
    pub fn encode(&self, value: &GoValue) -> String {
        let mut out = String::new();
        self.write_value(&mut out, value, 0);
        if self.trailing_newline {
            out.push('\n');
        }
        out
    }

    fn write_value(&self, out: &mut String, value: &GoValue, depth: usize) {
        match value {
            GoValue::Null => out.push_str("null"),
            GoValue::Bool(true) => out.push_str("true"),
            GoValue::Bool(false) => out.push_str("false"),
            GoValue::Int(i) => out.push_str(&i.to_string()),
            GoValue::Float(f) => out.push_str(&go_float(*f)),
            GoValue::Str(s) => self.write_string(out, s),
            GoValue::Array(items) => self.write_array(out, items, depth),
            GoValue::Object(entries) => {
                let pairs: Vec<(&str, &GoValue)> =
                    entries.iter().map(|(k, v)| (k.as_str(), v)).collect();
                self.write_object(out, &pairs, depth);
            }
            GoValue::Map(map) => {
                // Map-origin: keys byte-sorted. BTreeMap already orders by the
                // Rust `String` Ord, which is byte (UTF-8) order, matching Go.
                let pairs: Vec<(&str, &GoValue)> =
                    map.iter().map(|(k, v)| (k.as_str(), v)).collect();
                self.write_object(out, &pairs, depth);
            }
        }
    }

    fn write_array(&self, out: &mut String, items: &[GoValue], depth: usize) {
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

    fn write_object(&self, out: &mut String, pairs: &[(&str, &GoValue)], depth: usize) {
        if pairs.is_empty() {
            out.push_str("{}");
            return;
        }
        out.push('{');
        for (i, (k, v)) in pairs.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            self.newline_indent(out, depth + 1);
            self.write_string(out, k);
            out.push(':');
            if self.indent.is_some() {
                out.push(' ');
            }
            self.write_value(out, v, depth + 1);
        }
        self.newline_indent(out, depth);
        out.push('}');
    }

    fn newline_indent(&self, out: &mut String, depth: usize) {
        if let Some(unit) = self.indent {
            out.push('\n');
            for _ in 0..depth {
                out.push_str(unit);
            }
        }
    }

    /// Writes a JSON string literal reproducing Go's `encodeState.string`:
    /// escape `"`, `\`, control chars (`\n`/`\r`/`\t` shortcuts, else
    /// `\u00XX`), and `<`/`>`/`&` when `escape_html`.
    fn write_string(&self, out: &mut String, s: &str) {
        out.push('"');
        for ch in s.chars() {
            match ch {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                '<' | '>' | '&' if self.escape_html => {
                    out.push_str(&format!("\\u{:04x}", ch as u32));
                }
                '\u{2028}' if self.escape_html => out.push_str("\\u2028"),
                '\u{2029}' if self.escape_html => out.push_str("\\u2029"),
                c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
                c => out.push(c),
            }
        }
        out.push('"');
    }
}

/// Formats an `f64` the way Go's `encoding/json` does
/// (`strconv.AppendFloat(_, 'g', -1, 64)` semantics).
///
/// Integer-valued floats print without a decimal point (`1.0` -> `"1"`), and
/// short decimals print their shortest round-trip form. Non-finite values are
/// rendered as `null` (Go errors on them, but the renderer never produces NaN
/// scores; `null` keeps the output well-formed and visible).
///
/// NOTE: full exponent/`e±NN` edge-case parity is owned by `cf-gojson`; the
/// renderer's scores/percentages are always finite values in `0..=10`.
pub fn go_float(f: f64) -> String {
    if !f.is_finite() {
        return "null".to_string();
    }
    if f == 0.0 {
        // Preserve -0.0 like Go (which prints "-0").
        return if f.is_sign_negative() {
            "-0".to_string()
        } else {
            "0".to_string()
        };
    }
    if f == f.trunc() && f.abs() < 1e21 {
        // Integer-valued float: no decimal point.
        return format!("{}", f as i64);
    }
    // Shortest round-trip form (Rust's default matches Go's 'g' digits for the
    // small magnitudes the renderer emits).
    format!("{f}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_keys_byte_sorted() {
        let mut m = BTreeMap::new();
        m.insert("b".to_string(), GoValue::Int(2));
        m.insert("a".to_string(), GoValue::Int(1));
        let enc = Encoder::default();
        assert_eq!(enc.encode(&GoValue::Map(m)), r#"{"a":1,"b":2}"#);
    }

    #[test]
    fn object_preserves_declaration_order() {
        let v = GoValue::Object(vec![
            ("title".to_string(), GoValue::Str("X".to_string())),
            ("score".to_string(), GoValue::Float(0.8)),
        ]);
        let enc = Encoder::default();
        assert_eq!(enc.encode(&v), r#"{"title":"X","score":0.8}"#);
    }

    #[test]
    fn empty_array_is_brackets() {
        let enc = Encoder::default();
        assert_eq!(enc.encode(&GoValue::Array(vec![])), "[]");
    }

    #[test]
    fn html_escaping_on_by_default() {
        // Go encoding/json escapes <, > and & by default (DESIGN.md 2.1):
        // '<' -> <, '>' -> >, '&' -> &. The default Encoder has
        // escape_html=true, so the output must contain the escaped forms, NOT
        // the raw characters.
        let enc = Encoder::default();
        assert_eq!(
            enc.encode(&GoValue::Str("a<b>&c".to_string())),
            "\"a\\u003cb\\u003e\\u0026c\""
        );
    }

    #[test]
    fn html_escaping_can_be_disabled() {
        // SetEscapeHTML(false) leaves <, >, & untouched.
        let enc = Encoder {
            indent: None,
            escape_html: false,
            trailing_newline: false,
        };
        assert_eq!(
            enc.encode(&GoValue::Str("a<b>&c".to_string())),
            r#""a<b>&c""#
        );
    }

    #[test]
    fn unicode_line_separators_escaped_when_html_on() {
        // Go escapes U+2028/U+2029 to \u2028/\u2029 when HTML-escaping is on.
        let enc = Encoder::default();
        assert_eq!(
            enc.encode(&GoValue::Str("a\u{2028}b\u{2029}c".to_string())),
            "\"a\\u2028b\\u2029c\""
        );
    }

    #[test]
    fn float_integer_valued_has_no_decimal() {
        assert_eq!(go_float(1.0), "1");
        assert_eq!(go_float(0.8), "0.8");
        assert_eq!(go_float(0.0), "0");
    }

    #[test]
    fn indent_mode_adds_space_after_colon() {
        let v = GoValue::Object(vec![("a".to_string(), GoValue::Int(1))]);
        let enc = Encoder {
            indent: Some("  "),
            escape_html: true,
            trailing_newline: false,
        };
        assert_eq!(enc.encode(&v), "{\n  \"a\": 1\n}");
    }
}
