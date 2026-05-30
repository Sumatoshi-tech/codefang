//! JSON codec — byte-compatible with Go's `encoding/json` `Encoder`.
//!
//! The Go `JSONCodec` writes state with `json.NewEncoder(w)` and, when an indent
//! string is configured, `encoder.SetIndent("", indent)`. That carries four
//! behaviours this port reproduces exactly (see DESIGN.md §2.1):
//!
//! 1. **Map-key ordering** — object keys are sorted by raw UTF-8 byte order.
//! 2. **HTML escaping ON** — `<`, `>`, `&` become `<`/`>`/`&`, and
//!    `U+2028`/`U+2029` become ` `/` `.
//! 3. **Go float formatting** — `strconv.AppendFloat(b, f, 'g', -1, 64)` rules:
//!    exponential when `exp < -4 || exp >= 21`, `e±NN` exponent with sign and ≥2
//!    digits, integer-valued floats rendered without a decimal point.
//! 4. **Trailing newline + compact/indent semantics** — `Encoder.Encode` always
//!    appends exactly one `\n`; compact emits `{"a":1}` (no space after `:`),
//!    indent emits `{\n  "a": 1\n}` (space after `:`) with empty containers
//!    collapsed to `{}` / `[]`.
//!
//! # Relationship to `cf-gojson`
//!
//! DESIGN.md §1 routes every JSON emitter through the tier-0 `cf-gojson` crate.
//! `cf-gojson` is not yet implemented, so this module carries a self-contained
//! encoder over [`serde_json::Value`] that implements the same four rules. When
//! `cf-gojson` lands, [`encode_go_json`] should delegate to its
//! `Encoder { indent, escape_html: true, trailing_newline: true }` and this
//! module's `go_json` helpers retire. Persist on-disk state is never
//! user-visible report output, but keeping the bytes Go-compatible means any
//! checkpoint that *is* inspected matches the Go reference exactly.

use std::io::{Read, Write};

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::error::PersistError;
use crate::Codec;

/// Default indentation for pretty-printed JSON (`defaultIndent` in Go: two spaces).
pub const DEFAULT_INDENT: &str = "  ";

/// File extension for JSON state files (`jsonExtension` in Go).
pub const JSON_EXTENSION: &str = ".json";

/// JSON codec with optional indentation.
///
/// Mirrors Go's `persist.JSONCodec{Indent string}`: an empty `indent` means
/// compact JSON, any non-empty `indent` enables pretty-printing with that string
/// as one indentation level. Output is byte-compatible with Go's `encoding/json`
/// `Encoder` (HTML escaping on, trailing newline, map keys byte-sorted).
#[derive(Debug, Clone, Default)]
pub struct JsonCodec {
    /// Indentation string. Empty = compact JSON; non-empty = pretty-printed.
    pub indent: String,
}

impl JsonCodec {
    /// Creates a JSON codec with pretty-printing (two-space indent).
    ///
    /// Equivalent to Go's `NewJSONCodec()`.
    #[must_use]
    pub fn new() -> Self {
        JsonCodec {
            indent: DEFAULT_INDENT.to_string(),
        }
    }

    /// Creates a JSON codec with an explicit indent string (empty = compact).
    #[must_use]
    pub fn with_indent(indent: impl Into<String>) -> Self {
        JsonCodec {
            indent: indent.into(),
        }
    }
}

impl Codec for JsonCodec {
    fn encode<W: Write, T: Serialize>(&self, mut w: W, state: &T) -> Result<(), PersistError> {
        // Build the data model through serde, then render with the Go-compatible
        // encoder. serde_json::to_value cannot fail to *render*; it only fails if
        // the type cannot be represented (mirroring Go's json.Marshal error).
        let value = serde_json::to_value(state).map_err(PersistError::JsonEncode)?;
        let bytes = encode_go_json(&value, &self.indent);
        w.write_all(&bytes).map_err(PersistError::Io)
    }

    fn decode<R: Read, T: DeserializeOwned>(
        &self,
        r: R,
        state: &mut T,
    ) -> Result<(), PersistError> {
        let decoded: T = serde_json::from_reader(r).map_err(PersistError::JsonDecode)?;
        *state = decoded;
        Ok(())
    }

    fn extension(&self) -> &'static str {
        JSON_EXTENSION
    }
}

