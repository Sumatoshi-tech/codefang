//! UAST specification and JSON Schema definitions.
//!
//! This crate is the Rust port of the Go package `pkg/uast/pkg/spec`. Its sole
//! purpose is to embed the canonical UAST JSON Schema (`uast-schema.json`) and a
//! reference example document (`uast-example.json`) so that downstream consumers
//! — principally the `uast validate` subcommand — can load the default schema
//! without depending on a file path on disk.
//!
//! # Relationship to the Go source
//!
//! The Go package exposes the schema through an embedded filesystem produced by
//! `//go:embed` (in `schemafs.go`):
//!
//! ```go
//! //go:embed uast-schema.json
//! var UASTSchemaFS embed.FS
//! ```
//!
//! Go's `embed.FS` is a read-only, path-addressable view over files baked into
//! the binary at compile time. The single caller, `cmd/uast/validate.go`, reads
//! the schema with `spec.UASTSchemaFS.ReadFile("uast-schema.json")`.
//!
//! Rust has no direct `embed.FS` equivalent, so this crate provides:
//!
//! * [`SCHEMA`] / [`schema`] — the raw bytes/text of `uast-schema.json`,
//!   byte-identical to the file the Go binary embeds. This is the value
//!   `uast validate` uses as its built-in default schema.
//! * [`EXAMPLE`] / [`example`] — the raw bytes/text of `uast-example.json`.
//! * [`SCHEMA_FILE_NAME`] / [`EXAMPLE_FILE_NAME`] — the logical file names used by
//!   the Go embed, so a path-addressable shim ([`read_file`]) can mimic the
//!   `embed.FS.ReadFile` lookup behaviour where a caller insists on going through
//!   a name (exactly as `validate.go` does).
//!
//! # Byte-identity
//!
//! The embedded data is included verbatim via [`include_str!`] from copies of the
//! exact source files used by the Go build. No reformatting, re-serialization, or
//! whitespace normalization is performed, so the bytes returned here match the Go
//! embed byte-for-byte. This crate therefore performs **no** report serialization
//! and does not route through the shared go-compat serialization crates
//! (`cf-gojson` / `cf-goyaml`): it only hands back the canonical bytes it was
//! given.
//!
//! # Examples
//!
//! ```
//! // The default schema is always available and is valid JSON.
//! let schema = cf_uast_spec::schema();
//! assert!(schema.contains("\"$schema\""));
//!
//! // Path-addressable access mirrors Go's embed.FS.ReadFile.
//! let by_name = cf_uast_spec::read_file(cf_uast_spec::SCHEMA_FILE_NAME).unwrap();
//! assert_eq!(by_name, cf_uast_spec::SCHEMA.as_bytes());
//! ```

#![forbid(unsafe_code)]

/// Logical file name of the embedded UAST JSON Schema, matching the name used by
/// the Go `//go:embed` directive in `schemafs.go` and the
/// `UASTSchemaFS.ReadFile("uast-schema.json")` call in `cmd/uast/validate.go`.
pub const SCHEMA_FILE_NAME: &str = "uast-schema.json";

/// Logical file name of the embedded UAST example document.
pub const EXAMPLE_FILE_NAME: &str = "uast-example.json";

/// The canonical UAST JSON Schema, embedded verbatim at compile time.
///
/// This is byte-identical to `pkg/uast/pkg/spec/uast-schema.json` in the Go
/// source tree and is the built-in default schema used by `uast validate`.
pub const SCHEMA: &str = include_str!("uast-schema.json");

/// A reference UAST document, embedded verbatim at compile time.
///
/// This is byte-identical to `pkg/uast/pkg/spec/uast-example.json`. It is a valid
/// instance of [`SCHEMA`] and is useful for documentation, tests, and tooling.
pub const EXAMPLE: &str = include_str!("uast-example.json");

/// Returns the embedded UAST JSON Schema as a string slice.
///
/// Equivalent to reading `uast-schema.json` from the Go embedded filesystem. The
/// returned bytes are byte-identical to the Go embed.
#[must_use]
#[inline]
pub fn schema() -> &'static str {
    SCHEMA
}

