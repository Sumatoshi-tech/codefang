//! `BlobCache` provider and the [`CachedBlob`] value it produces.
//!
//! The provider reads `"changes"` and `"commit"` and produces `"blob_cache"`,
//! a map from blob [`struct@Hash`] to [`CachedBlob`] holding
//! the raw bytes of every blob touched by the commit.

use std::collections::HashMap;

use crate::analyzer::{dep, Analyzer, AnalyzerError, ValueMap};
use crate::git_model::{Action, Changes, Hash};

/// Sentinel error indicating a blob is binary. The `"binary"` message is part
/// of the error-text contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("binary")]
pub struct ErrorBinary;

/// A blob with its raw bytes cached in memory.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CachedBlob {
    /// Raw blob contents.
    pub data: Vec<u8>,
}

impl CachedBlob {
    /// Construct a cached blob from raw bytes.
    #[must_use]
    pub const fn new(data: Vec<u8>) -> Self {
        Self { data }
    }

    /// Return the blob contents as a string.
    ///
    /// Invalid UTF-8 is replaced lossily ([`String::from_utf8_lossy`]);
    /// callers that need exact bytes should use [`CachedBlob::data`]
    /// directly.
    #[must_use]
    pub fn str(&self) -> std::borrow::Cow<'_, str> {
        String::from_utf8_lossy(&self.data)
    }

    /// Count the number of lines.
    ///
    /// Semantics (frozen contract):
    /// * empty data -> `Ok(0)`.
    /// * binary data (contains a NUL byte) -> `Err(ErrorBinary)`.
    /// * otherwise count `\n`-separated segments; a trailing newline does not
    ///   add a final empty line.
    ///
    /// # Errors
    ///
    /// Returns [`ErrorBinary`] when the data contains a NUL byte.
    pub fn count_lines(&self) -> Result<usize, ErrorBinary> {
        if self.data.is_empty() {
            return Ok(0);
        }
        if is_binary(&self.data) {
            return Err(ErrorBinary);
        }
        Ok(count_lines_bytes(&self.data))
    }
}

/// Whether the byte slice looks binary (a NUL byte anywhere marks the content
/// as binary).
pub(crate) fn is_binary(data: &[u8]) -> bool {
    data.contains(&0)
}

/// Count `\n`-delimited segments, trimming a single trailing-newline empty
/// segment.
///
/// For non-empty input this equals the number of `\n` bytes when the data ends
/// in `\n`, and that plus one otherwise.
pub(crate) fn count_lines_bytes(data: &[u8]) -> usize {
    debug_assert!(!data.is_empty());
    let newlines = data.iter().filter(|&&b| b == b'\n').count();
    if *data.last().unwrap() == b'\n' {
        newlines
    } else {
        newlines + 1
    }
}

/// Abstraction over blob fetching, decoupling the provider from the concrete
/// repository handle.
///
/// A trait so the provider can be unit-tested without a real repository and
/// so a `git2::Repository` (which is not `Send`/`Sync`) does not have to live
/// inside the provider struct.
pub trait BlobSource {
    /// Read the raw bytes of the blob with the given hash.
    ///
    /// # Errors
    ///
    /// Returns an [`AnalyzerError`] when the blob cannot be read.
    fn read_blob(&self, hash: Hash) -> Result<Vec<u8>, AnalyzerError>;
}

/// A [`BlobSource`] backed by a libgit2 repository.
pub struct GitBlobSource<'r> {
    repo: &'r git2::Repository,
}

impl<'r> GitBlobSource<'r> {
    /// Wrap a borrowed repository as a blob source.
    #[must_use]
    pub const fn new(repo: &'r git2::Repository) -> Self {
        Self { repo }
    }
}

impl BlobSource for GitBlobSource<'_> {
    fn read_blob(&self, hash: Hash) -> Result<Vec<u8>, AnalyzerError> {
        let oid = git2::Oid::from_bytes(&hash.0)
            .map_err(|e| AnalyzerError::Git(e.to_string()))?;
        let blob = self.repo.find_blob(oid)?;
        Ok(blob.content().to_vec())
    }
}

/// `BlobCache` provider.
///
/// Holds the [`BlobSource`] used to fetch blob contents and the previous
/// commit's cache, reused to avoid re-reading unchanged from-side blobs
/// across commits. On a failed read an empty placeholder blob is inserted
/// rather than dropping the entry (reference-implementation behavior).
pub struct BlobCache<S: BlobSource> {
    source: S,
    previous_cache: HashMap<Hash, CachedBlob>,
}

