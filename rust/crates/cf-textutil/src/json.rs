//! The canonical report JSON writer.
//!
//! Per `specs/rust-rewrite/DESIGN.md` (§1.1, §2, §3.2 row "json"), report
//! serialization MUST route through the shared contract encoder — never raw
//! `serde_json` — so machine-format report bytes stay pinned (differential
//! gate: `rust/tests/compat`).
//!
//! The shared encoder is the tier-0 crate [`cf_gojson`]. This module is a
//! thin wrapper selecting the [`cf_gojson::Encoder`] configuration every
//! report JSON surface uses.
//!
//! # Encoding rules (report contract)
//!
//! - HTML escaping is **enabled** (`<`, `>`, `&` and U+2028 / U+2029 escaped).
//! - When `pretty` is `true`, output is indented with two spaces and a single
//!   space follows each `:`; empty containers collapse to `{}` / `[]`.
//! - A trailing newline is always written (one value per line).
//!
//! # The error path
//!
//! The contract rejects non-finite floats (`NaN`/`±Inf`) with the
//! `json: unsupported value` error. [`cf_gojson`]'s encoder is infallible and
//! assumes finite floats (its float formatter requires finite input), so this
//! wrapper performs that validity check *before* encoding, preserving the
//! fallible `Result` signature.

use std::io::Write;

use cf_gojson::{Encoder, GoValue};

/// Two-space indentation string used for pretty-printed JSON output.
pub const JSON_INDENT: &str = "  ";

