//! Binary codec for internal state files.
//!
//! Persist/checkpoint state is never user-visible report output, so no
//! cross-implementation wire format is reproduced (DESIGN.md §3). This codec
//! uses [`bincode`] for a compact, deterministic binary encoding of serde
//! types.
//!
//! The historical name [`GobCodec`], the `.gob` file extension, and the
//! `gob encode` / `gob decode` error prefixes are kept stable: tests and
//! on-disk layouts depend on them.

use std::io::{Read, Write};

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::error::PersistError;
use crate::Codec;

/// File extension for binary state files.
pub const GOB_EXTENSION: &str = ".gob";

/// Stateless binary codec backed by `bincode`, with a `.gob` extension.
///
/// Construct with [`GobCodec::new`].
#[derive(Debug, Clone, Copy, Default)]
pub struct GobCodec;

impl GobCodec {
    /// Creates a binary codec.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Codec for GobCodec {
    fn encode<W: Write, T: Serialize>(&self, mut w: W, state: &T) -> Result<(), PersistError> {
        let bytes = bincode::serialize(state).map_err(PersistError::GobEncode)?;
        w.write_all(&bytes).map_err(PersistError::Io)
    }

    fn decode<R: Read, T: DeserializeOwned>(
        &self,
        r: R,
        state: &mut T,
    ) -> Result<(), PersistError> {
        let decoded: T = bincode::deserialize_from(r).map_err(PersistError::GobDecode)?;
        *state = decoded;
        Ok(())
    }

    fn extension(&self) -> &'static str {
        GOB_EXTENSION
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(serde::Serialize, serde::Deserialize, PartialEq, Debug)]
    struct State {
        name: String,
        count: i64,
        values: std::collections::BTreeMap<String, i64>,
    }

    #[test]
    fn round_trip() {
        let mut values = std::collections::BTreeMap::new();
        values.insert("x".to_string(), 10);
        values.insert("y".to_string(), 20);
        let original = State {
            name: "gob-test".to_string(),
            count: 123,
            values,
        };

        let codec = GobCodec::new();
        let mut buf = Vec::new();
        codec.encode(&mut buf, &original).unwrap();

        let mut decoded = State {
            name: String::new(),
            count: 0,
            values: std::collections::BTreeMap::new(),
        };
        codec.decode(buf.as_slice(), &mut decoded).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn extension_is_gob() {
        assert_eq!(GobCodec::new().extension(), ".gob");
    }

    #[test]
    fn decode_error_is_reported() {
        let codec = GobCodec::new();
        let mut decoded = State {
            name: String::new(),
            count: 0,
            values: std::collections::BTreeMap::new(),
        };
        // Truncated / non-bincode input must fail with a "gob decode" prefix.
        let err = codec
            .decode(b"\xff\xff\xff".as_slice(), &mut decoded)
            .unwrap_err();
        assert!(err.to_string().contains("gob decode"));
    }
}