/// Encodes a [`Value`] to bytes that match Go's `encoding/json` `Encoder`.
///
/// `indent` empty selects compact mode; otherwise it is used as one indentation
/// level. Exactly one trailing `\n` is appended (Go's `Encoder.Encode`).
#[must_use]
pub fn encode_go_json(value: &Value, indent: &str) -> Vec<u8> {
    let mut out = Vec::new();
    if indent.is_empty() {
        write_compact(&mut out, value);
    } else {
        write_indented(&mut out, value, indent, 0);
    }
    out.push(b'\n');
    out
}

/// Writes a value in compact form (`{"a":1,"b":2}`, no space after `:`).
fn write_compact(out: &mut Vec<u8>, value: &Value) {
    match value {
        Value::Null => out.extend_from_slice(b"null"),
        Value::Bool(b) => out.extend_from_slice(if *b { b"true" } else { b"false" }),
        Value::Number(n) => write_number(out, n),
        Value::String(s) => write_go_string(out, s),
        Value::Array(arr) => {
            out.push(b'[');
            for (i, item) in arr.iter().enumerate() {
                if i > 0 {
                    out.push(b',');
                }
                write_compact(out, item);
            }
            out.push(b']');
        }
        Value::Object(map) => {
            out.push(b'{');
            for (i, key) in sorted_keys(map).into_iter().enumerate() {
                if i > 0 {
                    out.push(b',');
                }
                write_go_string(out, key);
                out.push(b':');
                write_compact(out, &map[key]);
            }
            out.push(b'}');
        }
    }
}

/// Writes a value with Go-style indentation (one space after `:`, empty
/// containers collapsed to `{}` / `[]`).
fn write_indented(out: &mut Vec<u8>, value: &Value, indent: &str, depth: usize) {
    match value {
        Value::Array(arr) if !arr.is_empty() => {
            out.push(b'[');
            for (i, item) in arr.iter().enumerate() {
                if i > 0 {
                    out.push(b',');
                }
                out.push(b'\n');
                push_indent(out, indent, depth + 1);
                write_indented(out, item, indent, depth + 1);
            }
            out.push(b'\n');
            push_indent(out, indent, depth);
            out.push(b']');
        }
        Value::Object(map) if !map.is_empty() => {
            out.push(b'{');
            for (i, key) in sorted_keys(map).into_iter().enumerate() {
                if i > 0 {
                    out.push(b',');
                }
                out.push(b'\n');
                push_indent(out, indent, depth + 1);
                write_go_string(out, key);
                out.extend_from_slice(b": ");
                write_indented(out, &map[key], indent, depth + 1);
            }
            out.push(b'\n');
            push_indent(out, indent, depth);
            out.push(b'}');
        }
        // Scalars and empty containers render the same as compact form.
        _ => write_compact(out, value),
    }
}

/// Appends `indent` repeated `depth` times.
fn push_indent(out: &mut Vec<u8>, indent: &str, depth: usize) {
    for _ in 0..depth {
        out.extend_from_slice(indent.as_bytes());
    }
}

/// Returns the object's keys sorted by raw UTF-8 byte order (Go's map-key rule).
fn sorted_keys(map: &serde_json::Map<String, Value>) -> Vec<&String> {
    let mut keys: Vec<&String> = map.keys().collect();
    keys.sort_unstable_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
    keys
}

/// Writes a `serde_json::Number` using Go's `encoding/json` numeric rules.
///
/// Integers (those that round-trip through `i64`/`u64`) are written verbatim.
/// Anything else is a float and goes through [`go_float`].
fn write_number(out: &mut Vec<u8>, n: &serde_json::Number) {
    if let Some(u) = n.as_u64() {
        let mut buf = itoa::Buffer::new();
        out.extend_from_slice(buf.format(u).as_bytes());
    } else if let Some(i) = n.as_i64() {
        let mut buf = itoa::Buffer::new();
        out.extend_from_slice(buf.format(i).as_bytes());
    } else if let Some(fl) = n.as_f64() {
        out.extend_from_slice(go_float(fl).as_bytes());
    } else {
        // serde_json with arbitrary_precision off cannot reach here; fall back
        // to the value's own textual form to avoid panicking.
        out.extend_from_slice(n.to_string().as_bytes());
    }
}

