//! `cf-gojson` — the report-format JSON value model and serializer.
//!
//! This tier-0 crate is the keystone of codefang's report layer: every
//! machine-format report (`json`, `ndjson`, `timeseries`, the CFB1 `bin`
//! payload, and the value tree feeding `cf-goyaml`) is built as a [`GoValue`]
//! and serialized through [`marshal`] / [`Encoder`]. The emitted bytes are a
//! frozen contract — internals may be tidied, but the encoding rules must not
//! change. See `specs/rust-rewrite/DESIGN.md` §2 for the byte-identity
//! strategy.
//!
//! Compatibility: output bytes are pinned against the reference implementation
//! by the differential gate in `tests/compat`.
//!
//! # Modules
//!
//! * [`value`] — the dynamic [`GoValue`] / [`GoMap`] / [`MapOrigin`] model
//!   (struct-origin objects keep declaration order; map-origin objects
//!   byte-sort keys on encode).
//! * [`ftoa`] — contract `f64` formatting ([`go_float`] for JSON numbers,
//!   [`format_float_g`] for the `'g'` layout): shortest round-trip digits,
//!   re-rendered with the contract layout rules.
//! * [`marshal`] — the encoder: compact [`marshal`], indented
//!   [`marshal_indent`], and the configurable builder [`Encoder`].
//!
//! # Quick start
//!
//! ```
//! use cf_gojson::{GoMap, GoValue, MapOrigin, marshal};
//!
//! let mut m = GoMap::new(MapOrigin::Map);
//! m.push("score", GoValue::Float(0.5));
//! m.push("name", GoValue::Str("a<b>".into()));
//! // map-origin keys byte-sort ("name" < "score"); '<','>' are HTML-escaped.
//! assert_eq!(marshal(&GoValue::Map(m)), br#"{"name":"a\u003cb\u003e","score":0.5}"#);
//! ```

pub mod ftoa;
pub mod marshal;
pub mod value;

pub use ftoa::{format_float_g, format_json_float};
pub use marshal::{
    go_float, marshal, marshal_indent, to_vec, to_vec_indent, write_go_json_string,
    write_go_json_string_opts, Encoder,
};
pub use value::{GoMap, GoValue, MapOrigin};

/// Crate name, retained for the workspace link smoke-test.
pub const CRATE_NAME: &str = "cf-gojson";
