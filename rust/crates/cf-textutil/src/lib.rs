//! `cf-textutil` — byte-level text utilities and the canonical Go-compatible
//! JSON writer for codefang reports.
//!
//! Rust port of the Go package `pkg/textutil`. It provides two groups of
//! functionality:
//!
//! 1. **Byte text helpers** ([`is_binary`], [`count_lines`],
//!    [`BINARY_SNIFF_LENGTH`]) — operate on raw `&[u8]`, mirroring Go's
//!    `[]byte` semantics exactly.
//!
//! 2. **Canonical JSON serialization** ([`write_json`], [`marshal_json`]) — the
//!    single source of truth for report JSON. Per the rewrite design, report
//!    serialization routes through a shared Go-byte-compatible encoder (never
//!    raw `serde_json`) so MACHINE-format report bytes stay byte-identical with
//!    Go: HTML escaping ON, optional two-space indent, trailing newline. See
//!    the [`json`] module for the writer and [`gocompat`] for the encoder
//!    ([`GoValue`] / [`Encoder`]).
//!
//! # Byte-identity
//!
//! Byte-identity of MACHINE-format report bytes is the project goal. The JSON
//! writer here matches Go's `json.NewEncoder` + `SetIndent("", "  ")` +
//! `Encode` call site (`pkg/textutil/textutil.go`).
//!
//! # Encoder routing (`cf-gojson`)
//!
//! The design's single Go-byte-compatible encoder is the tier-0 crate
//! `cf-gojson`, which `cf-textutil` is meant to wrap. While `cf-gojson` remains
//! a scaffold, the encoder lives in the in-crate [`gocompat`] module with an
//! API identical to the planned `cf-gojson` surface, so the migration is a
//! one-line `use` swap once `cf-gojson` is implemented.

/// Compatibility alias for the shared Go-byte-compatible encoder.
///
/// Historically `cf-textutil` carried an in-crate `gocompat` encoder while
/// `cf-gojson` was still a scaffold. `cf-gojson` is now the implemented tier-0
/// encoder, so `gocompat` is a thin re-export of its surface to keep the
/// migration a one-line `use` swap for downstream call sites.
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
