//! Binary codec — the Rust-native replacement for Go's `encoding/gob`.
//!
//! DESIGN.md §3 drops gob outright: it is a Go-specific wire format that is not
//! byte-portable, and persist/checkpoint state is never user-visible report
//! output. This codec uses [`bincode`] for a compact, deterministic, self-
//! describing-enough binary encoding of serde types.
//!
//! For drop-in parity with the Go API the type keeps the name [`GobCodec`], the
//! `.gob` file extension, and the `gob encode` / `gob decode` error prefixes the
//! Go unit tests assert on. The on-disk *bytes* differ from Go's gob (by design);
//! everything observable through the [`Codec`] contract is preserved.

use std::io::{Read, Write};

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::error::PersistError;
use crate::Codec;

/// File extension for binary state files (`gobExtension` in Go).
pub const GOB_EXTENSION: &str = ".gob";

/// Binary codec backed by `bincode` (the gob replacement).
///
/// Mirrors Go's `persist.GobCodec`: a stateless codec with a `.gob` extension.
/// Construct with [`GobCodec::new`] (`NewGobCodec()` in Go).
#[derive(Debug, Clone, Copy, Default)]
pub struct GobCodec;

impl GobCodec {
    /// Creates a binary codec. Equivalent to Go's `NewGobCodec()`.
    #[must_use]
    pub fn new() -> Self {
        GobCodec
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
