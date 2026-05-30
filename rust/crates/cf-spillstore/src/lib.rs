//! cf-spillstore — port of the Go package
//! `internal/analyzers/common/spillstore`.
//!
//! Provides a generic, disk-backed key/value store ([`SpillStore`]) that spills
//! accumulated data to temporary files during streaming hibernation, freeing
//! memory between chunks while preserving the full dataset for finalization.
//!
//! During normal (non-streaming) execution the store behaves as a plain map.
//! When [`SpillStore::spill`] is called (typically from a hibernate hook), the
//! current in-memory buffer is written to a numbered chunk file and the map is
//! cleared. [`SpillStore::collect`] merges every spilled chunk and the current
//! buffer back into a single map.
//!
//! # Behavioral parity with the Go original
//!
//! Every observable behavior of the Go `SpillStore[V]` is reproduced exactly:
//!
//! - The temp directory is created lazily on the first non-empty spill, under
//!   `base_dir` when set (otherwise the system temp dir), with the name prefix
//!   `codefang-spill-`.
//! - Chunk files are named `chunk_NNN` (zero-padded to width 3: `chunk_000`,
//!   `chunk_001`, …) in spill order, and read back in the same ascending order
//!   so that later chunks overwrite earlier ones (last-write-wins) for
//!   duplicate keys.
//! - [`SpillStore::spill`] is a no-op when the buffer is empty.
//! - [`SpillStore::collect`] / [`SpillStore::collect_with`] clean up the temp
//!   directory and reset the store afterward.
//! - [`SpillStore::restore_from_dir`] re-points the store at an existing spill
//!   directory for checkpoint restoration.
//! - [`SpillStore::for_each_spill`] iterates spilled chunks without cleaning up.
//!
//! # Byte-identity note (DESIGN.md §3)
//!
//! The spilled chunk files are intermediate, never-user-visible state. The Go
//! original encodes them with `encoding/gob`, which is dropped in the Rust tree
//! because gob is not byte-portable. This crate uses a Rust-native `serde`
//! codec ([`serde_json`]) for the chunk files. None of these bytes appear in a
//! MACHINE-format report, so the shared `cf-gojson` encoder is intentionally
//! not used here. The chunk file format is an internal implementation detail
//! and carries no cross-language byte-identity guarantee.
//!
//! Ported from Go. See `specs/rust-rewrite/DESIGN.md`.

#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::Serialize;

/// Filename prefix for the lazily-created spill temp directory.
///
/// Mirrors the Go pattern `"codefang-spill-*"` passed to `os.MkdirTemp`.
const SPILL_DIR_PREFIX: &str = "codefang-spill-";

/// Builds the chunk filename for a given spill index.
///
/// Mirrors the Go `fmt.Sprintf("chunk_%03d.gob", index)` zero-padded numbering.
/// The Rust codec is JSON rather than gob, so the extension is `.json`; the file
/// is purely internal state and never user-visible (see the crate-level
/// byte-identity note).
fn chunk_file_name(index: usize) -> String {
    format!("chunk_{index:03}.json")
}

/// Errors returned by [`SpillStore`] operations.
///
/// The variants mirror the failure points of the Go implementation
/// (`create temp dir`, `create/encode/close spill`, `open/decode spill`) so
/// callers can distinguish where a spill round-trip failed.
#[derive(Debug)]
pub enum SpillError {
    /// Failed to create the lazily-allocated temp directory.
    CreateTempDir(io::Error),
    /// Failed to create a spill chunk file. Carries the zero-based spill index.
    CreateSpill(usize, io::Error),
    /// Failed to encode (serialize + write) a spill chunk.
    EncodeSpill(usize, io::Error),
    /// Failed to open a spill chunk for reading.
    OpenSpill(usize, io::Error),
    /// Failed to decode (read + deserialize) a spill chunk.
    DecodeSpill(usize, io::Error),
}

impl fmt::Display for SpillError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CreateTempDir(e) => write!(f, "spillstore: create temp dir: {e}"),
            Self::CreateSpill(i, e) => write!(f, "spillstore: create spill file: {e} (chunk {i})"),
            Self::EncodeSpill(i, e) => write!(f, "spillstore: encode spill {i}: {e}"),
            Self::OpenSpill(i, e) => write!(f, "spillstore: open spill {i}: {e}"),
            Self::DecodeSpill(i, e) => write!(f, "spillstore: decode spill {i}: {e}"),
        }
    }
}

impl std::error::Error for SpillError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::CreateTempDir(e)
            | Self::CreateSpill(_, e)
            | Self::EncodeSpill(_, e)
            | Self::OpenSpill(_, e)
            | Self::DecodeSpill(_, e) => Some(e),
        }
    }
}

