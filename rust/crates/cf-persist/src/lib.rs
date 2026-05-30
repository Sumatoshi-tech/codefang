//! `cf-persist` — codec-based file persistence for arbitrary state types.
//!
//! Rust port of the Go `pkg/persist` package. It provides a pluggable [`Codec`]
//! abstraction, two concrete codecs ([`JsonCodec`] and [`GobCodec`]), directory-
//! scoped [`save_state`] / [`load_state`] helpers, and a typed [`Persister`].
//!
//! # Byte-identity and the dropped gob format
//!
//! Per specs/rust-rewrite/DESIGN.md §2–§3:
//!
//! * The **JSON codec** emits bytes compatible with Go's `encoding/json`
//!   `Encoder` (HTML escaping on, optional indent, trailing newline, map keys
//!   byte-sorted). It is the persistence-layer analogue of the tier-0
//!   `cf-gojson` crate and should delegate to `cf-gojson::Encoder` once that
//!   crate is implemented (see [`json`] for the bridge note).
//! * Go's **`encoding/gob`** is dropped: it is a Go-specific wire format that is
//!   not byte-portable, and persist/checkpoint state is never user-visible report
//!   output. [`GobCodec`] keeps the Go-facing API (name, `.gob` extension, error
//!   prefixes) but encodes with `bincode`.
//!
//! # Example
//!
//! ```
//! use cf_persist::{JsonCodec, Persister};
//! use serde::{Deserialize, Serialize};
//!
//! #[derive(Default, Serialize, Deserialize)]
//! struct MyState {
//!     label: String,
//!     value: i64,
//! }
//!
//! let dir = tempfile::tempdir().unwrap();
//! let persister = Persister::<MyState, _>::new("mystate", JsonCodec::new());
//!
//! persister
//!     .save(dir.path(), || MyState { label: "hello".into(), value: 42 })
//!     .unwrap();
//!
//! let mut restored = MyState::default();
//! persister
//!     .load(dir.path(), |s: MyState| restored = s)
//!     .unwrap();
//! assert_eq!(restored.value, 42);
//! ```
#![forbid(unsafe_code)]

pub mod error;
pub mod gob;
pub mod json;

use std::fs::File;
use std::io::{Read, Write};
use std::marker::PhantomData;
use std::path::Path;

use serde::Serialize;
use serde::de::DeserializeOwned;

pub use error::PersistError;
pub use gob::{GobCodec, GOB_EXTENSION};
pub use json::{JsonCodec, DEFAULT_INDENT, JSON_EXTENSION};

/// Defines how state is serialized and deserialized.
///
/// Mirrors Go's `persist.Codec` interface. Because Go uses reflection over `any`
/// while Rust resolves the concrete state type at the call site, [`encode`] and
/// [`decode`] are generic over the serde-(de)serializable state type rather than
/// taking a `&dyn Any`. The trait is therefore not object-safe; pass a concrete
/// codec to [`Persister`], [`save_state`], and [`load_state`].
///
/// [`encode`]: Codec::encode
/// [`decode`]: Codec::decode
pub trait Codec {
    /// Serializes `state` to the writer.
    ///
    /// # Errors
    ///
    /// Returns a [`PersistError`] if the value cannot be represented in the
    /// codec's format or if writing to `w` fails.
    fn encode<W: Write, T: Serialize>(&self, w: W, state: &T) -> Result<(), PersistError>;

    /// Reads state from the reader into `state`.
    ///
    /// # Errors
    ///
    /// Returns a [`PersistError`] if the reader does not contain a valid encoding
    /// of `T`.
    fn decode<R: Read, T: DeserializeOwned>(&self, r: R, state: &mut T)
        -> Result<(), PersistError>;

    /// Returns the file extension for this codec (for example `.json`, `.gob`).
    fn extension(&self) -> &'static str;
}

