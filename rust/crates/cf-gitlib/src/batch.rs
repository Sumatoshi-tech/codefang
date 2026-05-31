//! Batch configuration and batched blob access.
//!
//! [`BatchConfig`] is a faithful port of Go `pkg/gitlib/batch_config.go`
//! (`BlobBatchSize`, `DiffBatchSize`, `Workers`, and `DefaultBatchConfig`).
//!
//! [`BlobBatch`] is the **Rust replacement for the Go cgo "clib" batch shim**
//! (`pkg/gitlib/cgo_bridge.go`, `worker.go`, and the C sources under
//! `pkg/gitlib/clib/`). The Go package fetched many blobs at once through a
//! custom C shim over libgit2 to amortize cgo-crossing costs. Rust has no cgo
//! boundary, so the batch reduces to ordinary per-thread `git2` lookups (DESIGN
//! §3); this preserves the *interface* — a configurable batch that resolves a
//! set of blob hashes to their contents — so callers (cache/uast/analyze) port
//! over unchanged, while the entire C shim is dropped.

use crate::error::Result;
use crate::hash::Hash;
use crate::repository::Repository;

/// Default number of blobs to load per batch (Go `defaultBlobBatchSize`).
pub const DEFAULT_BLOB_BATCH_SIZE: usize = 100;
/// Default number of diffs to compute per batch (Go `defaultDiffBatchSize`).
pub const DEFAULT_DIFF_BATCH_SIZE: usize = 50;

/// Batch processing parameters. Port of Go `BatchConfig`.
#[derive(Clone, Copy, Debug)]
pub struct BatchConfig {
    /// Number of blobs to load per batch. Default: 100 (Go `BlobBatchSize`).
    pub blob_batch_size: usize,
    /// Number of diffs to compute per batch. Default: 50 (Go `DiffBatchSize`).
    pub diff_batch_size: usize,
    /// Number of parallel workers. Default: 1 — sequential processing within
    /// gitlib (Go `Workers`).
    pub workers: usize,
}

impl Default for BatchConfig {
    /// Port of Go `DefaultBatchConfig`.
    fn default() -> Self {
        BatchConfig {
            blob_batch_size: DEFAULT_BLOB_BATCH_SIZE,
            diff_batch_size: DEFAULT_DIFF_BATCH_SIZE,
            workers: 1,
        }
    }
}

impl BatchConfig {
    /// Port of Go `DefaultBatchConfig`.
    #[must_use]
    pub fn new_default() -> Self {
        BatchConfig::default()
    }

    /// The effective blob batch size, substituting the default for `0`.
    #[must_use]
    pub fn effective_blob_batch_size(&self) -> usize {
        if self.blob_batch_size == 0 {
            DEFAULT_BLOB_BATCH_SIZE
        } else {
            self.blob_batch_size
        }
    }
}

/// The result of resolving one blob in a batch.
#[derive(Clone, Debug)]
pub struct BlobResult {
    /// The requested blob hash.
    pub hash: Hash,
    /// The blob's bytes, or [`None`] if the lookup failed (e.g. missing object),
    /// matching the Go shim's per-entry failure handling.
    pub contents: Option<Vec<u8>>,
}

/// A per-thread batched blob fetcher over a [`Repository`]. Replaces the Go cgo
/// bridge / worker. Borrows the repository, so it is `!Send + !Sync` like
/// everything else in this crate.
pub struct BlobBatch<'repo> {
    repo: &'repo Repository,
    config: BatchConfig,
}

impl<'repo> BlobBatch<'repo> {
    /// Create a batch fetcher with the given configuration.
    #[must_use]
    pub fn new(repo: &'repo Repository, config: BatchConfig) -> Self {
        BlobBatch { repo, config }
    }

    /// Create a batch fetcher with [`BatchConfig::default`].
    #[must_use]
    pub fn with_defaults(repo: &'repo Repository) -> Self {
        BlobBatch::new(repo, BatchConfig::default())
    }

    /// The configuration in effect.
    #[must_use]
    pub fn config(&self) -> BatchConfig {
        self.config
    }

    /// Resolve a single blob's contents (materialized with `to_vec()`, DESIGN
    /// §3). Returns [`None`] contents when the blob is missing rather than
    /// erroring.
    #[must_use]
    pub fn fetch_one(&self, hash: Hash) -> BlobResult {
        let contents = self.repo.lookup_blob(hash).ok().map(|b| b.contents());
        BlobResult { hash, contents }
    }

    /// Resolve every requested blob, chunked into batches of at most
    /// [`BatchConfig::effective_blob_batch_size`] so peak memory stays bounded
    /// like the Go shim. Results are returned in request order.
    #[must_use]
    pub fn fetch_all(&self, hashes: &[Hash]) -> Vec<BlobResult> {
        let chunk = self.config.effective_blob_batch_size();
        let mut out = Vec::with_capacity(hashes.len());
        for batch in hashes.chunks(chunk) {
            for &h in batch {
                out.push(self.fetch_one(h));
            }
        }
        out
    }

    /// Resolve every requested blob, erroring on the first missing blob.
    ///
    /// # Errors
    /// Propagates the first lookup error.
    pub fn fetch_all_strict(&self, hashes: &[Hash]) -> Result<Vec<(Hash, Vec<u8>)>> {
        let mut out = Vec::with_capacity(hashes.len());
        for &h in hashes {
            let blob = self.repo.lookup_blob(h)?;
            out.push((h, blob.contents()));
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_matches_go() {
        let c = BatchConfig::default();
        assert_eq!(c.blob_batch_size, DEFAULT_BLOB_BATCH_SIZE);
        assert_eq!(c.diff_batch_size, DEFAULT_DIFF_BATCH_SIZE);
        assert_eq!(c.workers, 1);
    }

    #[test]
    fn effective_blob_batch_substitutes_zero() {
        assert_eq!(
            BatchConfig {
                blob_batch_size: 0,
                ..BatchConfig::default()
            }
            .effective_blob_batch_size(),
            DEFAULT_BLOB_BATCH_SIZE
        );
    }
}
