//! Analysis cache for codefang (UAST / blob caching keyed by git).
//!
//! The crate has two independent concerns, each in its own module:
//!
//! * [`lru_blob`] — the cross-commit [`LruBlobCache`]: a size-bounded LRU over
//!   git blob data with a Bloom pre-filter and sampled, cost-based eviction,
//!   built on the generic [`cf_alg_lru::Cache`].
//! * [`meta`] — the incremental-analysis metadata helpers: the
//!   [`IncrementalMeta`] record plus [`key`], [`is_stale`], [`write_meta`], and
//!   [`read_meta`] reading/writing `cache.json`.
//!
//! The minimal git value types the blob cache needs ([`GitHash`], [`CachedBlob`])
//! live in [`gitlib`] until the full `cf-gitlib` crate provides them (see that
//! module's replacement note).
//!
//! This crate does **not** depend on the framework; the dependency direction is
//! always framework -> cache, never the reverse.
//!
//! # Serialization byte-identity
//!
//! The only bytes this crate emits are the on-disk `cache.json` written by
//! [`write_meta`]. Although that file is internal state rather than a
//! user-facing machine report, its bytes are part of the compatibility contract
//! and are routed through the shared report-format JSON writer
//! ([`cf_textutil::write_json`], over `cf-gojson`) rather than raw `serde_json`.
//! Parsing on read uses `serde_json` (decoding never affects output bytes).
//!
//! Compatibility: output bytes are pinned against the reference implementation
//! by `tests/compat`.
//!
//! # Example
//!
//! ```
//! use cf_cache::{key, is_stale, IncrementalMeta, LruBlobCache, CachedBlob, GitHash};
//!
//! // Cache keys are deterministic from (root SHA, branch).
//! let k = key("rootsha", "main");
//! assert_eq!(k, key("rootsha", "main"));
//! assert_ne!(k, key("rootsha", "dev"));
//!
//! // Staleness is detected when the recorded root SHA diverges from current.
//! let meta = IncrementalMeta { root_sha: "abc".into(), ..Default::default() };
//! assert!(!is_stale(&meta, "abc"));
//! assert!(is_stale(&meta, "xyz")); // force-push / history rewrite
//!
//! // The LRU blob cache stores and retrieves blobs keyed by git hash.
//! let cache = LruBlobCache::new(1 << 20);
//! let hash = GitHash([1u8; 20]);
//! assert!(cache.get(&hash).is_none());
//! cache.put(hash, Some(CachedBlob::with_hash_for_test(hash, b"hello".to_vec())));
//! assert_eq!(cache.get(&hash).unwrap().data, b"hello");
//! ```

pub mod gitlib;
pub mod lru_blob;
pub mod meta;

#[doc(inline)]
pub use gitlib::{CachedBlob, GitHash};
#[doc(inline)]
pub use lru_blob::{LruBlobCache, LruStats, DEFAULT_LRU_CACHE_SIZE};
#[doc(inline)]
pub use meta::{
    is_stale, key, read_meta, write_meta, IncrementalMeta, ReadMetaError, WriteMetaError,
    META_FILENAME, META_FILE_PERM,
};