/// Returns the embedded UAST JSON Schema as raw bytes.
///
/// This is the form most useful to a JSON-schema engine that accepts a byte
/// buffer (the Go code passes the embed bytes to `gojsonschema.NewBytesLoader`).
#[must_use]
#[inline]
pub fn schema_bytes() -> &'static [u8] {
    SCHEMA.as_bytes()
}

/// Returns the embedded UAST example document as a string slice.
#[must_use]
#[inline]
pub fn example() -> &'static str {
    EXAMPLE
}

/// Returns the embedded UAST example document as raw bytes.
#[must_use]
#[inline]
pub fn example_bytes() -> &'static [u8] {
    EXAMPLE.as_bytes()
}

/// Reads an embedded file by its logical name, mirroring Go's
/// `embed.FS.ReadFile`.
///
/// This exists so callers structured around a path-addressable embedded
/// filesystem (as the Go code is) can be ported with minimal changes. Only the
/// two embedded files ([`SCHEMA_FILE_NAME`] and [`EXAMPLE_FILE_NAME`]) are
/// recognized; any other name yields `None`, analogous to the file-not-found
/// error returned by `embed.FS.ReadFile` for unknown paths.
///
/// # Examples
///
/// ```
/// assert!(cf_uast_spec::read_file("uast-schema.json").is_some());
/// assert!(cf_uast_spec::read_file("does-not-exist").is_none());
/// ```
#[must_use]
pub fn read_file(name: &str) -> Option<&'static [u8]> {
    match name {
        SCHEMA_FILE_NAME => Some(SCHEMA.as_bytes()),
        EXAMPLE_FILE_NAME => Some(EXAMPLE.as_bytes()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_is_non_empty_and_accessors_agree() {
        assert!(!SCHEMA.is_empty(), "embedded schema must not be empty");
        assert_eq!(schema(), SCHEMA);
        assert_eq!(schema_bytes(), SCHEMA.as_bytes());
    }

    #[test]
    fn example_is_non_empty_and_accessors_agree() {
        assert!(!EXAMPLE.is_empty(), "embedded example must not be empty");
        assert_eq!(example(), EXAMPLE);
        assert_eq!(example_bytes(), EXAMPLE.as_bytes());
    }

    #[test]
    fn schema_is_valid_json_with_expected_shape() {
        let v: serde_json::Value =
            serde_json::from_str(SCHEMA).expect("embedded schema must be valid JSON");
        // The Go schema is a JSON Schema document; it carries a `$schema` key and
        // describes the UAST node object. These structural assertions guard
        // against accidental truncation/corruption of the embedded bytes.
        assert!(v.get("$schema").is_some(), "schema must declare $schema");
        assert!(
            v.get("type").is_some() || v.get("properties").is_some() || v.get("$ref").is_some(),
            "schema must describe a type"
        );
        // The `type` property is required by the spec; confirm it is in `required`.
        let required = v.get("required").and_then(|r| r.as_array());
        assert!(
            required.is_some_and(|arr| arr.iter().any(|x| x == "type")),
            "schema must require the `type` field"
        );
    }

    #[test]
    fn example_is_valid_json_and_conforms_minimally() {
        let v: serde_json::Value =
            serde_json::from_str(EXAMPLE).expect("embedded example must be valid JSON");
        // The example is a single UAST node; per the spec the root must carry a
        // `type`. (Full schema validation is the job of `uast validate`; here we
        // only assert the embed is intact and structurally sane.)
        assert!(
            v.get("type").is_some(),
            "example root node must have a `type` field"
        );
    }

    #[test]
    fn read_file_matches_constants() {
        assert_eq!(read_file(SCHEMA_FILE_NAME), Some(SCHEMA.as_bytes()));
        assert_eq!(read_file(EXAMPLE_FILE_NAME), Some(EXAMPLE.as_bytes()));
        assert_eq!(read_file("nope.json"), None);
        assert_eq!(read_file(""), None);
    }

    #[test]
    fn file_name_constants_are_stable() {
        // These names are part of the embed contract ported from schemafs.go and
        // the ReadFile call in validate.go.
        assert_eq!(SCHEMA_FILE_NAME, "uast-schema.json");
        assert_eq!(EXAMPLE_FILE_NAME, "uast-example.json");
    }
}
