//! `cf-gojson` — Go `encoding/json` byte-parity value model and marshaller.
//!
//! This tier-0 crate is the keystone of codefang's Rust rewrite: every
//! machine-format report (`json`, `ndjson`, `timeseries`, the CFB1 `bin`
//! payload, and the value tree feeding `cf-goyaml`) is built as a [`GoValue`] and
//! serialized through [`marshal`] / [`Encoder`] so the emitted bytes match the Go
//! binary's `encoding/json` output **byte-for-byte**. See
//! `specs/rust-rewrite/DESIGN.md` §2 for the byte-identity strategy.
//!
//! # Modules
//!
//! * [`value`] — the dynamic [`GoValue`] / [`GoMap`] / [`MapOrigin`] model
//!   (struct fields keep declaration order; map keys byte-sort on encode).
//! * [`ftoa`] — Go-compatible `f64` formatting ([`go_float`] / the `'g'` form),
//!   reproducing `encoding/json`'s float encoder and `strconv.FormatFloat`.
//! * [`marshal`] — the encoder: compact [`marshal`], indented [`marshal_indent`],
//!   and the builder [`Encoder`] (`json.NewEncoder` shape).
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
//! assert_eq!(marshal(&GoValue::Map(m)), br#"{"name":"a<b>","score":0.5}"#);
//! ```

pub mod ftoa;
pub mod marshal;
pub mod value;

pub use ftoa::{format_float_g, format_json_float};
pub use marshal::{
    go_float, marshal, marshal_indent, to_vec, to_vec_indent, write_go_json_string, Encoder,
};
pub use value::{GoMap, GoValue, MapOrigin};

/// Crate name, retained for the workspace link smoke-test.
pub const CRATE_NAME: &str = "cf-gojson";