/// A `HashMap<String, V>` with transparent disk spilling.
///
/// See the crate-level documentation for the full behavioral contract. `V` must
/// be `serde`-serializable so chunks can be written to and read back from disk;
/// this is the Rust analogue of the Go original's reliance on `encoding/gob`
/// being able to encode any registered value type.
///
/// # Examples
///
/// ```
/// use cf_spillstore::SpillStore;
///
/// let mut store: SpillStore<i64> = SpillStore::new("");
/// store.put("a".to_string(), 1);
/// store.put("b".to_string(), 2);
///
/// // Spill the buffer to disk, then add more in memory.
/// store.spill().unwrap();
/// store.put("c".to_string(), 3);
///
/// // collect() merges spilled chunks with the in-memory buffer.
/// let merged = store.collect().unwrap();
/// assert_eq!(merged.get("a"), Some(&1));
/// assert_eq!(merged.get("c"), Some(&3));
/// ```
#[derive(Debug)]
pub struct SpillStore<V> {
    /// The current in-memory buffer.
    current: HashMap<String, V>,
    /// Temp directory; created lazily on first spill. `None` means no spills yet.
    dir: Option<PathBuf>,
    /// Parent directory for temp dirs. Empty means the system default temp dir.
    base_dir: String,
    /// Number of spill files written.
    spill_n: usize,
}

impl<V> SpillStore<V> {
    /// Creates a `SpillStore` with an empty in-memory buffer.
    ///
    /// When `base_dir` is non-empty, temp dirs for spill files are created under
    /// it; otherwise the system default temp dir is used. Mirrors Go `New`.
    pub fn new(base_dir: impl Into<String>) -> Self {
        Self {
            current: HashMap::new(),
            dir: None,
            base_dir: base_dir.into(),
            spill_n: 0,
        }
    }

    /// Stores a key/value pair in the current in-memory buffer. Mirrors Go `Put`.
    pub fn put(&mut self, key: String, val: V) {
        self.current.insert(key, val);
    }

    /// Returns a reference to a value from the current in-memory buffer.
    ///
    /// Does **not** read from spilled files (parity with Go `Get`). Returns
    /// `None` when the key is absent from the live buffer.
    pub fn get(&self, key: &str) -> Option<&V> {
        self.current.get(key)
    }

    /// Returns the number of entries in the current in-memory buffer.
    ///
    /// Mirrors Go `Len`. (The Go nil-receiver-returns-0 convention is expressed
    /// in Rust by `Option<SpillStore<V>>`; an absent store contributes 0.)
    pub fn len(&self) -> usize {
        self.current.len()
    }

    /// Reports whether the in-memory buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.current.is_empty()
    }

    /// Returns the current in-memory buffer. The caller must not mutate it
    /// through this borrow in a way that violates store invariants.
    ///
    /// Mirrors Go `Current`.
    pub fn current(&self) -> &HashMap<String, V> {
        &self.current
    }

    /// Returns the number of spill files written. Mirrors Go `SpillCount`.
    pub fn spill_count(&self) -> usize {
        self.spill_n
    }

    /// Returns the temp directory path, or `None` if no spills have occurred.
    ///
    /// Mirrors Go `SpillDir` (which returns "" when empty).
    pub fn spill_dir(&self) -> Option<&Path> {
        self.dir.as_deref()
    }

    /// Points the store at an existing spill directory with the given number of
    /// spill files. Used for checkpoint restoration. Mirrors Go `RestoreFromDir`.
    pub fn restore_from_dir(&mut self, dir: impl Into<PathBuf>, count: usize) {
        self.dir = Some(dir.into());
        self.spill_n = count;
        // `current` is always initialized in Rust; nothing to lazily allocate.
    }

    /// Removes the temp directory. Safe to call multiple times. Mirrors Go
    /// `Cleanup`.
    ///
    /// Removal errors are ignored, matching Go's `os.RemoveAll` whose error is
    /// discarded by the original `Cleanup`.
    pub fn cleanup(&mut self) {
        if let Some(dir) = self.dir.take() {
            let _ = fs::remove_dir_all(&dir);
        }
    }

    /// Lazily creates the temp directory if it does not yet exist, returning its
    /// path. Mirrors the lazy-`os.MkdirTemp` block at the top of Go `Spill`.
    fn ensure_dir(&mut self) -> Result<&Path, SpillError> {
        if self.dir.is_none() {
            let mut builder = tempfile::Builder::new();
            builder.prefix(SPILL_DIR_PREFIX);

            let created = if self.base_dir.is_empty() {
                builder.tempdir()
            } else {
                builder.tempdir_in(&self.base_dir)
            }
            .map_err(SpillError::CreateTempDir)?;

            // Persist the directory: the Go store owns the lifecycle explicitly
            // via Cleanup/Collect, not via RAII. keep() disables tempfile's
            // automatic deletion so the directory survives until we remove it.
            self.dir = Some(created.keep());
        }

        // Safe: just ensured Some above.
        Ok(self.dir.as_deref().expect("dir set"))
    }
}

