//! `cf-textutil` — byte-level text utilities and the canonical JSON writer
//! for codefang reports.
//!
//! Two groups of functionality:
//!
//! 1. **Byte text helpers** ([`is_binary`], [`count_lines`],
//!    [`BINARY_SNIFF_LENGTH`]) — operate on raw `&[u8]`; no UTF-8 decoding.
//!
//! 2. **Canonical JSON serialization** ([`write_json`], [`marshal_json`]) —
//!    the single entry point for report JSON. Report serialization routes
//!    through the shared contract encoder ([`cf_gojson`]) — never raw
//!    `serde_json` — with HTML escaping ON, an optional two-space indent, and
//!    a trailing newline. See the [`json`] module for the writer and
//!    [`gocompat`] for the encoder re-exports ([`GoValue`] / [`Encoder`]).
//!
//! Compatibility: machine-format report bytes are pinned against the reference
//! implementation by the differential gate in `rust/tests/compat`.
//!
//! # Examples
//!
//! ```
//! use cf_textutil::{count_lines, is_binary, marshal_json};
//! use cf_textutil::{GoMap, GoValue};
//!
//! // Byte text helpers operate on raw `&[u8]`.
//! assert_eq!(count_lines(b"a\nb\nc"), 3);
//! assert!(is_binary(b"x\x00y"));
//!
//! // Canonical compact report JSON, with a trailing newline.
//! let mut m = GoMap::new_struct();
//! m.push("lines", GoValue::Int(3));
//! let bytes = marshal_json(&GoValue::Map(m), false).unwrap();
//! assert_eq!(bytes, b"{\"lines\":3}\n");
//! ```

/// Re-export of the shared contract encoder's surface.
///
/// Historically `cf-textutil` carried an in-crate `gocompat` encoder before
/// `cf-gojson` existed; the module remains as a thin re-export so downstream
/// `use` paths keep working.
pub mod gocompat {
    pub use cf_gojson::{Encoder, GoMap, GoValue, MapOrigin};
}
pub mod json;
pub mod text;

#[doc(inline)]
pub use gocompat::{Encoder, GoMap, GoValue};
#[doc(inline)]
pub use json::{marshal_json, write_json, EncodeError, JsonError, JSON_INDENT};
#[doc(inline)]
pub use text::{count_lines, is_binary, BINARY_SNIFF_LENGTH};
