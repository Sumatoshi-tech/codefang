//! Cached blob data (Rust port of Go `pkg/gitlib/cached_blob.go`).
//!
//! [`CachedBlob`] holds a blob's hash, size, and materialized `Data`, with a
//! memoized line count (`-1` sentinel = binary). The Go type used
//! `textutil.IsBinary` / `textutil.CountLines` for the line-count and
//! binary-detection logic.
//!
//! **Transitive-dependency note (DESIGN rule 5):** `cf-textutil` is not yet
//! ported, so the text classification is abstracted behind the
//! [`TextClassifier`] trait. A small default implementation
//! ([`DefaultTextClassifier`]) is provided so this crate compiles and tests
//! standalone; once `cf-textutil` lands, swap the default for the real
//! `IsBinary`/`CountLines` so behavior matches Go byte-for-byte. See the crate
//! TODOs.

use crate::error::Result;
use crate::hash::Hash;
use crate::repository::Repository;
use std::cell::Cell;

/// Sentinel value indicating the blob is binary. Port of Go `lineCountBinary`.
const LINE_COUNT_BINARY: i64 = -1;

/// Error sentinel raised by [`CachedBlob::count_lines`] for binary files. Port
/// of Go `ErrBinary` ("binary").
pub const ERR_BINARY: &str = "binary";

/// Text classification used by [`CachedBlob`]. Abstracts `cf-textutil` until it
/// is ported (DESIGN rule 5).
pub trait TextClassifier {
    /// Report whether `data` appears to be binary. Must match
    /// `textutil.IsBinary` once `cf-textutil` is ported.
    fn is_binary(&self, data: &[u8]) -> bool;
    /// Count the lines in `data`. Must match `textutil.CountLines` once
    /// `cf-textutil` is ported.
    fn count_lines(&self, data: &[u8]) -> usize;
}

/// Placeholder text classifier with conservative defaults. **Not yet
/// byte-identical to Go `textutil`** — see the crate TODOs.
#[derive(Clone, Copy, Debug, Default)]
pub struct DefaultTextClassifier;

impl TextClassifier for DefaultTextClassifier {
    fn is_binary(&self, data: &[u8]) -> bool {
        // Heuristic placeholder: a NUL byte in the first 8000 bytes (the common
        // git/grep heuristic). Replace with textutil.IsBinary for parity.
        data.iter().take(8000).any(|&b| b == 0)
    }

    fn count_lines(&self, data: &[u8]) -> usize {
        // Placeholder: count newline bytes. Replace with textutil.CountLines for
        // parity (which may handle a missing final newline differently).
        data.iter().filter(|&&b| b == b'\n').count()
    }
}

/// A cached blob for efficient repeated access. Port of Go `CachedBlob`.
pub struct CachedBlob {
    hash: Hash,
    size: i64,
    /// The read contents of the blob object. Port of Go exported field `Data`.
    pub data: Vec<u8>,
    /// Memoized line count (`-1` = binary, `i64::MIN` = uncomputed).
    line_count: Cell<i64>,
}

impl CachedBlob {
    /// Build a [`CachedBlob`] from raw data, for tests. Port of Go
    /// `NewCachedBlobForTest`.
    #[must_use]
    pub fn new_for_test(data: Vec<u8>) -> Self {
        CachedBlob {
            hash: Hash::ZERO,
            size: data.len() as i64,
            data,
            line_count: Cell::new(i64::MIN),
        }
    }

    /// Build a [`CachedBlob`] from a hash and raw data, for tests. Port of Go
    /// `NewCachedBlobWithHashForTest`.
    #[must_use]
    pub fn new_with_hash_for_test(hash: Hash, data: Vec<u8>) -> Self {
        CachedBlob {
            hash,
            size: data.len() as i64,
            data,
            line_count: Cell::new(i64::MIN),
        }
    }

