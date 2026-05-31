//! Analysis cache for codefang (UAST / blob caching keyed by git).
//!
//! Faithful Rust port of the Go `internal/cache` package. The Go package has two
//! independent concerns, each mirrored by a module here:
//!
//! * [`lru_blob`] — the cross-commit [`LruBlobCache`] (Go `lru.go` /
//!   `LRUBlobCache`): a size-bounded LRU over git blob data with a Bloom
//!   pre-filter and sampled, cost-based eviction, built on the generic
//!   [`cf_alg_lru::Cache`].
//! * [`meta`] — the incremental-analysis metadata helpers (Go `incremental.go`):
//!   the [`IncrementalMeta`] record plus [`key`], [`is_stale`], [`write_meta`],
//!   and [`read_meta`] reading/writing `cache.json`.
//!
//! The minimal git value types the blob cache needs ([`GitHash`], [`CachedBlob`])
//! live in [`gitlib`] until the full `cf-gitlib` crate is ported (see that
//! module's replacement note).
//!
//! This crate does **not** depend on the framework, matching the Go package's
//! dependency direction (`internal/framework` depends on `internal/cache`, never
//! the reverse).
//!
//! # Serialization byte-identity
//!
//! The only bytes this crate emits are the on-disk `cache.json` written by
//! [`write_meta`]. Although that file is internal state rather than a user-facing
//! machine report, its bytes are still routed through the shared
//! Go-byte-compatible writer ([`cf_textutil::write_json`], over `cf-gojson`)
//! rather than raw `serde_json`, per the rewrite design. Parsing on read uses
//! `serde_json` (decoding a Go-produced artifact does not affect output bytes).

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