/// Formats an `f64` the way Go's `encoding/json` does
/// (`strconv.AppendFloat(b, f, 'g', -1, 64)`).
///
/// Steps:
/// 1. Non-finite values are invalid in Go JSON; this returns `"null"` defensively
///    (serde_json never yields NaN/Inf, so this branch is unreachable in practice).
/// 2. Obtain the shortest round-trip decimal from `ryu` (same unique digit
///    sequence Go's `strconv` produces).
/// 3. Re-render with Go's `'g'` rules: exponential when `exp < -4 || exp >= 21`,
///    `e±NN` exponent (sign + ≥2 digits), integer-valued floats without a `.`.
#[must_use]
pub fn go_float(f: f64) -> String {
    if !f.is_finite() {
        return "null".to_string();
    }
    if f == 0.0 {
        // Preserve the sign of zero, matching Go (`-0` stays `-0`).
        return if f.is_sign_negative() {
            "-0".to_string()
        } else {
            "0".to_string()
        };
    }

    let negative = f < 0.0;
    let abs = f.abs();

    // ryu gives the shortest round-trip representation; parse out its digits and
    // decimal exponent so we can re-render in Go's exact style.
    let mut ryu_buf = ryu::Buffer::new();
    let ryu_str = ryu_buf.format_finite(abs);
    let (digits, exp10) = parse_ryu(ryu_str);

    // `exp10` is the power of ten of the first significant digit, i.e. the value
    // equals 0.<digits> * 10^(exp10+1) reframed as <d0>.<rest> * 10^exp10.
    // Go switches to exponential form when exp10 < -4 || exp10 >= 21.
    let mut s = String::new();
    if negative {
        s.push('-');
    }

    if exp10 < -4 || exp10 >= 21 {
        render_exponential(&mut s, &digits, exp10);
    } else {
        render_fixed(&mut s, &digits, exp10);
    }
    s
}

/// Parses ryu's shortest output into `(significant_digits, exp10)` where the
/// value equals `d0.d1d2… × 10^exp10` (`d0` is the first char of `digits`).
fn parse_ryu(s: &str) -> (Vec<u8>, i32) {
    // ryu emits forms like "1.5", "150.0", "1e20", "1.234e-5", "0.001".
    let (mantissa, exp_part) = match s.split_once(['e', 'E']) {
        Some((m, e)) => (m, e.parse::<i32>().unwrap_or(0)),
        None => (s, 0),
    };

    let (int_part, frac_part) = match mantissa.split_once('.') {
        Some((i, f)) => (i, f),
        None => (mantissa, ""),
    };

    // Combine all significant digits, then find the position of the leading
    // significant digit to compute exp10.
    let mut all = String::with_capacity(int_part.len() + frac_part.len());
    all.push_str(int_part);
    all.push_str(frac_part);

    // exponent of the first digit of `int_part` (before the decimal point):
    //   value = (int_part.frac_part) × 10^exp_part
    //   first int digit weight = (len(int_part) - 1) + exp_part
    let mut exp10 = (int_part.len() as i32 - 1) + exp_part;

    // Strip leading zeros, decrementing exp10 for each removed leading zero.
    let bytes = all.into_bytes();
    let mut start = 0usize;
    while start < bytes.len() && bytes[start] == b'0' {
        start += 1;
        exp10 -= 1;
    }
    if start == bytes.len() {
        // All zeros (only happens for 0.0, handled before calling).
        return (vec![b'0'], 0);
    }

    // Strip trailing zeros from the significant digits (they carry no info).
    let mut end = bytes.len();
    while end > start + 1 && bytes[end - 1] == b'0' {
        end -= 1;
    }

    (bytes[start..end].to_vec(), exp10)
}

/// Renders fixed-point notation for a value whose leading digit has weight
/// `exp10` (Go's `'f'` branch of `'g'`).
fn render_fixed(s: &mut String, digits: &[u8], exp10: i32) {
    if exp10 >= 0 {
        let int_len = (exp10 + 1) as usize;
        if digits.len() <= int_len {
            // Integer value: all digits before the (implicit) point, pad zeros.
            for &d in digits {
                s.push(d as char);
            }
            for _ in 0..(int_len - digits.len()) {
                s.push('0');
            }
            // No fractional part, no decimal point (Go: float64(1.0) -> "1").
        } else {
            for &d in &digits[..int_len] {
                s.push(d as char);
            }
            s.push('.');
            for &d in &digits[int_len..] {
                s.push(d as char);
            }
        }
    } else {
        // 0.000ddd form: exp10 in [-4, -1].
        s.push_str("0.");
        for _ in 0..(-exp10 - 1) {
            s.push('0');
        }
        for &d in digits {
            s.push(d as char);
        }
    }
}

