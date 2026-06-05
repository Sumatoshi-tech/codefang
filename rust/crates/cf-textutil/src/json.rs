//! Canonical Go-compatible JSON writer (`WriteJSON`).
//!
//! Port of Go `pkg/textutil/textutil.go`'s `WriteJSON`, the canonical report
//! JSON writer used across codefang. Per `specs/rust-rewrite/DESIGN.md`
//! (§1.1, §2, §3.2 row "json"), report serialization MUST route through the
//! shared Go-byte-compatible encoder — never raw `serde_json` — so
//! MACHINE-format report bytes stay byte-identical with the Go implementation.
//!
//! The shared encoder is the tier-0 crate [`cf_gojson`]. This module is a thin,
//! faithful wrapper selecting the [`cf_gojson::Encoder`] configuration that
//! matches the Go `WriteJSON` call site exactly.
//!
//! # Encoding rules (reproduced from Go `encoding/json`)
//!
//! `WriteJSON` mirrors `json.NewEncoder(w)` + (optionally)
//! `enc.SetIndent("", "  ")` + `enc.Encode(v)`:
//!
//! - HTML escaping is **enabled** (`<`, `>`, `&` and U+2028 / U+2029 escaped) —
//!   this is `cf_gojson`'s only (Go-default) mode.
//! - When `pretty` is `true`, output is indented with two spaces and a single
//!   space follows each `:`; empty containers collapse to `{}` / `[]`.
//! - A trailing newline is always written (`Encoder.Encode` appends one `\n`).
//!
//! # The error path
//!
//! Go's `WriteJSON` returns `error` because `json.Encoder.Encode` can fail —
//! most relevantly on a non-finite float (`NaN`/`±Inf`), which `encoding/json`
//! rejects with `json: unsupported value`. `cf_gojson`'s encoder is infallible
//! and assumes finite floats (its float formatter is documented as requiring
//! finite input), so this wrapper performs the same validity check Go's encoder
//! does *before* encoding, preserving the fallible `Result` signature and the
//! `IsBinary`-adjacent test parity (`TestWriteJSON_ErrorOnUnsupportedType`).

use std::io::Write;

use cf_gojson::{Encoder, GoValue};

/// Two-space indentation string used for pretty-printed JSON output.
///
/// Port of the Go `jsonIndent` constant.
pub const JSON_INDENT: &str = "  ";

/// Error returned by [`write_json`] / [`marshal_json`].
///
/// Mirrors the single `error` return of Go `WriteJSON`, distinguishing
/// encoding failures (non-finite floats, which Go's `encoding/json` rejects)
/// from writer I/O failures.
#[derive(Debug)]
pub enum JsonError {
    /// The value could not be encoded to Go-compatible JSON.
    ///
    /// Carries the offending non-finite float, matching the only unsupported
    /// value `cf_gojson`'s value model can represent (Go's `encoding/json`
    /// raises `json: unsupported value` for the same input).
    Encode(EncodeError),
    /// Writing the encoded bytes to the destination writer failed.
    Io(std::io::Error),
}

/// An encoding failure analogous to Go's `json: unsupported value`.
///
/// The only value kind in [`cf_gojson::GoValue`] that Go's `encoding/json`
/// refuses is a non-finite float (`NaN` / `±Inf`).
#[derive(Debug, Clone, PartialEq)]
pub enum EncodeError {
    /// A non-finite float (`NaN` or infinity) was encountered.
    UnsupportedFloat(f64),
}

impl std::fmt::Display for EncodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            // Wording mirrors Go's "json: unsupported value: <float>".
            EncodeError::UnsupportedFloat(v) => write!(f, "json: unsupported value: {v}"),
        }
    }
}

impl std::error::Error for EncodeError {}

impl std::fmt::Display for JsonError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            // Matches Go's "encode JSON: %w" wrapping wording.
            JsonError::Encode(e) => write!(f, "encode JSON: {e}"),
            JsonError::Io(e) => write!(f, "encode JSON: {e}"),
        }
    }
}

impl std::error::Error for JsonError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            JsonError::Encode(e) => Some(e),
            JsonError::Io(e) => Some(e),
        }
    }
}

impl From<EncodeError> for JsonError {
    fn from(e: EncodeError) -> Self {
        JsonError::Encode(e)
    }
}

impl From<std::io::Error> for JsonError {
    fn from(e: std::io::Error) -> Self {
        JsonError::Io(e)
    }
}

/// Encodes `v` as JSON to `w` using the canonical codefang encoding.
///
/// Port of Go `WriteJSON`. If `pretty` is `true`, output is indented with two
/// spaces. HTML escaping is enabled and a trailing newline is always written.
/// All encoding is performed by [`cf_gojson::Encoder`] so the bytes are
/// byte-identical with Go `json.Encoder`.
///
/// This is the single source of truth for JSON report serialization in
/// `cf-textutil` so report bytes are identical across every call site.
///
/// # Errors
///
/// Returns [`JsonError::Encode`] if `v` contains a non-finite float (Go's
/// `encoding/json` rejects these) and [`JsonError::Io`] if writing to `w` fails.
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
    // Reproduce Go's `Encoder.Encode` failure on non-finite floats before
    // handing the value to `cf_gojson` (whose float formatter assumes finite
    // input). Go rejects the *whole* encode if any value is NaN/±Inf.
    if let Some(bad) = find_non_finite_float(v) {
        return Err(JsonError::Encode(EncodeError::UnsupportedFloat(bad)));
    }

    // Configure cf-gojson to match Go's `json.NewEncoder` call site in
    // `pkg/textutil`:
    //   - HTML escaping on   (cf-gojson's only, Go-default mode)
    //   - trailing newline   (Encoder.Encode always appends one '\n')
    //   - indent = "  " when pretty (SetIndent("", "  ")), else compact.
    let enc = if pretty {
        Encoder::indented(JSON_INDENT).with_trailing_newline(true)
    } else {
        Encoder::encoder() // compact + trailing newline
    };
    Ok(enc.encode(v))
}

/// Walks `v` and returns the first non-finite float (`NaN`/`±Inf`) it contains,
/// if any. Mirrors the value `encoding/json` would reject.
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

    // Ported from Go TestWriteJSON_PrettyOutput.
    #[test]
    fn test_write_json_pretty_output() {
        let mut buf = Vec::new();
        write_json(&mut buf, &obj_a1(), true).unwrap();
        assert_eq!(String::from_utf8(buf).unwrap(), "{\n  \"a\": 1\n}\n");
    }

    // Ported from Go TestWriteJSON_CompactOutput.
    #[test]
    fn test_write_json_compact_output() {
        let mut buf = Vec::new();
        write_json(&mut buf, &obj_a1(), false).unwrap();
        assert_eq!(String::from_utf8(buf).unwrap(), "{\"a\":1}\n");
    }

    // Ported from Go TestWriteJSON_ErrorOnUnsupportedType.
    // Go errors on channels; the closest representable analogue in the value
    // model is a non-finite float, which Go's encoding/json also rejects.
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
        // <, >, & must be \u-escaped (Go default HTML escaping).
        let mut m = GoMap::new_struct();
        m.push("k", GoValue::Str("<a>&".into()));
        let v = GoValue::Map(m);
        let s = String::from_utf8(marshal_json(&v, false).unwrap()).unwrap();
        assert!(s.contains("\\u003c"), "got {s}");
        assert!(s.contains("\\u003e"), "got {s}");
        assert!(s.contains("\\u0026"), "got {s}");
        assert!(s.ends_with('\n'), "missing trailing newline: {s:?}");
    }

    // Map-origin objects byte-sort their keys, exactly like Go's map encoder.
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