    /// Load and cache a blob from the repository. Port of Go
    /// `NewCachedBlobFromRepo`.
    ///
    /// # Errors
    /// Wraps the blob-lookup error as `"looking up blob <hash>: <err>"`,
    /// matching the Go wrapping.
    pub fn from_repo(repo: &Repository, blob_hash: Hash) -> Result<Self> {
        let blob = repo.lookup_blob(blob_hash).map_err(|e| {
            crate::GitError::Message(format!("looking up blob {blob_hash}: {e}"))
        })?;
        Ok(CachedBlob {
            hash: blob_hash,
            size: blob.size(),
            data: blob.contents(),
            line_count: Cell::new(i64::MIN),
        })
    }

    /// The blob hash. Port of Go `(CachedBlob).Hash`.
    #[must_use]
    pub fn hash(&self) -> Hash {
        self.hash
    }

    /// The blob size. Port of Go `(CachedBlob).Size`.
    #[must_use]
    pub fn size(&self) -> i64 {
        self.size
    }

    /// A reader over the cached data. Port of Go `(CachedBlob).Reader`.
    #[must_use]
    pub fn reader(&self) -> std::io::Cursor<&[u8]> {
        std::io::Cursor::new(self.data.as_slice())
    }

    /// A deep copy detaching the `data` slice. Port of Go `(CachedBlob).Clone`.
    /// Useful when the original data is part of a larger arena.
    #[must_use]
    pub fn clone_detached(&self) -> CachedBlob {
        CachedBlob {
            hash: self.hash,
            size: self.size,
            data: self.data.clone(),
            line_count: Cell::new(self.line_count.get()),
        }
    }

    /// The number of lines, memoized; returns `Err(ERR_BINARY)` for binary
    /// files. Port of Go `(CachedBlob).CountLines`.
    ///
    /// # Errors
    /// [`crate::GitError::Message`]`(ERR_BINARY)` when the blob is binary.
    pub fn count_lines(&self, classifier: &impl TextClassifier) -> Result<usize> {
        let mut lc = self.line_count.get();
        if lc == i64::MIN {
            lc = self.compute_line_count(classifier);
            self.line_count.set(lc);
        }
        if lc == LINE_COUNT_BINARY {
            return Err(crate::GitError::Message(ERR_BINARY.to_string()));
        }
        Ok(lc as usize)
    }

    fn compute_line_count(&self, classifier: &impl TextClassifier) -> i64 {
        if classifier.is_binary(&self.data) {
            LINE_COUNT_BINARY
        } else {
            classifier.count_lines(&self.data) as i64
        }
    }

    /// Report whether the blob appears binary. Port of Go
    /// `(CachedBlob).IsBinary`.
    #[must_use]
    pub fn is_binary(&self, classifier: &impl TextClassifier) -> bool {
        classifier.is_binary(&self.data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn for_test_sets_size() {
        let b = CachedBlob::new_for_test(b"hello".to_vec());
        assert_eq!(b.size(), 5);
        assert!(b.hash().is_zero());
    }

    #[test]
    fn count_lines_memoized() {
        let b = CachedBlob::new_for_test(b"a\nb\nc\n".to_vec());
        let c = DefaultTextClassifier;
        assert_eq!(b.count_lines(&c).unwrap(), 3);
        // Second call uses the cache.
        assert_eq!(b.count_lines(&c).unwrap(), 3);
    }

    #[test]
    fn binary_blob_errors() {
        let b = CachedBlob::new_for_test(vec![0u8, 1, 2, 0, 3]);
        let c = DefaultTextClassifier;
        assert!(b.is_binary(&c));
        assert!(b.count_lines(&c).is_err());
    }

    #[test]
    fn clone_detaches_data() {
        let b = CachedBlob::new_with_hash_for_test(Hash::new("deadbeef"), b"xy".to_vec());
        let cl = b.clone_detached();
        assert_eq!(cl.hash(), b.hash());
        assert_eq!(cl.data, b.data);
    }
}