/// Saves `state` to a file in `dir`.
///
/// The filename is `basename` + the codec's [`extension`](Codec::extension), and
/// the file is created (truncating any existing file). Equivalent to Go's
/// `persist.SaveState`.
///
/// # Errors
///
/// * [`PersistError::CreateStateFile`] if the file cannot be created.
/// * [`PersistError::EncodeState`] if the codec fails to encode `state`.
pub fn save_state<C, T>(
    dir: impl AsRef<Path>,
    basename: &str,
    codec: &C,
    state: &T,
) -> Result<(), PersistError>
where
    C: Codec,
    T: Serialize,
{
    let path = dir.as_ref().join(format!("{basename}{}", codec.extension()));
    let file = File::create(&path).map_err(PersistError::CreateStateFile)?;
    codec
        .encode(file, state)
        .map_err(|e| PersistError::EncodeState(Box::new(e)))
}

/// Loads state from a file in `dir` into `state`.
///
/// The filename is `basename` + the codec's [`extension`](Codec::extension).
/// Equivalent to Go's `persist.LoadState`.
///
/// # Errors
///
/// * [`PersistError::OpenStateFile`] if the file cannot be opened.
/// * [`PersistError::DecodeState`] if the codec fails to decode the contents.
pub fn load_state<C, T>(
    dir: impl AsRef<Path>,
    basename: &str,
    codec: &C,
    state: &mut T,
) -> Result<(), PersistError>
where
    C: Codec,
    T: DeserializeOwned,
{
    let path = dir.as_ref().join(format!("{basename}{}", codec.extension()));
    let file = File::open(&path).map_err(PersistError::OpenStateFile)?;
    codec
        .decode(file, state)
        .map_err(|e| PersistError::DecodeState(Box::new(e)))
}

/// Handles I/O for a specific state type `T` using a [`Codec`] `C`.
///
/// Equivalent to Go's generic `persist.Persister[T]`. The codec is owned by the
/// persister; `T` is captured as a type parameter so [`save`](Persister::save)
/// and [`load`](Persister::load) read like the Go closures-based API.
#[derive(Debug, Clone)]
pub struct Persister<T, C: Codec> {
    basename: String,
    codec: C,
    _state: PhantomData<fn() -> T>,
}

impl<T, C: Codec> Persister<T, C> {
    /// Creates a persister with the given `basename` and `codec`.
    ///
    /// Equivalent to Go's `NewPersister[T](basename, codec)`.
    pub fn new(basename: impl Into<String>, codec: C) -> Self {
        Persister {
            basename: basename.into(),
            codec,
            _state: PhantomData,
        }
    }

    /// Writes state to `dir` using the value produced by `build_state`.
    ///
    /// Equivalent to Go's `Persister.Save(dir, buildState)`.
    ///
    /// # Errors
    ///
    /// Propagates any error from [`save_state`].
    pub fn save<F>(&self, dir: impl AsRef<Path>, build_state: F) -> Result<(), PersistError>
    where
        F: FnOnce() -> T,
        T: Serialize,
    {
        let state = build_state();
        save_state(dir, &self.basename, &self.codec, &state)
    }

    /// Restores state from `dir` and hands it to `restore_state`.
    ///
    /// Equivalent to Go's `Persister.Load(dir, restoreState)`.
    ///
    /// # Errors
    ///
    /// Propagates any error from [`load_state`].
    pub fn load<F>(&self, dir: impl AsRef<Path>, restore_state: F) -> Result<(), PersistError>
    where
        F: FnOnce(T),
        T: DeserializeOwned + Default,
    {
        let mut state = T::default();
        load_state(dir, &self.basename, &self.codec, &mut state)?;
        restore_state(state);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};
    use std::collections::BTreeMap;

    #[derive(Default, Serialize, Deserialize, PartialEq, Debug, Clone)]
    struct TestState {
        name: String,
        count: i64,
        values: BTreeMap<String, i64>,
    }

    fn sample() -> TestState {
        let mut values = BTreeMap::new();
        values.insert("k".to_string(), 5);
        TestState {
            name: "load-test".to_string(),
            count: 77,
            values,
        }
    }

    // ---- save_state / load_state (ports of the Go SaveState/LoadState tests) -

