//! Temporary in-crate Go-`encoding/json`-compatible encoder.
//!
//! # Why this module exists
//!
//! Per `specs/rust-rewrite/DESIGN.md` (§1.1, §2.2) the canonical Go-byte-compatible
//! JSON encoder is the dedicated tier-0 crate **`cf-gojson`**, and `cf-textutil`
//! is supposed to depend on it (`cf-textutil` "wraps `cf-gojson`"). At the time
//! this crate was ported, `cf-gojson` is still a compiling scaffold that exports
//! nothing but `CRATE_NAME` — it does not yet provide `Encoder` / `GoValue` /
//! `EncodeError`. Because the port rules forbid editing another crate's files,
//! `cf-textutil` cannot compile against `cf-gojson` yet.
//!
//! This module is the **minimal interface** mandated for that situation: a
//! self-contained encoder that reproduces Go `encoding/json` bytes exactly, with
//! the same public surface (`GoValue`, `Encoder`, `EncodeError`) that the
//! eventual `cf-gojson` crate exposes. When `cf-gojson` lands, delete this module
//! and switch `json.rs` back to `use cf_gojson::{Encoder, GoValue, EncodeError};`
//! — the API is intentionally identical so that is a one-line change. See the
//! crate-level porting notes.
//!
//! # Go semantics reproduced
//!
//! Mirrors `encoding/json`'s encoder:
//!
//! * **Map keys are byte-sorted** at encode time (`map[string]X` order rule).
//!   `GoValue::object` therefore sorts its keys by raw UTF-8 bytes.
//! * **HTML escaping** (when enabled): `<` → `<`, `>` → `>`,
//!   `&` → `&`, U+2028 → ` `, U+2029 → ` `.
//! * **String escaping**: `"` → `\"`, `\\` → `\\`, `\n`/`\r`/`\t` short forms,
//!   other C0 controls (incl. `\b`/`\f`) → `\u00XX`.
//! * **Compact mode**: no spaces (`{"a":1,"b":2}`).
//! * **Indent mode** (`SetIndent("", "  ")`): two-space indent, one space after
//!   `:`, empty objects/arrays collapse to `{}` / `[]`.
//! * **Trailing newline**: appended when configured (mirrors `Encoder.Encode`).
//! * **Floats**: rendered with Go `strconv.AppendFloat(b, f, 'g', -1, 64)`
//!   semantics (`'e'` when `exp < -4 || exp >= 21`, `e±NN` exponent with sign and
//!   ≥2 digits, integer-valued floats printed without a decimal point). Non-finite
//!   floats are an error, matching Go.

use std::fmt;

/// Go-compatible JSON value tree.
///
/// Integers and floats are kept on separate variants so integers never go
/// through the float-formatting path, matching Go's `encoding/json`.
#[derive(Debug, Clone, PartialEq)]
pub enum GoValue {
    /// JSON `null`.
    Null,
    /// JSON boolean.
    Bool(bool),
    /// Signed integer (never float-formatted).
    Int(i64),
    /// Unsigned integer (never float-formatted).
    Uint(u64),
    /// IEEE-754 double, formatted with Go's `'g'/-1` rules.
    Float(f64),
    /// UTF-8 string.
    Str(String),
    /// JSON array.
    Array(Vec<GoValue>),
    /// JSON object. Invariant: entries are sorted by key bytes (Go map order).
    Object(Vec<(String, GoValue)>),
}

impl GoValue {
    /// Builds an owned string value.
    pub fn str(s: impl Into<String>) -> GoValue {
        GoValue::Str(s.into())
    }

    /// Builds a map-origin object value from `(key, value)` pairs, byte-sorting
    /// the keys exactly as Go's `encoding/json` sorts `map[string]X` keys.
    ///
    /// Later duplicate keys overwrite earlier ones (last-wins), matching Go map
    /// construction semantics.
    pub fn object<K: Into<String>>(entries: impl IntoIterator<Item = (K, GoValue)>) -> GoValue {
        let mut pairs: Vec<(String, GoValue)> = Vec::new();
        for (k, v) in entries {
            let k = k.into();
            if let Some(slot) = pairs.iter_mut().find(|(ek, _)| *ek == k) {
                slot.1 = v;
            } else {
                pairs.push((k, v));
            }
        }
        pairs.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
        GoValue::Object(pairs)
    }

    /// Builds an array value.
    pub fn array(items: impl IntoIterator<Item = GoValue>) -> GoValue {
        GoValue::Array(items.into_iter().collect())
    }
}

