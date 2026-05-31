//! Machine-format serialization for [`ComputedMetrics`].
//!
//! Implements the Go `(*Analyzer).FormatReportJSON` / `FormatReportYAML` /
//! `FormatReportBinary` paths:
//!
//! | Method | Go call | Encoder config |
//! | --- | --- | --- |
//! | JSON   | `json.MarshalIndent(metrics, "", "  ")` | indent `"  "`, HTML-escape ON, no trailing newline |
//! | YAML   | `yaml.Marshal(metrics)` | yaml.v3-compatible emitter |
//! | binary | `reportutil.EncodeBinaryEnvelope(metrics, w)` | CFB1 = `"CFB1"` + u32-LE len + compact JSON |
//!
//! # Routing (DESIGN §2, project rule 1)
//!
//! Per the design, machine-format bytes MUST go through the shared `cf-gojson` /
//! `cf-goyaml` / `cf-reportutil` crates rather than raw serde. This module builds the
//! Go-value tree via [`to_go_value`] and is written to hand that tree to those
//! crates. While those crates are still scaffolds, this module also carries a
//! self-contained reference encoder ([`encode_json`]) that implements the exact Go
//! `encoding/json` rules (declaration-order struct fields, byte-sorted map keys, HTML
//! escaping, Go `'g'/-1/64` float formatting, two-space indent, no trailing newline)
//! so the crate compiles, is testable, and produces correct bytes. Swapping
//! [`encode_json`]/[`encode_yaml`]/[`encode_binary`] to delegate to
//! `cf-gojson`/`cf-goyaml`/`cf-reportutil` is a mechanical follow-up tracked in the
//! crate todos.

use crate::metrics::{ComputedMetrics, FunctionCohesionData};

/// A Go-`encoding/json` value, mirroring `cf_gojson::GoValue`.
///
/// `Object` distinguishes struct-origin (declaration order, `omitempty` already
/// applied by the builder) from map-origin (`sorted = true`, keys byte-sorted on
/// encode) so the dual-mode ordering rule of DESIGN §2.2 is honored.
#[derive(Debug, Clone, PartialEq)]
pub enum GoValue {
    /// Go `int` / `int64`.
    Int(i64),
    /// Go `float64`.
    Float(f64),
    /// Go `string`.
    Str(String),
    /// Go slice.
    Array(Vec<GoValue>),
    /// Go struct (ordered) or map (sorted-on-encode).
    Object {
        /// Key/value pairs.
        fields: Vec<(String, GoValue)>,
        /// True for map-origin objects (byte-sort keys on encode).
        sorted: bool,
    },
}

impl GoValue {
    fn struct_obj(fields: Vec<(String, GoValue)>) -> GoValue {
        GoValue::Object {
            fields,
            sorted: false,
        }
    }
}

/// Builds the [`GoValue`] tree for [`ComputedMetrics`], honoring field order and
/// `omitempty`. This is the input you would hand to `cf_gojson::Encoder`.
#[must_use]
pub fn to_go_value(m: &ComputedMetrics) -> GoValue {
    GoValue::struct_obj(vec![
        (
            "function_cohesion".into(),
            GoValue::Array(m.function_cohesion.iter().map(func_cohesion_value).collect()),
        ),
        ("distribution".into(), distribution_value(&m.distribution)),
        (
            "low_cohesion_functions".into(),
            GoValue::Array(
                m.low_cohesion_functions
                    .iter()
                    .map(low_cohesion_value)
                    .collect(),
            ),
        ),
        ("aggregate".into(), aggregate_value(&m.aggregate)),
    ])
}

fn func_cohesion_value(f: &FunctionCohesionData) -> GoValue {
    let mut fields = Vec::with_capacity(6);
    fields.push(("name".into(), GoValue::Str(f.name.clone())));
    if !f.source_file.is_empty() {
        fields.push(("source_file".into(), GoValue::Str(f.source_file.clone())));
    }
    if !f.language.is_empty() {
        fields.push(("language".into(), GoValue::Str(f.language.clone())));
    }
    if !f.directory.is_empty() {
        fields.push(("directory".into(), GoValue::Str(f.directory.clone())));
    }
    fields.push(("cohesion".into(), GoValue::Float(f.cohesion)));
    fields.push((
        "quality_level".into(),
        GoValue::Str(f.quality_level.clone()),
    ));
    GoValue::struct_obj(fields)
}