    #[test]
    fn save_state_json_writes_file() {
        let dir = tempfile::tempdir().unwrap();
        let codec = JsonCodec::new();
        let state = TestState {
            name: "save-test".to_string(),
            count: 99,
            ..Default::default()
        };
        save_state(dir.path(), "test_state", &codec, &state).unwrap();
        assert!(dir.path().join("test_state.json").exists());
    }

    #[test]
    fn load_state_json_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let codec = JsonCodec::new();
        let original = sample();
        save_state(dir.path(), "test_state", &codec, &original).unwrap();

        let mut loaded = TestState::default();
        load_state(dir.path(), "test_state", &codec, &mut loaded).unwrap();
        assert_eq!(original, loaded);
    }

    #[test]
    fn save_state_gob_writes_file() {
        let dir = tempfile::tempdir().unwrap();
        let codec = GobCodec::new();
        let state = TestState {
            name: "gob-save".to_string(),
            count: 88,
            ..Default::default()
        };
        save_state(dir.path(), "gob_state", &codec, &state).unwrap();
        assert!(dir.path().join("gob_state.gob").exists());
    }

    #[test]
    fn load_state_gob_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let codec = GobCodec::new();
        let original = TestState {
            name: "gob-load".to_string(),
            count: 66,
            ..Default::default()
        };
        save_state(dir.path(), "gob_state", &codec, &original).unwrap();

        let mut loaded = TestState::default();
        load_state(dir.path(), "gob_state", &codec, &mut loaded).unwrap();
        assert_eq!(original.name, loaded.name);
        assert_eq!(original.count, loaded.count);
    }

    #[test]
    fn load_state_file_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let codec = JsonCodec::new();
        let mut state = TestState::default();
        let err = load_state(dir.path(), "nonexistent", &codec, &mut state).unwrap_err();
        assert!(err.to_string().contains("open"));
    }

    #[test]
    fn save_state_invalid_directory() {
        let codec = JsonCodec::new();
        let state = TestState {
            name: "test".to_string(),
            ..Default::default()
        };
        let err = save_state(
            "/nonexistent/path/that/does/not/exist",
            "test",
            &codec,
            &state,
        )
        .unwrap_err();
        assert!(err.to_string().contains("create"));
    }

    #[test]
    fn load_state_decode_error() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("corrupt.json"), b"not json{{{").unwrap();
        let codec = JsonCodec::new();
        let mut state = TestState::default();
        let err = load_state(dir.path(), "corrupt", &codec, &mut state).unwrap_err();
        assert!(err.to_string().contains("decode"));
    }

    // ---- Persister (ports of persister_test.go) ------------------------------

    #[test]
    fn persister_save_load_json() {
        let dir = tempfile::tempdir().unwrap();
        let p = Persister::<TestState, _>::new("mystate", JsonCodec::new());
        let original = TestState {
            name: "hello".to_string(),
            count: 42,
            ..Default::default()
        };

        p.save(dir.path(), || original.clone()).unwrap();

        let mut restored = TestState::default();
        p.load(dir.path(), |s| restored = s).unwrap();
        assert_eq!(original, restored);
    }

    #[test]
    fn persister_save_load_gob() {
        let dir = tempfile::tempdir().unwrap();
        let p = Persister::<TestState, _>::new("gobstate", GobCodec::new());
        let original = TestState {
            name: "gob".to_string(),
            count: 99,
            ..Default::default()
        };

        p.save(dir.path(), || original.clone()).unwrap();

        let mut restored = TestState::default();
        p.load(dir.path(), |s| restored = s).unwrap();
        assert_eq!(original.name, restored.name);
        assert_eq!(original.count, restored.count);
    }

    #[test]
    fn persister_load_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let p = Persister::<TestState, _>::new("missing", JsonCodec::new());
        let err = p.load(dir.path(), |_s| {}).unwrap_err();
        assert!(err.to_string().contains("open"));
    }

    #[test]
    fn persister_save_invalid_dir() {
        let p = Persister::<TestState, _>::new("state", JsonCodec::new());
        let err = p
            .save("/nonexistent/path", || TestState {
                name: "x".to_string(),
                ..Default::default()
            })
            .unwrap_err();
        assert!(err.to_string().contains("create"));
    }
}