impl<S: BlobSource> BlobCache<S> {
    /// Construct a `BlobCache` over the given blob source.
    pub fn new(source: S) -> Self {
        BlobCache {
            source,
            previous_cache: HashMap::new(),
        }
    }

    /// Build the cache for one commit's changes: each side that should be
    /// cached is read fresh, except a modify/delete from-side that is already
    /// present in the previous commit's cache, which is reused. Read failures
    /// store an empty placeholder blob. The previous cache is then advanced.
    /// (The reference implementation parallelizes this loop; the result is
    /// order-independent, so it runs sequentially here.)
    pub fn build(&mut self, changes: &Changes) -> HashMap<Hash, CachedBlob> {
        let mut cache: HashMap<Hash, CachedBlob> = HashMap::new();
        let mut new_cache: HashMap<Hash, CachedBlob> = HashMap::new();
        for change in changes {
            match change.action() {
                Some(Action::Insert) => {
                    self.handle_insert(change.to.hash, &mut cache, &mut new_cache);
                }
                Some(Action::Delete) => {
                    self.handle_delete(change.from.hash, &mut cache);
                }
                Some(Action::Modify) => {
                    self.handle_insert(change.to.hash, &mut cache, &mut new_cache);
                    self.handle_modify_from(change.from.hash, &mut cache);
                }
                None => {}
            }
        }
        self.previous_cache = new_cache;
        cache
    }

    /// Insert handling: read the to-side, placeholder on failure, record in
    /// both the working cache and the next-commit cache.
    fn handle_insert(
        &self,
        hash: Hash,
        cache: &mut HashMap<Hash, CachedBlob>,
        new_cache: &mut HashMap<Hash, CachedBlob>,
    ) {
        match self.source.read_blob(hash) {
            Ok(data) => {
                let blob = CachedBlob::new(data);
                cache.insert(hash, blob.clone());
                new_cache.insert(hash, blob);
            }
            Err(_) => {
                cache.insert(hash, CachedBlob::default());
                new_cache.insert(hash, CachedBlob::default());
            }
        }
    }

    /// Delete handling: reuse the previous cache if present, else read fresh
    /// (placeholder on failure).
    fn handle_delete(&self, hash: Hash, cache: &mut HashMap<Hash, CachedBlob>) {
        if let Some(existing) = self.previous_cache.get(&hash) {
            cache.insert(hash, existing.clone());
            return;
        }
        match self.source.read_blob(hash) {
            Ok(data) => cache.insert(hash, CachedBlob::new(data)),
            Err(_) => cache.insert(hash, CachedBlob::default()),
        };
    }

    /// Modify handling for the from-side: reuse the previous cache if
    /// present, else read fresh (placeholder on failure).
    fn handle_modify_from(&self, hash: Hash, cache: &mut HashMap<Hash, CachedBlob>) {
        self.handle_delete(hash, cache);
    }
}