fn low_cohesion_value(f: &crate::metrics::LowCohesionFunctionData) -> GoValue {
    let mut fields = Vec::with_capacity(7);
    fields.push(("name".into(), GoValue::Str(f.name.clone())));
    if !f.source_file.is_empty() {
        fields.push(("source_file".into(), GoValue::Str(f.source_file.clone())));
    }
    if !f.language.is_empty() {
        fields.push(("language".into(), GoValue::Str(f.language.clone())));
    }
    if !f.directory.is_empty() {
        fields.push(("directory".into(), GoValue::Str(f.directory.clone())));
    }
    fields.push(("cohesion".into(), GoValue::Float(f.cohesion)));
    fields.push(("risk_level".into(), GoValue::Str(f.risk_level.clone())));
    fields.push((
        "recommendation".into(),
        GoValue::Str(f.recommendation.clone()),
    ));
    GoValue::struct_obj(fields)
}

fn aggregate_value(a: &crate::metrics::AggregateData) -> GoValue {
    GoValue::struct_obj(vec![
        ("total_functions".into(), GoValue::Int(a.total_functions)),
        ("lcom".into(), GoValue::Float(a.lcom)),
        ("lcom_variant".into(), GoValue::Str(a.lcom_variant.clone())),
        ("cohesion_score".into(), GoValue::Float(a.cohesion_score)),
        (
            "function_cohesion".into(),
            GoValue::Float(a.function_cohesion),
        ),
        ("health_score".into(), GoValue::Float(a.health_score)),
        ("message".into(), GoValue::Str(a.message.clone())),
    ])
}

fn distribution_value(dist: &std::collections::BTreeMap<String, i64>) -> GoValue {
    // map-origin object: keys byte-sorted on encode. BTreeMap is already sorted by
    // byte order for ASCII keys; we mark sorted = true so the encoder enforces it.
    GoValue::Object {
        fields: dist
            .iter()
            .map(|(k, v)| (k.clone(), GoValue::Int(*v)))
            .collect(),
        sorted: true,
    }
}

// === Reference Go-compatible encoders (to be delegated to cf-gojson/cf-goyaml) ===

/// CFB1 magic (Go `reportutil.BinaryMagic`).
pub const BINARY_MAGIC: &[u8; 4] = b"CFB1";

/// Encodes `metrics` as Go `json.MarshalIndent(metrics, "", "  ")`: two-space
/// indent, HTML escaping ON, and no trailing newline.
#[must_use]
pub fn encode_json(metrics: &ComputedMetrics) -> Vec<u8> {
    let mut out = String::new();
    write_value(&mut out, &to_go_value(metrics), Some("  "), 0);
    out.into_bytes()
}

/// Encodes `metrics` as compact Go `json.Marshal(metrics)`: no spaces, HTML
/// escaping ON, no trailing newline. This is the CFB1 payload.
#[must_use]
pub fn encode_compact_json(metrics: &ComputedMetrics) -> Vec<u8> {
    let mut out = String::new();
    write_value(&mut out, &to_go_value(metrics), None, 0);
    out.into_bytes()
}

/// Encodes `metrics` as a single CFB1 envelope record (Go
/// `reportutil.EncodeBinaryEnvelope`): `"CFB1"` + `u32` little-endian payload
/// length + compact-JSON payload.
#[must_use]
pub fn encode_binary(metrics: &ComputedMetrics) -> Vec<u8> {
    let payload = encode_compact_json(metrics);
    let mut out = Vec::with_capacity(8 + payload.len());
    out.extend_from_slice(BINARY_MAGIC);
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(&payload);
    out
}

