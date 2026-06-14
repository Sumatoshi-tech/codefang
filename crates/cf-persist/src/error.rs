//! Error type for the persistence layer.
//!
//! Every failure carries a stable `<op>: ` prefix (for example `json encode`,
//! `open state file`). Unit tests — here and in the reference suite — assert on
//! those substrings, so [`PersistError`]'s `Display` output keeps the exact
//! prefixes.

/// An error produced while encoding, decoding, or moving state to/from disk.
///
/// Each variant's `Display` output begins with a stable operation prefix
/// (`json encode`, `gob decode`, `open state file`, …) that callers and tests
/// match on.
#[derive(Debug, thiserror::Error)]
pub enum PersistError {
    /// JSON encoding failed.
    #[error("json encode: {0}")]
    JsonEncode(#[source] serde_json::Error),
    /// JSON decoding failed.
    #[error("json decode: {0}")]
    JsonDecode(#[source] serde_json::Error),
    /// Binary (gob-replacement) encoding failed.
    #[error("gob encode: {0}")]
    GobEncode(#[source] Box<bincode::ErrorKind>),
    /// Binary (gob-replacement) decoding failed.
    #[error("gob decode: {0}")]
    GobDecode(#[source] Box<bincode::ErrorKind>),
    /// Creating the on-disk state file failed.
    #[error("create state file: {0}")]
    CreateStateFile(#[source] std::io::Error),
    /// Opening the on-disk state file failed.
    #[error("open state file: {0}")]
    OpenStateFile(#[source] std::io::Error),
    /// Writing encoded state to disk failed.
    #[error("encode state: {0}")]
    EncodeState(#[source] Box<Self>),
    /// Reading/decoding state from disk failed.
    #[error("decode state: {0}")]
    DecodeState(#[source] Box<Self>),
    /// A plain I/O failure surfaced by a codec while streaming to the writer.
    #[error("io: {0}")]
    Io(#[source] std::io::Error),
}