/// Error returned by [`write_json`] / [`marshal_json`].
///
/// Distinguishes encoding failures (non-finite floats, which the report
/// contract rejects) from writer I/O failures. The `"encode JSON: "` prefix
/// is part of the CLI error contract; keep the wording stable.
#[derive(Debug, thiserror::Error)]
pub enum JsonError {
    /// The value could not be encoded to report-contract JSON.
    ///
    /// Carries the offending non-finite float — the only unsupported value
    /// the [`cf_gojson::GoValue`] model can represent.
    #[error("encode JSON: {0}")]
    Encode(#[from] EncodeError),
    /// Writing the encoded bytes to the destination writer failed.
    #[error("encode JSON: {0}")]
    Io(#[from] std::io::Error),
}

/// An encoding failure on an unsupported value.
///
/// The wording (`json: unsupported value: …`) is part of the CLI error
/// contract; keep it stable.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum EncodeError {
    /// A non-finite float (`NaN` or infinity) was encountered.
    #[error("json: unsupported value: {0}")]
    UnsupportedFloat(f64),
}

/// Encodes `v` as JSON to `w` using the canonical codefang encoding.
///
/// If `pretty` is `true`, output is indented with two spaces. HTML escaping is
/// enabled and a trailing newline is always written. All encoding is performed
/// by [`cf_gojson::Encoder`], the contract encoder.
///
/// This is the single source of truth for JSON report serialization in
/// `cf-textutil` so report bytes are identical across every call site.
///
/// # Errors
///
/// Returns [`JsonError::Encode`] if `v` contains a non-finite float (the
/// report contract rejects these) and [`JsonError::Io`] if writing to `w`
/// fails.
///
/// # Examples
///
/// ```
/// use cf_textutil::write_json;
/// use cf_gojson::{GoMap, GoValue};
///
/// let mut m = GoMap::new_struct();
/// m.push("a", GoValue::Int(1));
/// let v = GoValue::Map(m);
/// let mut buf = Vec::new();
/// write_json(&mut buf, &v, true).unwrap();
/// assert_eq!(String::from_utf8(buf).unwrap(), "{\n  \"a\": 1\n}\n");
/// ```
pub fn write_json<W: Write>(mut w: W, v: &GoValue, pretty: bool) -> Result<(), JsonError> {
    let bytes = marshal_json(v, pretty)?;
    w.write_all(&bytes)?;
    Ok(())
}

/// Returns the canonical JSON encoding of `v` as a byte vector, using the same
/// rules as [`write_json`] (HTML escaping on, optional two-space indent,
/// trailing newline).
///
/// # Errors
///
/// Returns [`JsonError::Encode`] if `v` contains a non-finite float.
///
/// # Examples
///
/// ```
/// use cf_textutil::marshal_json;
/// use cf_gojson::{GoMap, GoValue};
///
/// let mut m = GoMap::new_struct();
/// m.push("a", GoValue::Int(1));
/// assert_eq!(marshal_json(&GoValue::Map(m), false).unwrap(), b"{\"a\":1}\n");
/// ```
pub fn marshal_json(v: &GoValue, pretty: bool) -> Result<Vec<u8>, JsonError> {
    // Reject non-finite floats before handing the value to `cf_gojson` (whose
    // float formatter assumes finite input). The *whole* encode fails if any
    // value is NaN/±Inf (report contract).
    if let Some(bad) = find_non_finite_float(v) {
        return Err(JsonError::Encode(EncodeError::UnsupportedFloat(bad)));
    }

    // The contract encoder configuration:
    //   - HTML escaping on   (the default mode)
    //   - trailing newline   (always appended)
    //   - indent = "  " when pretty, else compact.
    let enc = if pretty {
        Encoder::indented(JSON_INDENT).with_trailing_newline(true)
    } else {
        Encoder::encoder() // compact + trailing newline
    };
    Ok(enc.encode(v))
}

/// Walks `v` and returns the first non-finite float (`NaN`/`±Inf`) it
/// contains, if any — the value the report contract rejects.
fn find_non_finite_float(v: &GoValue) -> Option<f64> {
    match v {
        GoValue::Float(f) if !f.is_finite() => Some(*f),
        GoValue::Array(items) => items.iter().find_map(find_non_finite_float),
        GoValue::Map(m) => m.values().find_map(find_non_finite_float),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{marshal_json, write_json};
    use cf_gojson::{GoMap, GoValue};

    fn obj_a1() -> GoValue {
        let mut m = GoMap::new_struct();
        m.push("a", GoValue::Int(1));
        GoValue::Map(m)
    }

    // Reference suite: TestWriteJSON_PrettyOutput.
    #[test]
    fn test_write_json_pretty_output() {
        let mut buf = Vec::new();
        write_json(&mut buf, &obj_a1(), true).unwrap();
        assert_eq!(String::from_utf8(buf).unwrap(), "{\n  \"a\": 1\n}\n");
    }

    // Reference suite: TestWriteJSON_CompactOutput.
    #[test]
    fn test_write_json_compact_output() {
        let mut buf = Vec::new();
        write_json(&mut buf, &obj_a1(), false).unwrap();
        assert_eq!(String::from_utf8(buf).unwrap(), "{\"a\":1}\n");
    }

    // Reference suite: TestWriteJSON_ErrorOnUnsupportedType (which uses an
    // unencodable value; the closest representable analogue in this value
    // model is a non-finite float, which the contract also rejects).
    #[test]
    fn test_write_json_error_on_unsupported_value() {
        let mut buf = Vec::new();
        let mut m = GoMap::new_struct();
        m.push("x", GoValue::Float(f64::NAN));
        let bad = GoValue::Map(m);
        assert!(write_json(&mut buf, &bad, false).is_err());
        // Nothing should be written on the error path.
        assert!(buf.is_empty());
    }

    #[test]
    fn test_write_json_error_on_infinity_nested_in_array() {
        let mut buf = Vec::new();
        let bad = GoValue::Array(vec![GoValue::Int(1), GoValue::Float(f64::INFINITY)]);
        assert!(write_json(&mut buf, &bad, false).is_err());
    }

    #[test]
    fn test_marshal_json_matches_write_json() {
        let marshalled = marshal_json(&obj_a1(), false).unwrap();
        let mut buf = Vec::new();
        write_json(&mut buf, &obj_a1(), false).unwrap();
        assert_eq!(marshalled, buf);
    }

    #[test]
    fn test_write_json_html_escaping_enabled() {
        // <, >, & must be \u-escaped (default HTML escaping).
        let mut m = GoMap::new_struct();
        m.push("k", GoValue::Str("<a>&".into()));
        let v = GoValue::Map(m);
        let s = String::from_utf8(marshal_json(&v, false).unwrap()).unwrap();
        assert!(s.contains("\\u003c"), "got {s}");
        assert!(s.contains("\\u003e"), "got {s}");
        assert!(s.contains("\\u0026"), "got {s}");
        assert!(s.ends_with('\n'), "missing trailing newline: {s:?}");
    }

    // Map-origin objects byte-sort their keys (report-contract key order).
    #[test]
    fn test_write_json_map_keys_byte_sorted() {
        let mut m = GoMap::new_map();
        m.push("b", GoValue::Int(2));
        m.push("a", GoValue::Int(1));
        m.push("C", GoValue::Int(3));
        let s = String::from_utf8(marshal_json(&GoValue::Map(m), false).unwrap()).unwrap();
        assert_eq!(s, "{\"C\":3,\"a\":1,\"b\":2}\n");
    }
}
