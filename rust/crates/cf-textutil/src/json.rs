//! Canonical Go-compatible JSON writer (`WriteJSON`).
//!
//! Port of Go `pkg/textutil/textutil.go`'s `WriteJSON`, which is the canonical
//! report JSON writer used across codefang. Per `specs/rust-rewrite/DESIGN.md`
//! (§1.1, §2.2, §2.3 row "json (textutil)"), report serialization MUST route
//! through the shared Go-byte-compatible encoder — never raw `serde_json` — so
//! MACHINE-format report bytes stay byte-identical with the Go implementation.
//!
//! The shared encoder is the tier-0 crate `cf-gojson`. At the time this crate
//! was ported, `cf-gojson` is still a scaffold (exports only `CRATE_NAME`), so
//! `cf-textutil` cannot yet compile against it. As the design's "define the
//! minimal interface" fallback, this module routes through the in-crate
//! [`crate::gocompat`] encoder, which reproduces the identical Go bytes and
//! exposes the identical `Encoder` / `GoValue` / `EncodeError` surface. When
//! `cf-gojson` is implemented, switch the `use` below to
//! `use cf_gojson::{Encoder, GoValue};` and drop `gocompat`.
//!
//! # Encoding rules (reproduced from Go `encoding/json`)
//!
//! `WriteJSON` mirrors `json.NewEncoder(w)` + (optionally)
//! `enc.SetIndent("", "  ")` + `enc.Encode(v)`:
//!
//! - HTML escaping is **enabled** (`<`, `>`, `&` and U+2028 / U+2029 escaped).
//! - When `pretty` is `true`, output is indented with two spaces and a single
//!   space follows each `:`; empty containers collapse to `{}` / `[]`.
//! - A trailing newline is always written (`Encoder.Encode` appends one `\n`).
//!
//! These rules are all implemented by [`crate::gocompat::Encoder`]; this module
//! is a thin, faithful wrapper that selects the encoder configuration matching
//! the Go `WriteJSON` call site.

use std::io::Write;

use crate::gocompat::{Encoder, GoValue};

/// Two-space indentation string used for pretty-printed JSON output.
///
/// Port of the Go `jsonIndent` constant.
pub const JSON_INDENT: &str = "  ";

/// Error returned by [`write_json`] / [`marshal_json`].
///
/// Mirrors the single `error` return of Go `WriteJSON`, distinguishing
/// encoding failures (e.g. non-finite floats, which Go's `encoding/json`
/// rejects) from writer I/O failures.
#[derive(Debug)]
pub enum JsonError {
    /// The value could not be encoded to Go-compatible JSON.
    Encode(crate::gocompat::EncodeError),
    /// Writing the encoded bytes to the destination writer failed.
    Io(std::io::Error),
}

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

impl From<crate::gocompat::EncodeError> for JsonError {
    fn from(e: crate::gocompat::EncodeError) -> Self {
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
/// `cf-textutil` so that report bytes are identical across every call site.
///
/// # Errors
///
/// Returns [`JsonError::Encode`] if `v` cannot be encoded and [`JsonError::Io`]
/// if writing to `w` fails.
///
/// # Examples
///
/// ```
/// use cf_textutil::{write_json, GoValue};
///
/// let v = GoValue::object([("a", GoValue::Int(1))]);
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
/// Returns [`JsonError::Encode`] if `v` cannot be encoded.
///
/// # Examples
///
/// ```
/// use cf_textutil::{marshal_json, GoValue};
///
/// let v = GoValue::object([("a", GoValue::Int(1))]);
/// assert_eq!(marshal_json(&v, false).unwrap(), b"{\"a\":1}\n");
/// ```
pub fn marshal_json(v: &GoValue, pretty: bool) -> Result<Vec<u8>, JsonError> {
    // Configure cf-gojson to match Go's `json.Encoder`:
    //   - escape_html = true   (Go default, never SetEscapeHTML(false))
    //   - trailing_newline = true (Encoder.Encode always appends one '\n')
    //   - indent = Some("  ") when pretty (SetIndent("", "  ")), else None.
    let mut enc = Encoder::new();
    enc.set_escape_html(true);
    enc.set_trailing_newline(true);
    if pretty {
        enc.set_indent(Some(JSON_INDENT));
    } else {
        enc.set_indent(None);
    }
    Ok(enc.encode(v)?)
}

#[cfg(test)]
mod tests {
    use super::{marshal_json, write_json};
    use crate::gocompat::GoValue;

    fn obj_a1() -> GoValue {
        GoValue::object([("a", GoValue::Int(1))])
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
    // Go errors on channels; the closest cf-gojson analogue is a non-finite
    // float, which Go's encoding/json also rejects.
    #[test]
    fn test_write_json_error_on_unsupported_value() {
        let mut buf = Vec::new();
        let bad = GoValue::object([("x", GoValue::Float(f64::NAN))]);
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
        let v = GoValue::object([("k", GoValue::str("<a>&"))]);
        let s = String::from_utf8(marshal_json(&v, false).unwrap()).unwrap();
        assert!(s.contains("\\u003c"), "got {s}");
        assert!(s.contains("\\u003e"), "got {s}");
        assert!(s.contains("\\u0026"), "got {s}");
        assert!(s.ends_with('\n'), "missing trailing newline: {s:?}");
    }
}