/// Renders exponential notation `d.dddde±NN` (Go's `'e'` branch of `'g'`).
fn render_exponential(s: &mut String, digits: &[u8], exp10: i32) {
    s.push(digits[0] as char);
    if digits.len() > 1 {
        s.push('.');
        for &d in &digits[1..] {
            s.push(d as char);
        }
    }
    s.push('e');
    if exp10 < 0 {
        s.push('-');
    } else {
        s.push('+');
    }
    let mag = exp10.unsigned_abs();
    // At least two exponent digits (Go: "1e+21", "1.5e-05").
    if mag < 10 {
        s.push('0');
    }
    let mut buf = itoa::Buffer::new();
    s.push_str(buf.format(mag));
}

/// Writes a JSON string with Go's `encodeState.string` escaping (HTML on).
///
/// Escapes `"`, `\`, the C0 control range (as `\u00XX` with `\n \r \t`
/// shortcuts), and — because Go never calls `SetEscapeHTML(false)` — `<`, `>`,
/// `&`, `U+2028`, and `U+2029`.
fn write_go_string(out: &mut Vec<u8>, s: &str) {
    out.push(b'"');
    for ch in s.chars() {
        match ch {
            '"' => out.extend_from_slice(b"\\\""),
            '\\' => out.extend_from_slice(b"\\\\"),
            '\n' => out.extend_from_slice(b"\\n"),
            '\r' => out.extend_from_slice(b"\\r"),
            '\t' => out.extend_from_slice(b"\\t"),
            // HTML-significant characters Go escapes by default.
            '<' => out.extend_from_slice(b"\\u003c"),
            '>' => out.extend_from_slice(b"\\u003e"),
            '&' => out.extend_from_slice(b"\\u0026"),
            // Line/paragraph separators (valid in JSON, invalid in JS strings).
            '\u{2028}' => out.extend_from_slice(b"\\u2028"),
            '\u{2029}' => out.extend_from_slice(b"\\u2029"),
            c if (c as u32) < 0x20 => {
                out.extend_from_slice(b"\\u00");
                let byte = c as u8;
                out.push(hex_digit(byte >> 4));
                out.push(hex_digit(byte & 0x0f));
            }
            c => {
                let mut buf = [0u8; 4];
                out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
            }
        }
    }
    out.push(b'"');
}

