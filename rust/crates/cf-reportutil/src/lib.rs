//! Shared helpers for analyzers that emit reports (`cf-reportutil`).
//!
//! Rust port of the Go package `internal/analyzers/common/reportutil`. It
//! provides three things, one per module:
//!
//! * [`binary`] — the **CFB1** binary envelope (`binary.go`): a stream of
//!   `[4-byte magic "CFB1"][LE u32 payload length][compact escaped JSON payload]`
//!   records ([`binary::encode_binary_envelope`],
//!   [`binary::decode_binary_envelope`], [`binary::decode_binary_envelopes`]).
//! * [`accessors`] — type-safe accessors over the dynamic `map[string]any`
//!   report model (`reportutil.go`): [`accessors::get`], [`accessors::get_int`],
//!   [`accessors::get_float64`], [`accessors::get_string`],
//!   [`accessors::get_string_slice`], [`accessors::get_functions`],
//!   [`accessors::get_string_int_map`], [`accessors::map_string`].
//! * [`format`] — small scalar formatting helpers (`reportutil.go`):
//!   [`format::format_int`], [`format::format_float`],
//!   [`format::format_percent`], [`format::pct`].
//!
//! # Byte identity
//!
//! The CFB1 payload is the *compact, HTML-escaped* JSON encoding produced by
//! Go's `encoding/json.Marshal`. Per `specs/rust-rewrite/DESIGN.md` (§2.2, §2.5)
//! this crate does **not** use `serde_json`; it routes the payload through the
//! shared [`cf_gojson`] crate's `marshal`, which reproduces Go's defaults
//! exactly: map keys byte-sorted, `<`/`>`/`&` and `U+2028`/`U+2029` escaped, no
//! insignificant whitespace, no trailing newline.
//!
//! Cross-type numeric coercion in [`accessors::get_int`] /
//! [`accessors::get_float64`] delegates to the shared [`cf_safeconv`] crate,
//! mirroring Go's `safeconv.ToInt` / `safeconv.ToFloat64`.

pub mod accessors;
pub mod binary;
pub mod format;

// Flat re-exports so callers can use the package-level names that mirror the Go
// `reportutil` API surface (e.g. `reportutil.GetString`, `reportutil.FormatInt`,
// `reportutil.EncodeBinaryEnvelope`).
pub use accessors::{
    get, get_float64, get_functions, get_int, get_string, get_string_int_map, get_string_slice,
    map_string,
};
pub use binary::{
    decode_binary_envelope, decode_binary_envelopes, encode_binary_envelope, DecodeError,
    EncodeError, BINARY_HEADER_SIZE, BINARY_LENGTH_SIZE, BINARY_MAGIC, MAX_PAYLOAD_SIZE,
};
pub use format::{format_float, format_int, format_percent, pct, PERCENT_MULTIPLIER};

// Re-export the dynamic value model from the serializer so downstream crates can
// build reports without taking a direct `cf-gojson` dependency just for the
// types named in this crate's public signatures.
pub use cf_gojson::{GoMap, GoValue};
