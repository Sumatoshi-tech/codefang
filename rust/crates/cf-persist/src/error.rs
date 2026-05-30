//! Error type for the persistence layer.
//!
//! The Go package wraps every failure with a stable `fmt.Errorf("<op>: %w", err)`
//! prefix (for example `"json encode"`, `"open state file"`). The Go unit tests
//! assert on those substrings, so [`PersistError`]'s `Display` output reproduces
//! the exact same prefixes to keep behavioral parity.

use std::fmt;

/// An error produced while encoding, decoding, or moving state to/from disk.
///
/// Each variant's [`fmt::Display`] output begins with the same operation prefix
/// the Go `pkg/persist` package emits, so callers and tests that match on
/// substrings (`"json encode"`, `"gob decode"`, `"open state file"`, …) behave
/// identically.
#[derive(Debug)]
pub enum PersistError {
    /// JSON encoding failed. Go: `fmt.Errorf("json encode: %w", err)`.
    JsonEncode(serde_json::Error),
    /// JSON decoding failed. Go: `fmt.Errorf("json decode: %w", err)`.
    JsonDecode(serde_json::Error),
    /// Binary (gob-replacement) encoding failed. Go: `"gob encode: %w"`.
    GobEncode(Box<bincode::ErrorKind>),
    /// Binary (gob-replacement) decoding failed. Go: `"gob decode: %w"`.
    GobDecode(Box<bincode::ErrorKind>),
    /// Creating the on-disk state file failed. Go: `"create state file: %w"`.
    CreateStateFile(std::io::Error),
    /// Opening the on-disk state file failed. Go: `"open state file: %w"`.
    OpenStateFile(std::io::Error),
    /// Writing encoded state to disk failed. Go: `"encode state: %w"`.
    EncodeState(Box<PersistError>),
    /// Reading/decoding state from disk failed. Go: `"decode state: %w"`.
    DecodeState(Box<PersistError>),
    /// A plain I/O failure surfaced by a codec while streaming to the writer.
    Io(std::io::Error),
}

impl fmt::Display for PersistError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PersistError::JsonEncode(e) => write!(f, "json encode: {e}"),
            PersistError::JsonDecode(e) => write!(f, "json decode: {e}"),
            PersistError::GobEncode(e) => write!(f, "gob encode: {e}"),
            PersistError::GobDecode(e) => write!(f, "gob decode: {e}"),
            PersistError::CreateStateFile(e) => write!(f, "create state file: {e}"),
            PersistError::OpenStateFile(e) => write!(f, "open state file: {e}"),
            PersistError::EncodeState(e) => write!(f, "encode state: {e}"),
            PersistError::DecodeState(e) => write!(f, "decode state: {e}"),
            PersistError::Io(e) => write!(f, "io: {e}"),
        }
    }
}

impl std::error::Error for PersistError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            PersistError::JsonEncode(e) | PersistError::JsonDecode(e) => Some(e),
            PersistError::GobEncode(e) | PersistError::GobDecode(e) => Some(e.as_ref()),
            PersistError::CreateStateFile(e)
            | PersistError::OpenStateFile(e)
            | PersistError::Io(e) => Some(e),
            PersistError::EncodeState(e) | PersistError::DecodeState(e) => Some(e.as_ref()),
        }
    }
}