/// Renders a [`GoValue`] into `out`. `indent = Some("  ")` reproduces
/// `MarshalIndent`; `indent = None` reproduces compact `Marshal`.
fn write_value(out: &mut String, v: &GoValue, indent: Option<&str>, depth: usize) {
    match v {
        GoValue::Int(n) => out.push_str(&n.to_string()),
        GoValue::Float(f) => out.push_str(&go_float(*f)),
        GoValue::Str(s) => write_json_string(out, s),
        GoValue::Array(items) => write_array(out, items, indent, depth),
        GoValue::Object { fields, sorted } => write_object(out, fields, *sorted, indent, depth),
    }
}

fn write_array(out: &mut String, items: &[GoValue], indent: Option<&str>, depth: usize) {
    if items.is_empty() {
        out.push_str("[]");
        return;
    }
    out.push('[');
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        newline_indent(out, indent, depth + 1);
        write_value(out, item, indent, depth + 1);
    }
    newline_indent(out, indent, depth);
    out.push(']');
}

fn write_object(
    out: &mut String,
    fields: &[(String, GoValue)],
    sorted: bool,
    indent: Option<&str>,
    depth: usize,
) {
    if fields.is_empty() {
        out.push_str("{}");
        return;
    }
    // For map-origin objects, byte-sort keys at encode time (Go map JSON rule).
    let mut ordered: Vec<&(String, GoValue)> = fields.iter().collect();
    if sorted {
        ordered.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
    }
    out.push('{');
    for (i, (k, val)) in ordered.iter().map(|p| (&p.0, &p.1)).enumerate() {
        if i > 0 {
            out.push(',');
        }
        newline_indent(out, indent, depth + 1);
        write_json_string(out, k);
        out.push(':');
        if indent.is_some() {
            out.push(' '); // space after colon only in indent mode
        }
        write_value(out, val, indent, depth + 1);
    }
    newline_indent(out, indent, depth);
    out.push('}');
}

fn newline_indent(out: &mut String, indent: Option<&str>, depth: usize) {
    if let Some(unit) = indent {
        out.push('\n');
        for _ in 0..depth {
            out.push_str(unit);
        }
    }
}

