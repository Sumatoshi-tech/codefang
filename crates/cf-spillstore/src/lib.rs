//! cf-spillstore — disk-backed spill storage for streaming analyzers.
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
//! # Behavioral contract
//!
//! The store's observable behavior is part of the compatibility contract:
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
//! The spilled chunk files are intermediate, never-user-visible state, encoded
//! with a Rust-native `serde` codec ([`serde_json`]). None of these bytes
//! appear in a MACHINE-format report, so the shared `cf-gojson` encoder is
//! intentionally not used here. The chunk file format is an internal
//! implementation detail and carries no cross-version byte-identity guarantee.

#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::Serialize;

/// Filename prefix for the lazily-created spill temp directory.
const SPILL_DIR_PREFIX: &str = "codefang-spill-";

/// Builds the zero-padded chunk filename for a given spill index
/// (`chunk_000.json`, `chunk_001.json`, ...). The file is purely internal
/// state and never user-visible (see the crate-level byte-identity note).
fn chunk_file_name(index: usize) -> String {
    format!("chunk_{index:03}.json")
}

/// Errors returned by [`SpillStore`] operations.
///
/// The variants name the failure points of a spill round-trip (`create temp
/// dir`, `create/encode spill`, `open/decode spill`) so callers can
/// distinguish where it failed.
#[derive(Debug, thiserror::Error)]
pub enum SpillError {
    /// Failed to create the lazily-allocated temp directory.
    #[error("spillstore: create temp dir: {0}")]
    CreateTempDir(#[source] io::Error),
    /// Failed to create a spill chunk file. Carries the zero-based spill index.
    #[error("spillstore: create spill file: {1} (chunk {0})")]
    CreateSpill(usize, #[source] io::Error),
    /// Failed to encode (serialize + write) a spill chunk.
    #[error("spillstore: encode spill {0}: {1}")]
    EncodeSpill(usize, #[source] io::Error),
    /// Failed to open a spill chunk for reading.
    #[error("spillstore: open spill {0}: {1}")]
    OpenSpill(usize, #[source] io::Error),
    /// Failed to decode (read + deserialize) a spill chunk.
    #[error("spillstore: decode spill {0}: {1}")]
    DecodeSpill(usize, #[source] io::Error),
}

/// A `HashMap<String, V>` with transparent disk spilling.
///
/// See the crate-level documentation for the full behavioral contract. `V` must
/// be `serde`-serializable so chunks can be written to and read back from
/// disk.
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
    /// it; otherwise the system default temp dir is used.
    pub fn new(base_dir: impl Into<String>) -> Self {
        Self {
            current: HashMap::new(),
            dir: None,
            base_dir: base_dir.into(),
            spill_n: 0,
        }
    }

    /// Stores a key/value pair in the current in-memory buffer.
    pub fn put(&mut self, key: String, val: V) {
        self.current.insert(key, val);
    }

    /// Returns a reference to a value from the current in-memory buffer.
    ///
    /// Does **not** read from spilled files. Returns `None` when the key is
    /// absent from the live buffer.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&V> {
        self.current.get(key)
    }

    /// Returns the number of entries in the current in-memory buffer.
    #[must_use]
    pub fn len(&self) -> usize {
        self.current.len()
    }

    /// Reports whether the in-memory buffer is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.current.is_empty()
    }

    /// Returns the current in-memory buffer.
    #[must_use]
    pub const fn current(&self) -> &HashMap<String, V> {
        &self.current
    }

    /// Returns the number of spill files written.
    #[must_use]
    pub const fn spill_count(&self) -> usize {
        self.spill_n
    }

    /// Returns the temp directory path, or `None` if no spills have occurred.
    #[must_use]
    pub fn spill_dir(&self) -> Option<&Path> {
        self.dir.as_deref()
    }

    /// Points the store at an existing spill directory with the given number of
    /// spill files. Used for checkpoint restoration.
    pub fn restore_from_dir(&mut self, dir: impl Into<PathBuf>, count: usize) {
        self.dir = Some(dir.into());
        self.spill_n = count;
    }

    /// Removes the temp directory. Safe to call multiple times.
    ///
    /// Removal errors are deliberately ignored (best-effort cleanup).
    pub fn cleanup(&mut self) {
        if let Some(dir) = self.dir.take() {
            let _ = fs::remove_dir_all(&dir);
        }
    }

    /// Lazily creates the temp directory if it does not yet exist, returning its
    /// path.
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

            // Persist the directory: the store owns the lifecycle explicitly
            // via cleanup()/collect(), not via RAII. keep() disables tempfile's
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
    /// the first non-empty spill.
    ///
    /// # Errors
    ///
    /// Returns a [`SpillError`] if the temp directory or chunk file cannot be
    /// created, or if encoding the buffer fails.
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
        // Flush/close: dropping the File closes it; spill files are not
        // durability-critical, so no explicit fsync.
        drop(file);

        self.spill_n += 1;
        self.current = HashMap::new();

        Ok(())
    }

    /// Returns all data (spilled + in-memory) merged into one map, then cleans
    /// up spill files and resets the store.
    ///
    /// Later entries overwrite earlier ones for the same key.
    ///
    /// # Errors
    ///
    /// Returns a [`SpillError`] if a spilled chunk cannot be read back.
    pub fn collect(&mut self) -> Result<HashMap<String, V>, SpillError> {
        self.collect_with::<fn(V, V) -> V>(None)
    }

    /// Merges spilled chunks using an optional merge function, then cleans up
    /// spill files and resets the store.
    ///
    /// When `merge` is `None`, later values overwrite earlier ones for duplicate
    /// keys (last-write-wins). When `merge` is `Some(f)`, it is called as
    /// `f(existing, incoming)` for conflicting keys.
    ///
    /// # Errors
    ///
    /// Returns a [`SpillError`] if a spilled chunk cannot be read back.
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
    /// Does not clean up spill files or modify the current buffer.
    ///
    /// # Errors
    ///
    /// Returns a [`SpillError`] if a chunk cannot be read, or propagates the
    /// first error returned by `f`.
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

    /// Reads and decodes a single spill chunk by index.
    fn read_spill_file(&self, index: usize) -> Result<HashMap<String, V>, SpillError> {
        let dir = self.dir.as_deref().ok_or_else(|| {
            SpillError::OpenSpill(index, io::Error::from(io::ErrorKind::NotFound))
        })?;
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
/// With `merge == None`, this is last-write-wins (`src` overwrites `dst`).
/// With `merge == Some(f)`, conflicting keys are resolved by
/// `f(existing, incoming)`.
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

    /// Helper building a `HashMap<String, i64>` from literal pairs.
    fn map_int(pairs: &[(&str, i64)]) -> HashMap<String, i64> {
        pairs.iter().map(|&(k, v)| (k.to_string(), v)).collect()
    }

    /// Mirrors the reference test `TestSpillStore_NoSpill`.
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

    /// Mirrors the reference test `TestSpillStore_SingleSpill`.
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
            .map(|&(k, v)| (k.to_string(), v.to_string()))
            .collect();
        assert_eq!(collected, want);
        // Cleaned up.
        assert!(s.spill_dir().is_none());
    }

    /// Mirrors the reference test `TestSpillStore_MultipleSpills`.
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

    /// Mirrors the reference test `TestSpillStore_SpillEmpty`.
    #[test]
    fn spill_empty() {
        let mut s: SpillStore<i64> = SpillStore::new("");

        s.spill().unwrap(); // No-op.
        assert_eq!(s.spill_count(), 0);
        assert!(s.spill_dir().is_none());
    }

    /// Mirrors the reference test `TestSpillStore_CollectWith`.
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

    /// Mirrors the reference test `TestSpillStore_Cleanup`.
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

    /// Mirrors the reference test `TestSpillStore_RestoreFromDir`.
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

    /// Mirrors the reference test `TestSpillStore_StructValues`.
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

    /// Mirrors the reference test `TestSpillStore_PointerValues`, with the
    /// pointer value type modeled as `Box<TestStruct>`; round-trip equality is
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
            collected.get("x").map(Box::as_ref),
            Some(&TestStruct {
                name: "hello".to_string(),
                value: 42
            })
        );
        assert_eq!(
            collected.get("y").map(Box::as_ref),
            Some(&TestStruct {
                name: "world".to_string(),
                value: 99
            })
        );
    }

    /// Additional coverage: collect resets `spill_n` and clears the buffer so
    /// the store is reusable.
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

    /// Additional coverage: `for_each_spill` visits chunks in ascending order
    /// and does not clean up the spill directory.
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