/// Error produced when a value cannot be encoded to Go-compatible JSON.
///
/// The only such case Go's `encoding/json` raises for the value kinds modeled by
/// [`GoValue`] is a non-finite float (NaN / ±Inf), which Go rejects with
/// `json: unsupported value`.
#[derive(Debug, Clone, PartialEq)]
pub enum EncodeError {
    /// A non-finite float (NaN or infinity) was encountered.
    UnsupportedFloat(f64),
}

impl fmt::Display for EncodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            // Wording mirrors Go's "json: unsupported value: <float>".
            EncodeError::UnsupportedFloat(v) => write!(f, "json: unsupported value: {v}"),
        }
    }
}

impl std::error::Error for EncodeError {}

/// Go-`encoding/json`-compatible encoder.
///
/// Configure with [`Encoder::set_escape_html`], [`Encoder::set_trailing_newline`]
/// and [`Encoder::set_indent`], then call [`Encoder::encode`].
#[derive(Debug, Clone)]
pub struct Encoder {
    escape_html: bool,
    trailing_newline: bool,
    indent: Option<String>,
}

impl Default for Encoder {
    fn default() -> Self {
        // Go defaults: HTML escaping on, compact, no trailing newline. The
        // trailing newline is opt-in because `json.Marshal` omits it while
        // `json.Encoder.Encode` appends one.
        Encoder {
            escape_html: true,
            trailing_newline: false,
            indent: None,
        }
    }
}

impl Encoder {
    /// Creates an encoder with Go's defaults (HTML escaping on, compact, no
    /// trailing newline).
    pub fn new() -> Self {
        Encoder::default()
    }

    /// Sets whether `<`, `>`, `&`, U+2028 and U+2029 are escaped (Go's
    /// `SetEscapeHTML`). Default `true`.
    pub fn set_escape_html(&mut self, on: bool) {
        self.escape_html = on;
    }

    /// Sets whether a single trailing `\n` is appended (Go's `Encoder.Encode`).
    pub fn set_trailing_newline(&mut self, on: bool) {
        self.trailing_newline = on;
    }

    /// Sets the indentation string (`Some("  ")` ≈ `SetIndent("", "  ")`); `None`
    /// selects compact mode.
    pub fn set_indent(&mut self, indent: Option<&str>) {
        self.indent = indent.map(|s| s.to_string());
    }

    /// Encodes `v` to Go-compatible JSON bytes.
    ///
    /// # Errors
    /// Returns [`EncodeError::UnsupportedFloat`] if `v` contains a non-finite
    /// float, matching Go's `encoding/json`.
    pub fn encode(&self, v: &GoValue) -> Result<Vec<u8>, EncodeError> {
        let mut out = String::new();
        match &self.indent {
            Some(ind) => self.fmt_indented(v, ind, 0, &mut out)?,
            None => self.fmt_compact(v, &mut out)?,
        }
        if self.trailing_newline {
            out.push('\n');
        }
        Ok(out.into_bytes())
    }

    fn fmt_compact(&self, v: &GoValue, out: &mut String) -> Result<(), EncodeError> {
        match v {
            GoValue::Null => out.push_str("null"),
            GoValue::Bool(true) => out.push_str("true"),
            GoValue::Bool(false) => out.push_str("false"),
            GoValue::Int(n) => out.push_str(&n.to_string()),
            GoValue::Uint(n) => out.push_str(&n.to_string()),
            GoValue::Float(f) => out.push_str(&go_float(*f)?),
            GoValue::Str(s) => self.encode_string(s, out),
            GoValue::Array(arr) => {
                out.push('[');
                for (i, item) in arr.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    self.fmt_compact(item, out)?;
                }
                out.push(']');
            }
            GoValue::Object(map) => {
                out.push('{');
                for (i, (k, val)) in map.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    self.encode_string(k, out);
                    out.push(':');
                    self.fmt_compact(val, out)?;
                }
                out.push('}');
            }
        }
        Ok(())
    }

    fn fmt_indented(
        &self,
        v: &GoValue,
        ind: &str,
        depth: usize,
        out: &mut String,
    ) -> Result<(), EncodeError> {
        match v {
            GoValue::Null => out.push_str("null"),
            GoValue::Bool(true) => out.push_str("true"),
            GoValue::Bool(false) => out.push_str("false"),
            GoValue::Int(n) => out.push_str(&n.to_string()),
            GoValue::Uint(n) => out.push_str(&n.to_string()),
            GoValue::Float(f) => out.push_str(&go_float(*f)?),
            GoValue::Str(s) => self.encode_string(s, out),
            GoValue::Array(arr) => {
                if arr.is_empty() {
                    out.push_str("[]");
                    return Ok(());
                }
                out.push('[');
                for (i, item) in arr.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    out.push('\n');
                    push_indent(ind, depth + 1, out);
                    self.fmt_indented(item, ind, depth + 1, out)?;
                }
                out.push('\n');
                push_indent(ind, depth, out);
                out.push(']');
            }
            GoValue::Object(map) => {
                if map.is_empty() {
                    out.push_str("{}");
                    return Ok(());
                }
                out.push('{');
                for (i, (k, val)) in map.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    out.push('\n');
                    push_indent(ind, depth + 1, out);
                    self.encode_string(k, out);
                    out.push_str(": ");
                    self.fmt_indented(val, ind, depth + 1, out)?;
                }
                out.push('\n');
                push_indent(ind, depth, out);
                out.push('}');
            }
        }
        Ok(())
    }

    /// Encodes a string with Go's `encodeState.string` rules.
    fn encode_string(&self, s: &str, out: &mut String) {
        out.push('"');
        for c in s.chars() {
            match c {
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
                c if (c as u32) < 0x20 => push_u_escape(c as u32, out),
                c => out.push(c),
            }
        }
        out.push('"');
    }
}

