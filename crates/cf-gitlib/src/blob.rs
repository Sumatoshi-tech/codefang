//! Blob access.
//!
//! [`Blob`] is a thin borrow over a libgit2 [`git2::Blob`], freed via RAII
//! [`Drop`]. [`Blob::contents`] returns an owned `Vec<u8>` via
//! `content().to_vec()`, per the design directive to copy blob bytes out of
//! libgit2-owned memory.
//!
//! [`CachedBlob`] caches a blob's bytes plus a lazily-computed, memoized line
//! count, with [`GitError::Binary`] signalling binary content.

use std::cell::Cell;

use crate::error::{GitError, Result};
use crate::hash::Hash;

/// Sentinel line-count value meaning "binary".
const LINE_COUNT_BINARY: i64 = -1;

/// A libgit2 blob.
///
/// The underlying [`git2::Blob`] is freed on [`Drop`], so this type borrows
/// its parent [`crate::Repository`] for the blob's lifetime — `!Send`/`!Sync`,
/// single thread, matching libgit2's threading model.
pub struct Blob<'repo> {
    blob: git2::Blob<'repo>,
}

impl<'repo> Blob<'repo> {
    /// Wraps a libgit2 blob.
    pub(crate) fn new(blob: git2::Blob<'repo>) -> Self {
        Blob { blob }
    }

    /// Returns the blob hash.
    #[must_use]
    pub fn hash(&self) -> Hash {
        Hash::from_oid(&self.blob.id())
    }

    /// Returns the blob size in bytes.
    #[must_use]
    pub fn size(&self) -> i64 {
        self.blob.size() as i64
    }

    /// Returns an owned copy of the blob contents.
    ///
    /// Uses `content().to_vec()` per the design: blob bytes are copied out of
    /// libgit2-owned storage so the returned `Vec` outlives the blob.
    #[must_use]
    pub fn contents(&self) -> Vec<u8> {
        self.blob.content().to_vec()
    }

    /// Borrows the blob contents without copying.
    ///
    /// A slice is the idiomatic reader source; it is valid only while the blob
    /// is alive.
    #[must_use]
    pub fn content(&self) -> &[u8] {
        self.blob.content()
    }

    /// Returns the underlying libgit2 blob.
    #[must_use]
    pub fn native(&self) -> &git2::Blob<'repo> {
        &self.blob
    }
}

/// A cached blob: its hash, size, owned bytes, and a memoized line count.
///
/// [`CachedBlob::count_lines`] computes the line count once and caches it,
/// returning [`GitError::Binary`] for binary content.
///
/// The line-count cache uses [`Cell`], so `count_lines` takes `&self`. This
/// type owns its data and carries no libgit2 handle, so it can move freely
/// across threads.
#[derive(Debug, Clone)]
pub struct CachedBlob {
    hash: Hash,
    size: i64,
    /// The read contents of the blob object.
    pub data: Vec<u8>,
    /// Cached line count: `None` = not yet computed, `Some(-1)` = binary.
    line_count: Cell<Option<i64>>,
}

impl CachedBlob {
    /// Creates a [`CachedBlob`] from raw data.
    #[must_use]
    pub fn for_test(data: Vec<u8>) -> Self {
        CachedBlob {
            hash: Hash::zero(),
            size: data.len() as i64,
            data,
            line_count: Cell::new(None),
        }
    }

    /// Creates a [`CachedBlob`] with a given hash.
    #[must_use]
    pub fn with_hash_for_test(hash: Hash, data: Vec<u8>) -> Self {
        CachedBlob {
            hash,
            size: data.len() as i64,
            data,
            line_count: Cell::new(None),
        }
    }

    /// Loads and caches a blob from the repository.
    ///
    /// # Errors
    ///
    /// Returns [`GitError::CachedBlobLookup`]
    /// when the blob cannot be found.
    pub fn from_repo(repo: &crate::Repository, blob_hash: Hash) -> Result<Self> {
        let blob = repo.lookup_blob(blob_hash).map_err(|e| {
            // The wrapper text "looking up blob <hex>: <cause>" is a frozen
            // error string. The inner lookup error is
            // GitError::LookupBlob(git2::Error); unwrap it to the underlying
            // libgit2 error so the message carries the raw cause.
            match e {
                GitError::LookupBlob(src) => GitError::CachedBlobLookup {
                    hash: blob_hash.to_string(),
                    source: src,
                },
                other => other,
            }
        })?;

        Ok(CachedBlob {
            hash: blob_hash,
            size: blob.size(),
            data: blob.contents(),
            line_count: Cell::new(None),
        })
    }