impl<S: BlobSource> Analyzer for BlobCache<S> {
    fn name(&self) -> &'static str {
        "BlobCache"
    }

    fn provides(&self) -> Vec<&'static str> {
        vec!["blob_cache"]
    }

    fn requires(&self) -> Vec<&'static str> {
        vec!["changes"]
    }

    fn consume(&mut self, deps: &mut ValueMap) -> Result<ValueMap, AnalyzerError> {
        let changes = dep::<Changes>(deps, "changes")?.clone();
        let cache = self.build(&changes);
        let mut out = ValueMap::new();
        out.insert("blob_cache".to_string(), Box::new(cache));
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git_model::Change;

    // Mirrors reference test TestCachedBlob_CountLines.
    #[test]
    fn count_lines_table() {
        let cases: &[(&str, Vec<u8>, usize, bool)] = &[
            ("empty", vec![], 0, false),
            ("single line no newline", b"hello".to_vec(), 1, false),
            ("single line with newline", b"hello\n".to_vec(), 1, false),
            ("two lines", b"hello\nworld".to_vec(), 2, false),
            ("two lines trailing newline", b"hello\nworld\n".to_vec(), 2, false),
            ("binary", vec![0x00, 0x01, 0x02], 0, true),
        ];
        for (name, data, want, want_err) in cases {
            let b = CachedBlob::new(data.clone());
            match b.count_lines() {
                Ok(got) => {
                    assert!(!*want_err, "{name}: expected error, got Ok({got})");
                    assert_eq!(got, *want, "{name}: line count mismatch");
                }
                Err(_) => {
                    assert!(*want_err, "{name}: unexpected error");
                }
            }
        }
    }

    // Mirrors reference test TestCachedBlob_Str.
    #[test]
    fn str_round_trips() {
        let b = CachedBlob::new(b"hello world".to_vec());
        assert_eq!(b.str(), "hello world");
    }

    struct MapSource(HashMap<Hash, Vec<u8>>);
    impl BlobSource for MapSource {
        fn read_blob(&self, hash: Hash) -> Result<Vec<u8>, AnalyzerError> {
            self.0
                .get(&hash)
                .cloned()
                .ok_or_else(|| AnalyzerError::Other("missing blob".into()))
        }
    }

    fn h(n: u8) -> Hash {
        let mut b = [0u8; 20];
        b[0] = n;
        Hash(b)
    }

    #[test]
    fn build_caches_inserts_modifies_and_deletes() {
        let mut blobs = HashMap::new();
        blobs.insert(h(1), b"added\n".to_vec());
        blobs.insert(h(2), b"old\n".to_vec());
        blobs.insert(h(3), b"new\n".to_vec());
        blobs.insert(h(4), b"gone\n".to_vec());
        let mut cache = BlobCache::new(MapSource(blobs));

        let changes: Changes = vec![
            // insert: from empty, to h(1)
            Change {
                from: crate::git_model::ChangeEntry::default(),
                to: crate::git_model::ChangeEntry { name: "a".into(), hash: h(1) },
            },
            // modify: from h(2) to h(3)
            Change {
                from: crate::git_model::ChangeEntry { name: "b".into(), hash: h(2) },
                to: crate::git_model::ChangeEntry { name: "b".into(), hash: h(3) },
            },
            // delete: from h(4), to empty
            Change {
                from: crate::git_model::ChangeEntry { name: "c".into(), hash: h(4) },
                to: crate::git_model::ChangeEntry::default(),
            },
        ];

        let result = cache.build(&changes);
        assert_eq!(result.get(&h(1)).unwrap().data, b"added\n");
        assert_eq!(result.get(&h(3)).unwrap().data, b"new\n"); // to-hash for modify
        assert_eq!(result.get(&h(4)).unwrap().data, b"gone\n"); // from-hash for delete
        // The from-side of a modify is ALSO cached (read fresh when not in
        // the previous cache); h(2) is therefore present.
        assert_eq!(result.get(&h(2)).unwrap().data, b"old\n");
    }

    #[test]
    fn modify_from_side_reuses_previous_cache() {
        let mut blobs = HashMap::new();
        blobs.insert(h(2), b"old\n".to_vec());
        blobs.insert(h(3), b"new\n".to_vec());
        let mut cache = BlobCache::new(MapSource(blobs));
        // Seed the previous cache as if h(2) was produced by an earlier commit.
        cache.previous_cache.insert(h(2), CachedBlob::new(b"reused\n".to_vec()));
        let changes: Changes = vec![Change {
            from: crate::git_model::ChangeEntry { name: "b".into(), hash: h(2) },
            to: crate::git_model::ChangeEntry { name: "b".into(), hash: h(3) },
        }];
        let result = cache.build(&changes);
        // from-side reused from previous cache, not re-read.
        assert_eq!(result.get(&h(2)).unwrap().data, b"reused\n");
    }

    #[test]
    fn read_failure_yields_empty_placeholder() {
        // Empty source: every read fails.
        let mut cache = BlobCache::new(MapSource(HashMap::new()));
        let changes: Changes = vec![Change {
            from: Default::default(),
            to: crate::git_model::ChangeEntry { name: "a".into(), hash: h(1) },
        }];
        let result = cache.build(&changes);
        assert_eq!(result.get(&h(1)).unwrap().data, b"");
    }

    #[test]
    fn provider_metadata() {
        let cache = BlobCache::new(MapSource(HashMap::new()));
        assert_eq!(cache.name(), "BlobCache");
        assert_eq!(cache.provides(), vec!["blob_cache"]);
        assert_eq!(cache.requires(), vec!["changes"]);
    }
}