/// Appends `depth` copies of `ind`.
fn push_indent(ind: &str, depth: usize, out: &mut String) {
    for _ in 0..depth {
        out.push_str(ind);
    }
}

/// Appends a `\u00XX` escape for a code point below `0x10000`.
fn push_u_escape(cp: u32, out: &mut String) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    out.push_str("\\u");
    out.push(HEX[((cp >> 12) & 0xF) as usize] as char);
    out.push(HEX[((cp >> 8) & 0xF) as usize] as char);
    out.push(HEX[((cp >> 4) & 0xF) as usize] as char);
    out.push(HEX[(cp & 0xF) as usize] as char);
}

/// Formats `f` with Go `strconv.AppendFloat(b, f, 'g', -1, 64)` semantics, as
/// used by `encoding/json`'s `floatEncoder`.
///
/// Strategy: obtain the shortest round-trip decimal from Rust's formatter (which,
/// like Go's `strconv`, is shortest and round-trips), then re-render the exponent
/// using Go's `'g'` rules: switch to exponential when the decimal exponent is
/// `< -4` or `>= 21`, render the exponent as `e` + sign + at least two digits,
/// and print integer-valued floats without a decimal point.
fn go_float(f: f64) -> Result<String, EncodeError> {
    if !f.is_finite() {
        return Err(EncodeError::UnsupportedFloat(f));
    }
    if f == 0.0 {
        // Preserves Go's behavior, including "-0" for negative zero.
        return Ok(if f.is_sign_negative() {
            "-0".to_string()
        } else {
            "0".to_string()
        });
    }

    let neg = f < 0.0;
    let abs = f.abs();

    // Shortest round-trip significant digits via Rust's exponential formatter,
    // which yields `d(.ddd)e±N` with no trailing zeros — the same unique digit
    // sequence Go's strconv produces.
    let sci = format!("{abs:e}"); // e.g. "1.5e2", "1e0", "1.234e-5"
    let (mantissa, exp_str) = sci.split_once('e').expect("exponential format has 'e'");
    let exp: i32 = exp_str.parse().expect("valid exponent");

    // Pure significant digits (drop the mantissa '.').
    let digits: String = mantissa.chars().filter(|&c| c != '.').collect();
    let ndigits = digits.len() as i32;

    // Go's 'g': use 'e' when exp < -4 or exp >= 21 (exp = decimal point position
    // measured as power of ten of the leading digit).
    let body = if (-4..21).contains(&exp) {
        render_fixed(&digits, exp, ndigits)
    } else {
        render_exponential(&digits, exp)
    };

    Ok(if neg { format!("-{body}") } else { body })
}

/// Renders digits in Go `'e'` style: `d[.ddd]e±NN`.
fn render_exponential(digits: &str, exp: i32) -> String {
    let mut m = String::new();
    let mut chars = digits.chars();
    m.push(chars.next().unwrap_or('0'));
    let rest: String = chars.collect();
    if !rest.is_empty() {
        m.push('.');
        m.push_str(&rest);
    }
    let sign = if exp < 0 { '-' } else { '+' };
    let mag = exp.unsigned_abs();
    // Go always uses at least two exponent digits.
    let exp_part = if mag < 10 {
        format!("{sign}0{mag}")
    } else {
        format!("{sign}{mag}")
    };
    format!("{m}e{exp_part}")
}

