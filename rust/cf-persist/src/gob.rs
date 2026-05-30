//! Go-specific binary codec (gob-equivalent).
//!
//! The Go `persist.GobCodec` used `encoding/gob`. Gob's wire format is
//! self-describing but **Go-specific** — it encodes Go type information and is
//! explicitly documented as not portable to other languages. There is no gob
//! encoder/decoder in the Rust ecosystem, so a byte-for-byte port is impossible.
//!
//! Crucially, gob output was never part of any byte-identity-critical *report*
//! surface (the formats the rewrite must reproduce exactly are json, yaml,
//! ndjson, timeseries, compact and bin). Gob was only used for **internal**
//! persistence such as checkpoints, where the contract is merely "compact,
//! efficient, written and read back by the same program." This port preserves
//! that contract using [`bincode`], a fast Rust-native binary format.
//!
//! See the crate-level [todo] list: when checkpoint/common are ported, confirm
//! that no on-disk gob artifacts need to be read by the Rust binary. If
//! cross-version (Go-written) checkpoints must be loaded, a dedicated gob reader
//! will be required and this codec is not sufficient.
//!
//! [todo]: crate

use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::error::{Error, Result};
use crate::Codec;

/// A compact, Go-specific binary codec used for internal persistence.
///
/// Mirrors the Go `persist.GobCodec`. Backed by [`bincode`]; output is **not**
/// interchangeable with Go's `encoding/gob`. Use [`JsonCodec`](crate::JsonCodec)
/// for anything that must be portable or byte-compatible with Go.
#[derive(Debug, Clone, Copy, Default)]
pub struct GobCodec;

impl GobCodec {
    /// Creates a new binary codec.
    pub fn new() -> Self {
        Self
    }

    /// Encodes `value` to its binary representation (inherent form of
    /// [`Codec::encode`]).
    pub fn encode<T: Serialize>(&self, value: &T) -> Result<Vec<u8>> {
        bincode::serialize(value).map_err(Error::GobEncode)
    }

    /// Decodes binary `data` into a value of type `T` (inherent form of
    /// [`Codec::decode`]).
    pub fn decode<T: DeserializeOwned>(&self, data: &[u8]) -> Result<T> {
        bincode::deserialize(data).map_err(Error::GobDecode)
    }
}

impl Codec for GobCodec {
    fn encode<T: Serialize>(&self, value: &T) -> Result<Vec<u8>> {
        GobCodec::encode(self, value)
    }

    fn decode<T: DeserializeOwned>(&self, data: &[u8]) -> Result<T> {
        GobCodec::decode(self, data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    struct Sample {
        name: String,
        value: i64,
    }

    // Ported from Go TestGobCodecEncodeDecode.
    #[test]
    fn gob_codec_encode_decode_round_trips() {
        let codec = GobCodec::new();
        let original = Sample {
            name: "test".into(),
            value: 42,
        };

        let data = codec.encode(&original).expect("encode");
        let decoded: Sample = codec.decode(&data).expect("decode");

        assert_eq!(decoded, original);
    }

    // Ported in spirit from Go TestGobCodecEncodeError. The Go test encodes a
    // channel (unencodable). The closest Rust analogue is decoding into a type
    // the bytes cannot satisfy, which must surface a GobDecode error.
    #[test]
    fn gob_codec_decode_error_is_reported() {
        let codec = GobCodec::new();
        let err = codec.decode::<Sample>(b"\x00\x01\x02").unwrap_err();
        assert!(matches!(err, Error::GobDecode(_)));
        assert!(err.to_string().starts_with("persist: gob decode:"));
    }

    #[test]
    fn dyn_codec_object_safe_usage() {
        // Ensure both codecs satisfy the Codec trait and can be used uniformly.
        fn round_trip<C: Codec>(c: &C) {
            let v = Sample {
                name: "x".into(),
                value: 7,
            };
            let bytes = c.encode(&v).unwrap();
            let back: Sample = c.decode(&bytes).unwrap();
            assert_eq!(v, back);
        }
        round_trip(&GobCodec::new());
        round_trip(&crate::JsonCodec::compact());
    }
}