impl<V> SpillStore<V>
where
    V: Serialize + DeserializeOwned,
{
    /// Writes the current buffer to a numbered chunk file and clears the map.
    ///
    /// No-op when the buffer is empty. The temp directory is created lazily on
    /// the first non-empty spill. Mirrors Go `Spill`.
    pub fn spill(&mut self) -> Result<(), SpillError> {
        if self.current.is_empty() {
            return Ok(());
        }

        let index = self.spill_n;
        let path = {
            let dir = self.ensure_dir()?;
            dir.join(chunk_file_name(index))
        };

        let file = fs::File::create(&path).map_err(|e| SpillError::CreateSpill(index, e))?;
        serde_json::to_writer(&file, &self.current)
            .map_err(|e| SpillError::EncodeSpill(index, io::Error::from(e)))?;
        // Flush/close: dropping the File closes it; the Go original likewise
        // relies on Close() rather than an explicit fsync.
        drop(file);

        self.spill_n += 1;
        self.current = HashMap::new();

        Ok(())
    }

    /// Returns all data (spilled + in-memory) merged into one map, then cleans
    /// up spill files and resets the store.
    ///
    /// Later entries overwrite earlier ones for the same key. Mirrors Go
    /// `Collect` (which delegates to `CollectWith(nil)`).
    pub fn collect(&mut self) -> Result<HashMap<String, V>, SpillError> {
        self.collect_with::<fn(V, V) -> V>(None)
    }

    /// Merges spilled chunks using an optional merge function, then cleans up
    /// spill files and resets the store.
    ///
    /// When `merge` is `None`, later values overwrite earlier ones for duplicate
    /// keys (last-write-wins). When `merge` is `Some(f)`, it is called as
    /// `f(existing, incoming)` for conflicting keys. Mirrors Go `CollectWith`.
    pub fn collect_with<F>(&mut self, merge: Option<F>) -> Result<HashMap<String, V>, SpillError>
    where
        F: Fn(V, V) -> V,
    {
        let mut result: HashMap<String, V> = HashMap::new();

        for i in 0..self.spill_n {
            let chunk = self.read_spill_file(i)?;
            merge_into(&mut result, chunk, merge.as_ref());
        }

        // Drain the live buffer into the result.
        let current = std::mem::take(&mut self.current);
        merge_into(&mut result, current, merge.as_ref());

        self.cleanup();
        self.current = HashMap::new();
        self.spill_n = 0;

        Ok(result)
    }

    /// Iterates through all spill files, calling `f` for each decoded chunk.
    ///
    /// Does not clean up spill files or modify the current buffer. Mirrors Go
    /// `ForEachSpill`.
    pub fn for_each_spill<F>(&self, mut f: F) -> Result<(), SpillError>
    where
        F: FnMut(HashMap<String, V>) -> Result<(), SpillError>,
    {
        for i in 0..self.spill_n {
            let chunk = self.read_spill_file(i)?;
            f(chunk)?;
        }

        Ok(())
    }

    /// Reads and decodes a single spill chunk by index. Mirrors Go
    /// `readSpillFile`.
    fn read_spill_file(&self, index: usize) -> Result<HashMap<String, V>, SpillError> {
        let dir = self
            .dir
            .as_deref()
            .ok_or_else(|| SpillError::OpenSpill(index, io::Error::from(io::ErrorKind::NotFound)))?;
        let path = dir.join(chunk_file_name(index));

        let file = fs::File::open(&path).map_err(|e| SpillError::OpenSpill(index, e))?;
        let reader = io::BufReader::new(file);
        let chunk: HashMap<String, V> = serde_json::from_reader(reader)
            .map_err(|e| SpillError::DecodeSpill(index, io::Error::from(e)))?;

        Ok(chunk)
    }
}