/// Renders digits in Go `'f'`/fixed style for `-4 <= exp < 21`.
fn render_fixed(digits: &str, exp: i32, ndigits: i32) -> String {
    if exp >= 0 {
        if exp + 1 >= ndigits {
            // All significant digits are to the left of the point; pad zeros.
            let zeros = (exp + 1 - ndigits) as usize;
            format!("{digits}{}", "0".repeat(zeros))
        } else {
            // Split into integer and fractional parts.
            let split = (exp + 1) as usize;
            format!("{}.{}", &digits[..split], &digits[split..])
        }
    } else {
        // 0.00...digits  (leading zeros after the decimal point).
        let leading = (-exp - 1) as usize;
        format!("0.{}{}", "0".repeat(leading), digits)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn compact() -> Encoder {
        let mut e = Encoder::new();
        e.set_escape_html(true);
        e.set_trailing_newline(true);
        e.set_indent(None);
        e
    }

    fn enc(v: &GoValue) -> String {
        String::from_utf8(compact().encode(v).unwrap()).unwrap()
    }

    #[test]
    fn object_keys_byte_sorted() {
        let v = GoValue::object([
            ("b", GoValue::Int(2)),
            ("a", GoValue::Int(1)),
            ("C", GoValue::Int(3)),
        ]);
        // Uppercase 'C' (0x43) sorts before lowercase 'a'/'b' by byte order.
        assert_eq!(enc(&v), "{\"C\":3,\"a\":1,\"b\":2}\n");
    }

    #[test]
    fn html_escaping() {
        let v = GoValue::str("a<b>c&d");
        assert_eq!(enc(&v), "\"a\\u003cb\\u003ec\\u0026d\"\n");
    }

    #[test]
    fn separators_escaped() {
        let v = GoValue::str("x\u{2028}y\u{2029}z");
        assert_eq!(enc(&v), "\"x\\u2028y\\u2029z\"\n");
    }

    #[test]
    fn control_chars() {
        let v = GoValue::str("\u{08}\t\n\u{0c}\r\u{01}");
        assert_eq!(enc(&v), "\"\\u0008\\t\\n\\u000c\\r\\u0001\"\n");
    }

    #[test]
    fn float_integer_valued() {
        assert_eq!(enc(&GoValue::Float(1.0)), "1\n");
        assert_eq!(enc(&GoValue::Float(100.0)), "100\n");
        assert_eq!(enc(&GoValue::Float(-0.0)), "-0\n");
        assert_eq!(enc(&GoValue::Float(0.0)), "0\n");
    }

    #[test]
    fn float_fractional() {
        assert_eq!(enc(&GoValue::Float(1.5)), "1.5\n");
        assert_eq!(enc(&GoValue::Float(0.5)), "0.5\n");
        // 2.71 (not pi) — a plain fractional value with no significance.
        assert_eq!(enc(&GoValue::Float(2.71)), "2.71\n");
        assert_eq!(enc(&GoValue::Float(12.25)), "12.25\n");
    }

    #[test]
    fn float_exponential_thresholds() {
        // exp >= 21 -> exponential; 1e21 == 1 followed by 21 zeros.
        assert_eq!(enc(&GoValue::Float(1e21)), "1e+21\n");
        // exp < -4 -> exponential.
        assert_eq!(enc(&GoValue::Float(1e-5)), "1e-05\n");
        // exp == -4 stays fixed.
        assert_eq!(enc(&GoValue::Float(1e-4)), "0.0001\n");
        // exp == 20 stays fixed (1 followed by 20 zeros).
        assert_eq!(enc(&GoValue::Float(1e20)), "100000000000000000000\n");
    }

    #[test]
    fn float_nan_inf_error() {
        assert!(compact().encode(&GoValue::Float(f64::NAN)).is_err());
        assert!(compact().encode(&GoValue::Float(f64::INFINITY)).is_err());
        assert!(compact().encode(&GoValue::Float(f64::NEG_INFINITY)).is_err());
    }

    #[test]
    fn indent_empty_containers() {
        let mut e = compact();
        e.set_indent(Some("  "));
        let v = GoValue::object([("arr", GoValue::array([])), ("obj", GoValue::object::<&str>([]))]);
        let s = String::from_utf8(e.encode(&v).unwrap()).unwrap();
        assert_eq!(s, "{\n  \"arr\": [],\n  \"obj\": {}\n}\n");
    }

    #[test]
    fn no_trailing_newline_when_disabled() {
        let mut e = Encoder::new();
        e.set_trailing_newline(false);
        let s = String::from_utf8(e.encode(&GoValue::Int(7)).unwrap()).unwrap();
        assert_eq!(s, "7");
    }

    #[test]
    fn escape_html_off() {
        let mut e = Encoder::new();
        e.set_escape_html(false);
        e.set_trailing_newline(true);
        let s = String::from_utf8(e.encode(&GoValue::str("<a>")).unwrap()).unwrap();
        assert_eq!(s, "\"<a>\"\n");
    }
}
