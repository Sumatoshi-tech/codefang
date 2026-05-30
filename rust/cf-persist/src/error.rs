//! Error types for the persistence codecs.

use std::fmt;

/// Convenience alias for results produced by this crate.
pub type Result<T> = std::result::Result<T, Error>;

/// An error produced while encoding or decoding a value.
///
/// The `Display` representation mirrors the Go implementation's wrapped error
/// messages (e.g. `persist: json encode: ...`), so log output and assertions
/// that match on these strings stay compatible across the port.
#[derive(Debug)]
pub enum Error {
    /// A JSON value failed to encode.
    JsonEncode(serde_json::Error),
    /// JSON data failed to decode.
    JsonDecode(serde_json::Error),
    /// A value failed to encode with the binary (gob-equivalent) codec.
    GobEncode(bincode::Error),
    /// Binary (gob-equivalent) data failed to decode.
    GobDecode(bincode::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::JsonEncode(e) => write!(f, "persist: json encode: {e}"),
            Error::JsonDecode(e) => write!(f, "persist: json decode: {e}"),
            Error::GobEncode(e) => write!(f, "persist: gob encode: {e}"),
            Error::GobDecode(e) => write!(f, "persist: gob decode: {e}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::JsonEncode(e) | Error::JsonDecode(e) => Some(e),
            Error::GobEncode(e) | Error::GobDecode(e) => Some(e.as_ref()),
        }
    }
}
