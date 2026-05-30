//! Persistence / serialization codecs.
//!
//! This crate is a direct port of the Go `pkg/persist` package. It provides a
//! small [`Codec`] abstraction over encoding and decoding of values to and from
//! byte slices, plus two concrete implementations:
//!
//! * [`JsonCodec`] — a JSON codec whose byte output matches Go's
//!   `encoding/json` encoder with `SetEscapeHTML(true)`: HTML metacharacters and
//!   the Unicode line/paragraph separators are escaped, map keys are emitted in
//!   sorted order, output is optionally pretty-printed, and a trailing newline is
//!   always appended (mirroring `json.Encoder.Encode`).
//! * [`GobCodec`] — a Go-specific binary codec. The original Go implementation
//!   used `encoding/gob`, whose wire format is Go-specific and **not** portable
//!   across languages. There is no gob implementation in the Rust ecosystem, and
//!   gob output was never part of any byte-identity-critical report surface (it
//!   is used only for internal checkpoint persistence). This port therefore
//!   keeps the same "compact, internal, non-portable" contract but backs it with
//!   a Rust-native binary format ([`bincode`]). See the type-level docs for the
//!   compatibility caveats.
//!
//! # Byte-identity
//!
//! The whole point of [`JsonCodec`] is to reproduce Go's JSON bytes exactly so
//! that report serialization stays byte-for-byte compatible during the Rust
//! rewrite. The Go-compat encoding behavior lives in
//! [`json::GoCompatFormatter`], which can be reused by higher-level report
//! serializers that need the same escaping/ordering rules.
//!
//! # Example
//!
//! ```
//! use cf_persist::{Codec, JsonCodec};
//! use serde::{Serialize, Deserialize};
//!
//! #[derive(Serialize, Deserialize, PartialEq, Debug)]
//! struct Sample { name: String, value: i64 }
//!
//! let codec = JsonCodec::compact();
//! let original = Sample { name: "test".into(), value: 42 };
//!
//! let bytes = codec.encode(&original).unwrap();
//! // Go's json.Encoder always appends a trailing newline.
//! assert_eq!(bytes, b"{\"name\":\"test\",\"value\":42}\n");
//!
//! let decoded: Sample = codec.decode(&bytes).unwrap();
//! assert_eq!(decoded, original);
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod error;
mod gob;
pub mod json;

pub use error::{Error, Result};
pub use gob::GobCodec;
pub use json::JsonCodec;

use serde::de::DeserializeOwned;
use serde::Serialize;

/// Abstracts encoding and decoding of values to and from byte slices.
///
/// Implementations may be format-specific (JSON, binary, etc.) and are used to
/// serialize analyzer state, checkpoints, and other persisted structures.
///
/// This mirrors the Go `persist.Codec` interface. Unlike the Go version — which
/// decodes into a caller-provided pointer (`Decode(data, v any) error`) — the
/// Rust API returns the decoded value, which is the idiomatic shape given
/// `serde`'s `DeserializeOwned`.
pub trait Codec {
    /// Serializes `value` into its byte representation.
    fn encode<T: Serialize>(&self, value: &T) -> Result<Vec<u8>>;

    /// Deserializes `data` into a value of type `T`.
    fn decode<T: DeserializeOwned>(&self, data: &[u8]) -> Result<T>;
}