/// Merges `src` into `dst` using an optional conflict-resolution function.
///
/// With `merge == None`, this is last-write-wins (`src` overwrites `dst`),
/// matching Go's `maps.Copy(dst, src)`. With `merge == Some(f)`, conflicting
/// keys are resolved by `f(existing, incoming)`. Mirrors Go `mergeInto`.
fn merge_into<V, F>(dst: &mut HashMap<String, V>, src: HashMap<String, V>, merge: Option<&F>)
where
    F: Fn(V, V) -> V,
{
    match merge {
        None => {
            for (k, v) in src {
                dst.insert(k, v);
            }
        }
        Some(f) => {
            for (k, v) in src {
                if let Some(existing) = dst.remove(&k) {
                    dst.insert(k, f(existing, v));
                } else {
                    dst.insert(k, v);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    /// Helper mirroring the Go tests' `map[string]int{...}` literal comparisons.
    fn map_int(pairs: &[(&str, i64)]) -> HashMap<String, i64> {
        pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect()
    }

    /// Port of Go `TestSpillStore_NoSpill`.
    #[test]
    fn no_spill() {
        let mut s: SpillStore<i64> = SpillStore::new("");
        s.put("a".to_string(), 1);
        s.put("b".to_string(), 2);

        assert_eq!(s.len(), 2);

        assert_eq!(s.get("a"), Some(&1));

        let collected = s.collect().unwrap();
        assert_eq!(collected, map_int(&[("a", 1), ("b", 2)]));
    }

    /// Port of Go `TestSpillStore_SingleSpill`.
    #[test]
    fn single_spill() {
        let mut s: SpillStore<String> = SpillStore::new("");
        s.put("k1".to_string(), "v1".to_string());
        s.put("k2".to_string(), "v2".to_string());

        s.spill().unwrap();
        assert_eq!(s.len(), 0);
        assert_eq!(s.spill_count(), 1);

        s.put("k3".to_string(), "v3".to_string());

        let collected = s.collect().unwrap();
        let want: HashMap<String, String> = [("k1", "v1"), ("k2", "v2"), ("k3", "v3")]
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        assert_eq!(collected, want);
        // Cleaned up.
        assert!(s.spill_dir().is_none());
    }

    /// Port of Go `TestSpillStore_MultipleSpills`.
    #[test]
    fn multiple_spills() {
        let mut s: SpillStore<i64> = SpillStore::new("");

        // Chunk 1.
        s.put("a".to_string(), 1);
        s.put("b".to_string(), 2);
        s.spill().unwrap();

        // Chunk 2.
        s.put("c".to_string(), 3);
        s.put("d".to_string(), 4);
        s.spill().unwrap();

        // Chunk 3 (in-memory).
        s.put("e".to_string(), 5);

        let collected = s.collect().unwrap();
        assert_eq!(
            collected,
            map_int(&[("a", 1), ("b", 2), ("c", 3), ("d", 4), ("e", 5)])
        );
    }

    /// Port of Go `TestSpillStore_SpillEmpty`.
    #[test]
    fn spill_empty() {
        let mut s: SpillStore<i64> = SpillStore::new("");

        s.spill().unwrap(); // No-op.
        assert_eq!(s.spill_count(), 0);
        assert!(s.spill_dir().is_none());
    }

    /// Port of Go `TestSpillStore_CollectWith`.
    #[test]
    fn collect_with() {
        let mut s: SpillStore<HashMap<String, i64>> = SpillStore::new("");

        // Chunk 1: file "a.go" couples with "b.go".
        s.put("a.go".to_string(), map_int(&[("b.go", 3)]));
        s.spill().unwrap();

        // Chunk 2: file "a.go" couples with "b.go" again + "c.go".
        s.put("a.go".to_string(), map_int(&[("b.go", 2), ("c.go", 1)]));

        let merge = |mut existing: HashMap<String, i64>, incoming: HashMap<String, i64>| {
            for (k, v) in incoming {
                *existing.entry(k).or_insert(0) += v;
            }
            existing
        };

        let collected = s.collect_with(Some(merge)).unwrap();
        assert_eq!(
            collected.get("a.go"),
            Some(&map_int(&[("b.go", 5), ("c.go", 1)]))
        );
    }

    /// Port of Go `TestSpillStore_Cleanup`.
    #[test]
    fn cleanup() {
        let mut s: SpillStore<i64> = SpillStore::new("");
        s.put("x".to_string(), 42);
        s.spill().unwrap();

        let dir = s.spill_dir().unwrap().to_path_buf();
        assert!(dir.is_dir());

        s.cleanup();
        assert!(!dir.exists());

        // Double cleanup is safe.
        s.cleanup();
    }

    /// Port of Go `TestSpillStore_RestoreFromDir`.
    #[test]
    fn restore_from_dir() {
        // Write spill files via one store.
        let mut s1: SpillStore<i64> = SpillStore::new("");
        s1.put("a".to_string(), 1);
        s1.spill().unwrap();
        s1.put("b".to_string(), 2);
        s1.spill().unwrap();

        let dir = s1.spill_dir().unwrap().to_path_buf();
        let count = s1.spill_count();

        // Restore into a new store.
        let mut s2: SpillStore<i64> = SpillStore::new("");
        s2.restore_from_dir(dir, count);
        s2.put("c".to_string(), 3);

        let collected = s2.collect().unwrap();
        assert_eq!(collected, map_int(&[("a", 1), ("b", 2), ("c", 3)]));
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct TestStruct {
        name: String,
        value: i64,
    }

    /// Port of Go `TestSpillStore_StructValues`.
    #[test]
    fn struct_values() {
        let mut s: SpillStore<TestStruct> = SpillStore::new("");
        s.put(
            "x".to_string(),
            TestStruct {
                name: "hello".to_string(),
                value: 42,
            },
        );
        s.spill().unwrap();

        s.put(
            "y".to_string(),
            TestStruct {
                name: "world".to_string(),
                value: 99,
            },
        );

        let collected = s.collect().unwrap();
        assert_eq!(
            collected.get("x"),
            Some(&TestStruct {
                name: "hello".to_string(),
                value: 42
            })
        );
        assert_eq!(
            collected.get("y"),
            Some(&TestStruct {
                name: "world".to_string(),
                value: 99
            })
        );
    }

    /// Port of Go `TestSpillStore_PointerValues`. In Rust we model the Go
    /// `*testStruct` value type with `Box<TestStruct>`; round-trip equality is
    /// preserved.
    #[test]
    fn pointer_values() {
        let mut s: SpillStore<Box<TestStruct>> = SpillStore::new("");
        s.put(
            "x".to_string(),
            Box::new(TestStruct {
                name: "hello".to_string(),
                value: 42,
            }),
        );
        s.spill().unwrap();

        s.put(
            "y".to_string(),
            Box::new(TestStruct {
                name: "world".to_string(),
                value: 99,
            }),
        );

        let collected = s.collect().unwrap();
        assert_eq!(
            collected.get("x").map(|b| b.as_ref()),
            Some(&TestStruct {
                name: "hello".to_string(),
                value: 42
            })
        );
        assert_eq!(
            collected.get("y").map(|b| b.as_ref()),
            Some(&TestStruct {
                name: "world".to_string(),
                value: 99
            })
        );
    }

    /// Additional coverage: collect resets spill_n and clears the buffer so the
    /// store is reusable (parity with Go Collect's tail reset).
    #[test]
    fn collect_resets_store() {
        let mut s: SpillStore<i64> = SpillStore::new("");
        s.put("a".to_string(), 1);
        s.spill().unwrap();
        let _ = s.collect().unwrap();

        assert_eq!(s.spill_count(), 0);
        assert_eq!(s.len(), 0);
        assert!(s.spill_dir().is_none());

        // Reusable after collect.
        s.put("b".to_string(), 2);
        let again = s.collect().unwrap();
        assert_eq!(again, map_int(&[("b", 2)]));
    }

    /// Additional coverage: for_each_spill visits chunks in ascending order and
    /// does not clean up the spill directory.
    #[test]
    fn for_each_spill_iterates_without_cleanup() {
        let mut s: SpillStore<i64> = SpillStore::new("");
        s.put("a".to_string(), 1);
        s.spill().unwrap();
        s.put("b".to_string(), 2);
        s.spill().unwrap();

        let mut seen: Vec<i64> = Vec::new();
        s.for_each_spill(|chunk| {
            for (_, v) in chunk {
                seen.push(v);
            }
            Ok(())
        })
        .unwrap();
        seen.sort_unstable();
        assert_eq!(seen, vec![1, 2]);

        // Directory still present (no cleanup).
        assert!(s.spill_dir().unwrap().is_dir());
        s.cleanup();
    }

    /// Additional coverage: last-write-wins ordering across chunks. A key that
    /// appears in an earlier chunk and again later resolves to the later value.
    #[test]
    fn last_write_wins_across_chunks() {
        let mut s: SpillStore<i64> = SpillStore::new("");
        s.put("k".to_string(), 1);
        s.spill().unwrap();
        s.put("k".to_string(), 2);
        s.spill().unwrap();
        s.put("k".to_string(), 3); // in-memory, highest precedence

        let collected = s.collect().unwrap();
        assert_eq!(collected.get("k"), Some(&3));
    }
}
