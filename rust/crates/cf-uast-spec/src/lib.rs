//! UAST specification and JSON Schema definitions.
//!
//! This crate embeds the canonical UAST JSON Schema (`uast-schema.json`) and a
//! reference example document (`uast-example.json`) so that downstream consumers
//! — principally the `uast validate` subcommand — can load the default schema
//! without depending on a file path on disk.
//!
//! The crate provides:
//!
//! * [`SCHEMA`] / [`schema`] — the raw bytes/text of `uast-schema.json`. This is
//!   the value `uast validate` uses as its built-in default schema.
//! * [`EXAMPLE`] / [`example`] — the raw bytes/text of `uast-example.json`.
//! * [`SCHEMA_FILE_NAME`] / [`EXAMPLE_FILE_NAME`] — the logical file names, so a
//!   path-addressable lookup ([`read_file`]) is available where a caller resolves
//!   the embedded data by name.
//!
//! # Byte-identity
//!
//! The embedded data is included verbatim via [`include_str!`]. No reformatting,
//! re-serialization, or whitespace normalization is performed: the bytes served
//! here are exactly the canonical schema/example files, as required by the
//! CLI compatibility contract. This crate performs **no** report serialization
//! and does not route through the shared report-format serialization crates
//! (`cf-gojson` / `cf-goyaml`).
//!
//! # Examples
//!
//! ```
//! // The default schema is always available and is valid JSON.
//! let schema = cf_uast_spec::schema();
//! assert!(schema.contains("\"$schema\""));
//!
//! // Path-addressable access by logical file name.
//! let by_name = cf_uast_spec::read_file(cf_uast_spec::SCHEMA_FILE_NAME).unwrap();
//! assert_eq!(by_name, cf_uast_spec::SCHEMA.as_bytes());
//! ```

#![forbid(unsafe_code)]

/// Logical file name of the embedded UAST JSON Schema, used by
/// [`read_file`] lookups and by `uast validate`.
pub const SCHEMA_FILE_NAME: &str = "uast-schema.json";

/// Logical file name of the embedded UAST example document.
pub const EXAMPLE_FILE_NAME: &str = "uast-example.json";

/// The canonical UAST JSON Schema, embedded verbatim at compile time.
///
/// This is the built-in default schema used by `uast validate`; the bytes are
/// served unmodified (CLI compatibility contract).
pub const SCHEMA: &str = include_str!("uast-schema.json");

/// A reference UAST document, embedded verbatim at compile time.
///
/// It is a valid instance of [`SCHEMA`] and is useful for documentation, tests,
/// and tooling.
pub const EXAMPLE: &str = include_str!("uast-example.json");

/// Returns the embedded UAST JSON Schema as a string slice.
#[must_use]
#[inline]
pub const fn schema() -> &'static str {
    SCHEMA
}

/// Returns the embedded UAST JSON Schema as raw bytes.
///
/// This is the form most useful to a JSON-schema engine that accepts a byte
/// buffer.
#[must_use]
#[inline]
pub const fn schema_bytes() -> &'static [u8] {
    SCHEMA.as_bytes()
}

/// Returns the embedded UAST example document as a string slice.
#[must_use]
#[inline]
pub const fn example() -> &'static str {
    EXAMPLE
}

/// Returns the embedded UAST example document as raw bytes.
#[must_use]
#[inline]
pub const fn example_bytes() -> &'static [u8] {
    EXAMPLE.as_bytes()
}

/// Reads an embedded file by its logical name.
///
/// This exists so callers structured around a path-addressable embedded
/// filesystem can resolve the data by name. Only the two embedded files
/// ([`SCHEMA_FILE_NAME`] and [`EXAMPLE_FILE_NAME`]) are recognized; any other
/// name yields `None`.
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
        // The embed is a JSON Schema document; it carries a `$schema` key and
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
        // These names are part of the embedded-file lookup contract used by
        // `uast validate`.
        assert_eq!(SCHEMA_FILE_NAME, "uast-schema.json");
        assert_eq!(EXAMPLE_FILE_NAME, "uast-example.json");
    }
}