/// Returns the lowercase ASCII hex digit for a nibble in `0..=15`.
fn hex_digit(nibble: u8) -> u8 {
    match nibble {
        0..=9 => b'0' + nibble,
        _ => b'a' + (nibble - 10),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn render(value: &Value, indent: &str) -> String {
        String::from_utf8(encode_go_json(value, indent)).unwrap()
    }

    // ---- map-key byte-ordering ----------------------------------------------

    #[test]
    fn object_keys_sorted_by_byte_order() {
        let v = json!({"b": 2, "a": 1, "A": 0});
        // Uppercase 'A' (0x41) sorts before lowercase 'a' (0x61) and 'b' (0x62).
        assert_eq!(render(&v, ""), "{\"A\":0,\"a\":1,\"b\":2}\n");
    }

    // ---- HTML escaping is ON by default -------------------------------------

    #[test]
    fn html_significant_chars_escaped() {
        let v = json!({"k": "a<b>c&d"});
        assert_eq!(render(&v, ""), "{\"k\":\"a\\u003cb\\u003ec\\u0026d\"}\n");
    }

    #[test]
    fn line_and_paragraph_separators_escaped() {
        let v = Value::String("x\u{2028}y\u{2029}z".to_string());
        assert_eq!(render(&v, ""), "\"x\\u2028y\\u2029z\"\n");
    }

    #[test]
    fn control_chars_and_shortcuts() {
        let v = Value::String("\t\n\r\u{0001}".to_string());
        assert_eq!(render(&v, ""), "\"\\t\\n\\r\\u0001\"\n");
    }

    // ---- compact vs indent + trailing newline -------------------------------

    #[test]
    fn compact_has_no_space_after_colon() {
        let v = json!({"a": 1, "b": 2});
        assert_eq!(render(&v, ""), "{\"a\":1,\"b\":2}\n");
    }

    #[test]
    fn indent_has_space_after_colon_and_newlines() {
        let v = json!({"a": 1, "b": 2});
        assert_eq!(render(&v, "  "), "{\n  \"a\": 1,\n  \"b\": 2\n}\n");
    }

    #[test]
    fn empty_containers_collapse_in_indent_mode() {
        let v = json!({"arr": [], "obj": {}});
        assert_eq!(render(&v, "  "), "{\n  \"arr\": [],\n  \"obj\": {}\n}\n");
    }

    #[test]
    fn nested_indentation() {
        let v = json!({"outer": {"inner": [1, 2]}});
        assert_eq!(
            render(&v, "  "),
            "{\n  \"outer\": {\n    \"inner\": [\n      1,\n      2\n    ]\n  }\n}\n"
        );
    }

    #[test]
    fn always_one_trailing_newline() {
        let v = json!(42);
        let bytes = encode_go_json(&v, "");
        assert_eq!(bytes, b"42\n");
    }

    // ---- Go float formatting -------------------------------------------------

    #[test]
    fn integer_valued_float_has_no_decimal_point() {
        assert_eq!(go_float(1.0), "1");
        assert_eq!(go_float(100.0), "100");
        assert_eq!(go_float(-7.0), "-7");
    }

    #[test]
    fn negative_zero_preserved() {
        assert_eq!(go_float(-0.0), "-0");
        assert_eq!(go_float(0.0), "0");
    }

    #[test]
    fn exponential_threshold_at_1e21() {
        // 1e20 stays fixed; 1e21 switches to exponential (Go's 21 threshold).
        assert_eq!(go_float(1e20), "100000000000000000000");
        assert_eq!(go_float(1e21), "1e+21");
    }

    #[test]
    fn small_exponential_threshold() {
        // exp >= -4 stays fixed; exp < -4 goes exponential.
        assert_eq!(go_float(1e-4), "0.0001");
        assert_eq!(go_float(1e-5), "1e-05");
    }

    #[test]
    fn fractional_values() {
        assert_eq!(go_float(1.5), "1.5");
        assert_eq!(go_float(0.5), "0.5");
        assert_eq!(go_float(-2.25), "-2.25");
    }

    #[test]
    fn exponent_has_sign_and_at_least_two_digits() {
        assert_eq!(go_float(1.5e-5), "1.5e-05");
        assert_eq!(go_float(1e100), "1e+100");
    }

    // ---- round-trip through the codec ---------------------------------------

    #[derive(serde::Serialize, serde::Deserialize, PartialEq, Debug)]
    struct State {
        name: String,
        count: i64,
        values: std::collections::BTreeMap<String, i64>,
    }

    #[test]
    fn codec_round_trip() {
        let mut values = std::collections::BTreeMap::new();
        values.insert("a".to_string(), 1);
        values.insert("b".to_string(), 2);
        let original = State {
            name: "test".to_string(),
            count: 42,
            values,
        };

        let codec = JsonCodec::new();
        let mut buf = Vec::new();
        codec.encode(&mut buf, &original).unwrap();

        let mut decoded = State {
            name: String::new(),
            count: 0,
            values: std::collections::BTreeMap::new(),
        };
        codec.decode(buf.as_slice(), &mut decoded).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn codec_extension_is_json() {
        assert_eq!(JsonCodec::new().extension(), ".json");
    }

    #[test]
    fn compact_codec_has_at_most_one_newline() {
        let codec = JsonCodec::with_indent("");
        let original = State {
            name: "compact".to_string(),
            count: 1,
            values: std::collections::BTreeMap::new(),
        };
        let mut buf = Vec::new();
        codec.encode(&mut buf, &original).unwrap();
        let text = String::from_utf8(buf).unwrap();
        assert!(text.matches('\n').count() <= 1);
    }

    #[test]
    fn pretty_codec_contains_indent() {
        let codec = JsonCodec::new();
        let original = State {
            name: "pretty".to_string(),
            count: 1,
            values: std::collections::BTreeMap::new(),
        };
        let mut buf = Vec::new();
        codec.encode(&mut buf, &original).unwrap();
        let text = String::from_utf8(buf).unwrap();
        assert!(text.contains(DEFAULT_INDENT));
    }

    #[test]
    fn codec_decode_error_is_reported() {
        let codec = JsonCodec::new();
        let mut decoded = State {
            name: String::new(),
            count: 0,
            values: std::collections::BTreeMap::new(),
        };
        let err = codec
            .decode(b"not valid json{{{".as_slice(), &mut decoded)
            .unwrap_err();
        assert!(err.to_string().contains("json decode"));
    }
}