    /// Constructs a [`CachedBlob`] from already-loaded batch fields.
    ///
    /// Used by the batch worker (`worker.rs`) where blob bytes and a precomputed
    /// line count are produced together.
    #[must_use]
    pub(crate) fn from_parts(hash: Hash, size: i64, data: Vec<u8>, line_count: i64) -> Self {
        CachedBlob {
            hash,
            size,
            data,
            line_count: Cell::new(Some(line_count)),
        }
    }

    /// Returns the blob hash.
    #[must_use]
    pub fn hash(&self) -> Hash {
        self.hash
    }

    /// Returns the blob size.
    #[must_use]
    pub fn size(&self) -> i64 {
        self.size
    }

    /// Deep-copies the blob, detaching the data slice.
    ///
    /// A [`Vec`] already owns its bytes, so this is a straightforward deep copy
    /// that also carries the memoized line count forward.
    #[must_use]
    pub fn clone_detached(&self) -> Self {
        CachedBlob {
            hash: self.hash,
            size: self.size,
            data: self.data.clone(),
            line_count: Cell::new(self.line_count.get()),
        }
    }

    /// Returns the number of lines, or [`GitError::Binary`] for binary content.
    ///
    /// The result is computed once and cached. Binary blobs cache the sentinel
    /// and always return [`GitError::Binary`].
    ///
    /// # Errors
    ///
    /// Returns [`GitError::Binary`] when the blob is binary.
    pub fn count_lines(&self) -> Result<usize> {
        let cached = match self.line_count.get() {
            Some(v) => v,
            None => {
                let v = self.compute_line_count();
                self.line_count.set(Some(v));
                v
            }
        };

        if cached == LINE_COUNT_BINARY {
            return Err(GitError::Binary);
        }

        Ok(cached as usize)
    }

    /// Computes the line count or the binary sentinel.
    fn compute_line_count(&self) -> i64 {
        if cf_textutil::is_binary(&self.data) {
            return LINE_COUNT_BINARY;
        }
        cf_textutil::count_lines(&self.data) as i64
    }

    /// Reports whether the blob looks binary.
    #[must_use]
    pub fn is_binary(&self) -> bool {
        cf_textutil::is_binary(&self.data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Mirrors reference test TestCachedBlob_CountLines_Caching.
    #[test]
    fn count_lines_caches() {
        let blob = CachedBlob::for_test(b"line1\nline2\nline3\n".to_vec());
        assert_eq!(blob.count_lines().unwrap(), 3);
        assert_eq!(blob.count_lines().unwrap(), 3);
    }

    // Mirrors reference test TestCachedBlob_CountLines_BinaryCaching.
    #[test]
    fn count_lines_binary_caches() {
        let blob = CachedBlob::for_test(b"binary\x00data".to_vec());
        assert!(matches!(blob.count_lines(), Err(GitError::Binary)));
        assert!(matches!(blob.count_lines(), Err(GitError::Binary)));
    }

    // Mirrors reference test TestCachedBlob_CountLines_EmptyBlob.
    #[test]
    fn count_lines_empty() {
        let blob = CachedBlob::for_test(Vec::new());
        assert_eq!(blob.count_lines().unwrap(), 0);
    }

    // Mirrors reference test TestCachedBlob_CountLines_NoTrailingNewline.
    #[test]
    fn count_lines_no_trailing_newline() {
        let blob = CachedBlob::for_test(b"line1\nline2".to_vec());
        assert_eq!(blob.count_lines().unwrap(), 2);
    }

    #[test]
    fn clone_detached_carries_line_count() {
        let blob = CachedBlob::with_hash_for_test(Hash::new("ab"), b"a\nb\n".to_vec());
        assert_eq!(blob.count_lines().unwrap(), 2);
        let c = blob.clone_detached();
        assert_eq!(c.hash(), blob.hash());
        assert_eq!(c.data, blob.data);
        assert_eq!(c.count_lines().unwrap(), 2);
    }
}