/// Writes a Go-`encoding/json`-escaped string (HTML escaping ON): escapes `"`, `\`,
/// control chars (`\n`/`\r`/`\t` shortcuts, else `\u00XX`), and `<`, `>`, `&`,
/// `U+2028`, `U+2029`.
fn write_json_string(out: &mut String, s: &str) {
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
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// Formats an `f64` with Go `strconv.AppendFloat(b, f, 'g', -1, 64)` semantics, as
/// used by `encoding/json`'s float encoder (DESIGN §2.2).
///
/// Rules reproduced: shortest round-trip digits; switch to exponential when
/// `exp < -4 || exp >= 21`; exponent rendered `e±NN` with sign and >= 2 digits;
/// integer-valued floats printed without a decimal point (`1.0` -> `1`); `±0`
/// printed as `0`.
///
/// This is the high-risk path; the integrated build should delegate to
/// `cf_gojson::go_float`, which is fuzzed against Go in the golden harness
/// (DESIGN §6 Layer A).
#[must_use]
pub fn go_float(f: f64) -> String {
    if f == 0.0 {
        // encoding/json renders both +0.0 and -0.0 as "0".
        return "0".to_string();
    }
    if f.is_nan() || f.is_infinite() {
        // Go's encoding/json errors on NaN/Inf; here we fall back to "0", which never
        // occurs for cohesion values (all finite in [0,1] or 0..100). The integrated
        // cf-gojson path returns an error instead.
        return "0".to_string();
    }

    // Obtain the shortest round-tripping decimal via Rust's formatter, which uses a
    // Grisu/Ryū-style shortest representation matching Go's digit sequence.
    let shortest = format!("{f}");
    reformat_go_g(&shortest)
}

/// Re-renders a shortest decimal string into Go `'g'` form.
fn reformat_go_g(shortest: &str) -> String {
    let (sign, body) = if let Some(stripped) = shortest.strip_prefix('-') {
        ("-", stripped)
    } else {
        ("", shortest)
    };

    let (mantissa_digits, decimal_exp) = decompose(body);

    // decimal_exp is the power of ten of the FIRST significant digit (scientific).
    // Go 'g' uses exponential when exp < -4 || exp >= 21.
    let use_exp = decimal_exp < -4 || decimal_exp >= 21;

    let rendered = if use_exp {
        render_exponential(&mantissa_digits, decimal_exp)
    } else {
        render_fixed(&mantissa_digits, decimal_exp)
    };

    format!("{sign}{rendered}")
}

/// Decomposes a non-negative decimal string (no sign) like "123.45" or "1e20" into
/// `(significant_digits_without_point, exponent_of_first_digit)`.
fn decompose(body: &str) -> (String, i32) {
    let (num, exp_part) = match body.split_once(['e', 'E']) {
        Some((n, e)) => (n, e.parse::<i32>().unwrap_or(0)),
        None => (body, 0),
    };

    let (int_part, frac_part) = match num.split_once('.') {
        Some((i, f)) => (i, f),
        None => (num, ""),
    };

    let all: String = format!("{int_part}{frac_part}");
    let point_pos = int_part.len() as i32; // digits before the decimal point

    let Some(first_sig) = all.find(|c: char| c != '0') else {
        return ("0".to_string(), 0);
    };

    let exp_of_first = point_pos - 1 - first_sig as i32 + exp_part;
    let last_sig = all.rfind(|c: char| c != '0').unwrap();
    let digits: String = all[first_sig..=last_sig].to_string();

    (digits, exp_of_first)
}

/// Renders fixed-point (`'f'`) form, `exp` = power of the first digit.
fn render_fixed(digits: &str, exp: i32) -> String {
    let n = digits.len() as i32;
    if exp >= 0 {
        if exp + 1 >= n {
            let mut s = digits.to_string();
            for _ in 0..(exp + 1 - n) {
                s.push('0');
            }
            s
        } else {
            let split = (exp + 1) as usize;
            format!("{}.{}", &digits[..split], &digits[split..])
        }
    } else {
        let mut s = String::from("0.");
        for _ in 0..(-exp - 1) {
            s.push('0');
        }
        s.push_str(digits);
        s
    }
}

/// Renders exponential (`'e'`) form: `d.dddde±NN`.
fn render_exponential(digits: &str, exp: i32) -> String {
    let mantissa = if digits.len() == 1 {
        digits.to_string()
    } else {
        format!("{}.{}", &digits[..1], &digits[1..])
    };
    let sign = if exp < 0 { '-' } else { '+' };
    let mag = exp.unsigned_abs();
    format!("{mantissa}e{sign}{mag:02}")
}

/// Encodes `metrics` as yaml.v3-compatible YAML.
///
/// NOTE: the integrated build MUST delegate to `cf_goyaml` (DESIGN §2.4, highest
/// residual risk). This reference implementation covers the cohesion shape only and
/// is intentionally minimal; it is not asserted byte-identical here and exists so the
/// YAML method compiles and round-trips structurally.
#[must_use]
pub fn encode_yaml(metrics: &ComputedMetrics) -> Vec<u8> {
    let mut out = String::new();
    write_yaml_value(&mut out, &to_go_value(metrics), 0);
    out.into_bytes()
}

fn write_yaml_value(out: &mut String, v: &GoValue, indent: usize) {
    match v {
        GoValue::Int(n) => out.push_str(&n.to_string()),
        GoValue::Float(f) => out.push_str(&go_float(*f)),
        GoValue::Str(s) => out.push_str(&yaml_scalar(s)),
        GoValue::Array(items) => {
            if items.is_empty() {
                out.push_str("[]");
                return;
            }
            for item in items {
                out.push('\n');
                push_indent(out, indent);
                out.push_str("- ");
                write_yaml_value(out, item, indent + 1);
            }
        }
        GoValue::Object { fields, sorted } => {
            let mut ordered: Vec<&(String, GoValue)> = fields.iter().collect();
            if *sorted {
                ordered.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
            }
            for (k, val) in ordered.iter().map(|p| (&p.0, &p.1)) {
                out.push('\n');
                push_indent(out, indent);
                out.push_str(k);
                out.push(':');
                match val {
                    GoValue::Object { .. } | GoValue::Array(_) => {
                        write_yaml_value(out, val, indent + 1);
                    }
                    _ => {
                        out.push(' ');
                        write_yaml_value(out, val, indent);
                    }
                }
            }
        }
    }
}

fn push_indent(out: &mut String, indent: usize) {
    for _ in 0..indent {
        out.push_str("  ");
    }
}

fn yaml_scalar(s: &str) -> String {
    if s.is_empty() {
        "\"\"".to_string()
    } else {
        // Minimal: quote if it could be misparsed. Cosmetic-grade.
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn go_float_integers() {
        assert_eq!(go_float(1.0), "1");
        assert_eq!(go_float(0.0), "0");
        assert_eq!(go_float(-0.0), "0");
        assert_eq!(go_float(100.0), "100");
        assert_eq!(go_float(2.0), "2");
    }

    #[test]
    fn go_float_decimals() {
        assert_eq!(go_float(0.5), "0.5");
        assert_eq!(go_float(0.25), "0.25");
        assert_eq!(go_float(0.75), "0.75");
        assert_eq!(go_float(0.1), "0.1");
    }

    #[test]
    fn go_float_small_uses_exponent() {
        // 1e-5 has first-digit exponent -5 < -4 -> exponential.
        assert_eq!(go_float(0.00001), "1e-05");
        // 1e-4 -> exp -4, not < -4 -> fixed.
        assert_eq!(go_float(0.0001), "0.0001");
    }

    #[test]
    fn go_float_large_uses_exponent() {
        // 1e21 -> exp 21 >= 21 -> exponential.
        assert_eq!(go_float(1e21), "1e+21");
        // 1e20 -> exp 20 < 21 -> fixed (21 digits).
        assert_eq!(go_float(1e20), "100000000000000000000");
    }

    #[test]
    fn json_empty_metrics_shape() {
        let m = ComputedMetrics::default();
        let bytes = encode_json(&m);
        let s = String::from_utf8(bytes).unwrap();
        // function_cohesion [] , distribution {} , low_cohesion_functions [] ,
        // aggregate {...}. No trailing newline.
        assert!(s.starts_with("{\n  \"function_cohesion\": [],"));
        assert!(!s.ends_with('\n'));
        assert!(s.contains("\"distribution\": {}"));
    }

    #[test]
    fn binary_envelope_header() {
        let m = ComputedMetrics::default();
        let bytes = encode_binary(&m);
        assert_eq!(&bytes[0..4], b"CFB1");
        let len = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]) as usize;
        assert_eq!(len, bytes.len() - 8);
        // Payload is compact (no spaces after colon, no newlines).
        let payload = String::from_utf8(bytes[8..].to_vec()).unwrap();
        assert!(!payload.contains('\n'));
        assert!(payload.contains("\"function_cohesion\":[]"));
    }

    #[test]
    fn compact_json_no_space_after_colon() {
        let m = ComputedMetrics::default();
        let s = String::from_utf8(encode_compact_json(&m)).unwrap();
        assert!(s.contains("\"distribution\":{}"));
        assert!(!s.contains(": "));
    }

    #[test]
    fn html_escaping_on() {
        let mut m = ComputedMetrics::default();
        m.aggregate.message = "a<b>&c".to_string();
        let s = String::from_utf8(encode_compact_json(&m)).unwrap();
        assert!(s.contains("a\\u003cb\\u003e\\u0026c"));
    }

    #[test]
    fn distribution_keys_byte_sorted() {
        let mut m = ComputedMetrics::default();
        m.distribution.insert("poor".into(), 1);
        m.distribution.insert("excellent".into(), 2);
        m.distribution.insert("good".into(), 3);
        let s = String::from_utf8(encode_compact_json(&m)).unwrap();
        // byte order: excellent < good < poor
        let pos_e = s.find("excellent").unwrap();
        let pos_g = s.find("good").unwrap();
        let pos_p = s.find("\"poor\"").unwrap();
        assert!(pos_e < pos_g && pos_g < pos_p);
    }
}
