//! Shared helpers for analyzers that emit reports (`cf-reportutil`).
//!
//! Three things, one per module:
//!
//! * [`binary`] — the **CFB1** binary envelope: a stream of
//!   `[4-byte magic "CFB1"][LE u32 payload length][compact escaped JSON payload]`
//!   records ([`binary::encode_binary_envelope`],
//!   [`binary::decode_binary_envelope`], [`binary::decode_binary_envelopes`]).
//! * [`accessors`] — type-safe accessors over the dynamic report map:
//!   [`accessors::get`], [`accessors::get_int`],
//!   [`accessors::get_float64`], [`accessors::get_string`],
//!   [`accessors::get_string_slice`], [`accessors::get_functions`],
//!   [`accessors::get_string_int_map`], [`accessors::map_string`].
//! * [`format`] — small scalar formatting helpers:
//!   [`format::format_int`], [`format::format_float`],
//!   [`format::format_percent`], [`format::pct`].
//!
//! # Byte identity
//!
//! The CFB1 payload is the *compact, HTML-escaped* report-contract JSON
//! encoding. Per `specs/rust-rewrite/DESIGN.md` (§2.2, §2.5) this crate does
//! **not** use `serde_json`; it routes the payload through the shared
//! [`cf_gojson`] crate's `marshal` (map keys byte-sorted, `<`/`>`/`&` and
//! `U+2028`/`U+2029` escaped, no insignificant whitespace, no trailing
//! newline). Output bytes are pinned against the reference implementation by
//! `rust/tests/compat`.
//!
//! Cross-type numeric coercion in [`accessors::get_int`] /
//! [`accessors::get_float64`] delegates to the shared [`cf_safeconv`] crate.

pub mod accessors;
pub mod binary;
pub mod format;

// Flat re-exports so callers can use short package-level names
// (`cf_reportutil::get_string`, `cf_reportutil::format_int`,
// `cf_reportutil::encode_binary_envelope`, …).
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
