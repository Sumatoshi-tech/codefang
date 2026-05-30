//! A generic thread-safe LRU cache with optional Bloom pre-filtering,
//! size-based eviction, and cost-aware eviction sampling.
//!
//! This is a faithful Rust port of the Go package `pkg/alg/lru`
//! (`cache.go`, `ops.go`, `stats.go`). Behavior is reproduced exactly:
//!
//! - **LRU ordering** is maintained with an intrusive doubly-linked list. The
//!   Go implementation uses heap pointers; this port uses index handles into a
//!   slab (a [`Vec`] with a free-list), giving byte-for-byte identical eviction
//!   order without any `unsafe` code.
//! - **Count-based eviction** ([`Builder::with_max_entries`]): the cache holds
//!   at most `n` entries; the least-recently-used entry is evicted first.
//! - **Size-based eviction** ([`Builder::with_max_bytes`]): a per-value size
//!   function bounds the total cached bytes; oversized values (larger than the
//!   whole cache) are silently rejected.
//! - **Cost-based eviction** ([`Builder::with_cost_eviction`]): instead of
//!   always evicting the tail, sample up to `sample_size` entries from the LRU
//!   tail and evict the one with the lowest cost.
//! - **Bloom pre-filtering** ([`Builder::with_bloom_filter`]): definite misses
//!   on `get` / `get_multi` are short-circuited without taking the main lock.
//! - **Value cloning on insert** ([`Builder::with_clone_func`]): detach values
//!   from shared memory before storing.
//! - **Lock-free metrics**: hits, misses, and Bloom-filtered counts are atomic.
//!
//! At least one capacity limit ([`Builder::with_max_entries`] or
//! [`Builder::with_max_bytes`]) must be provided to [`Cache::new`]; otherwise
//! `new` panics, exactly as the Go `New` does.
//!
//! This crate is internal behavior only: per `specs/rust-rewrite/DESIGN.md` the
//! LRU cache is used by the cache layer and the analyzer framework as an
//! in-memory cache and never appears in any machine-format report. It therefore
//! does not depend on the `cf-gojson` / `cf-goyaml` byte-compat serialization
//! crates — there is no user-visible serialization surface. Its only dependency
//! is the sibling Bloom-filter crate, mirroring the Go import of `pkg/alg/bloom`.
//!
//! # Concurrency
//!
//! Like the Go cache, this type is safe to share across threads. The mutable
//! cache state (map + list + sizes) is guarded by an [`std::sync::RwLock`], and
//! the Bloom filter (which `get` reads outside that lock for the fast-path
//! short-circuit) is guarded by its own [`std::sync::RwLock`] so reads do not
//! contend with the main lock — matching Go's design where `filter.Test` is
//! called before acquiring the cache mutex.
//!
//! # Example
//!
//! ```
//! use cf_alg_lru::Cache;
//!
//! let cache: Cache<i64, String> = Cache::new(|c| {
//!     c.with_max_entries(2);
//! });
//!
//! cache.put(1, "a".to_string());
//! cache.put(2, "b".to_string());
//! cache.get(&1); // touch key 1 so it is most-recently-used
//! cache.put(3, "c".to_string()); // evicts key 2 (least-recently-used)
//!
//! assert!(cache.get(&1).is_some());
//! assert!(cache.get(&2).is_none());
//! assert!(cache.get(&3).is_some());
//! ```

#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::RwLock;

use cf_alg_bloom::Filter;

mod ops;
mod stats;

pub use stats::Stats;

/// The default false-positive rate for the Bloom pre-filter.
///
/// At 1%, 99% of definite cache misses are short-circuited without lock
/// acquisition. Mirrors `defaultBloomFPRate` in the Go `cache.go`.
const DEFAULT_BLOOM_FP_RATE: f64 = 0.01;

/// A sentinel index meaning "no node" in the intrusive list (Go's `nil`).
const NIL: usize = usize::MAX;

/// A doubly-linked-list node holding a key-value pair.
///
/// Mirrors the Go `entry[K, V]` struct. `prev`/`next` are slab indices rather
/// than pointers; [`NIL`] plays the role of a nil pointer.
struct Entry<K, V> {
    key: K,
    value: V,
    size: i64,
    access_count: i64,
    prev: usize,
    next: usize,
}

/// The mutable interior of the cache, guarded by a single write lock.
///
/// Holds the key→slot map, the slab of [`Entry`] nodes with its free-list, the
/// head/tail of the LRU list, and the running size accounting. This bundles
/// everything the Go code mutates under `c.mu` into one lock-guarded value.
struct Inner<K, V> {
    /// Slab storage for entries. Slots are reused via `free`.
    slots: Vec<Option<Entry<K, V>>>,
    /// Free-list of reusable slot indices.
    free: Vec<usize>,
    /// Maps a key to its slab index.
    index: HashMap<K, usize>,
    /// Most recently used slot, or [`NIL`].
    head: usize,
    /// Least recently used slot, or [`NIL`].
    tail: usize,
    /// Current total size in bytes (sum of entry sizes).
    cur_size: i64,
}

impl<K, V> Inner<K, V> {
    fn new() -> Self {
        Self {
            slots: Vec::new(),
            free: Vec::new(),
            index: HashMap::new(),
            head: NIL,
            tail: NIL,
            cur_size: 0,
        }
    }

    /// Returns a shared reference to the entry at `idx`.
    fn entry(&self, idx: usize) -> &Entry<K, V> {
        self.slots[idx]
            .as_ref()
            .expect("lru: slot referenced after free (internal invariant violated)")
    }

    /// Returns a mutable reference to the entry at `idx`.
    fn entry_mut(&mut self, idx: usize) -> &mut Entry<K, V> {
        self.slots[idx]
            .as_mut()
            .expect("lru: slot referenced after free (internal invariant violated)")
    }

    /// Allocates a slot for `entry`, reusing a freed slot when available.
    fn alloc(&mut self, entry: Entry<K, V>) -> usize {
        if let Some(idx) = self.free.pop() {
            self.slots[idx] = Some(entry);
            idx
        } else {
            self.slots.push(Some(entry));
            self.slots.len() - 1
        }
    }

    /// Frees the slot at `idx`, returning the removed entry.
    fn dealloc(&mut self, idx: usize) -> Entry<K, V> {
        let entry = self.slots[idx]
            .take()
            .expect("lru: double free of slot (internal invariant violated)");
        self.free.push(idx);
        entry
    }
}

/// Optional function converting a key to its Bloom-filter byte representation.
///
/// Mirrors Go's `keyToBytes func(K) []byte`.
type KeyToBytes<K> = Box<dyn Fn(&K) -> Vec<u8> + Send + Sync>;

/// Optional function returning the size in bytes of a value.
///
/// Mirrors Go's `sizeFunc func(V) int64`.
type SizeFunc<V> = Box<dyn Fn(&V) -> i64 + Send + Sync>;

/// Optional function cloning a value before insertion.
///
/// Mirrors Go's `cloneFunc func(V) V`.
type CloneFunc<V> = Box<dyn Fn(&V) -> V + Send + Sync>;

/// Optional cost function for sampling-based eviction.
///
/// Receives `(access_count, size_bytes)`; lower cost is evicted first. Mirrors
/// Go's `costFunc func(accessCount, sizeBytes int64) float64`.
type CostFunc = Box<dyn Fn(i64, i64) -> f64 + Send + Sync>;

/// A thread-safe generic LRU cache.
///
/// Supports optional Bloom pre-filtering, size-based eviction, cost-aware
/// eviction sampling, and value cloning on insertion. See the [crate-level
/// documentation](crate) for an overview.
///
/// Construct one with [`Cache::new`], configuring it inside the closure with the
/// `with_*` builder methods (mirroring Go's functional options).
pub struct Cache<K, V>
where
    K: Eq + Hash + Clone,
{
    inner: RwLock<Inner<K, V>>,

    // Capacity limits.
    max_entries: i64,
    max_size: i64,

    // Optional features.
    filter: Option<RwLock<Filter>>,
    key_to_bytes: Option<KeyToBytes<K>>,
    size_func: Option<SizeFunc<V>>,
    clone_func: Option<CloneFunc<V>>,

    // Cost-based eviction.
    cost_func: Option<CostFunc>,
    sample_size: i64,

    // Metrics (atomic for lock-free reads).
    hits: AtomicI64,
    misses: AtomicI64,
    bloom_filtered: AtomicI64,
}

/// A builder handle passed to the [`Cache::new`] configuration closure.
///
/// This plays the role of Go's `Option[K, V]` functional options: each method
/// records one configuration choice. It is a thin mutable view over the
/// not-yet-finalized [`Cache`].
pub struct Builder<'a, K, V>
where
    K: Eq + Hash + Clone,
{
    cache: &'a mut Cache<K, V>,
}

impl<K, V> Builder<'_, K, V>
where
    K: Eq + Hash + Clone,
{
    /// Sets the maximum number of entries (count-based eviction).
    ///
    /// Mirrors Go's `WithMaxEntries`.
    pub fn with_max_entries(&mut self, n: i64) -> &mut Self {
        self.cache.max_entries = n;
        self
    }

    /// Sets the maximum total size in bytes and a function to compute the size
    /// of each value, enabling size-based eviction.
    ///
    /// Mirrors Go's `WithMaxBytes`.
    pub fn with_max_bytes<F>(&mut self, max_bytes: i64, size_func: F) -> &mut Self
    where
        F: Fn(&V) -> i64 + Send + Sync + 'static,
    {
        self.cache.max_size = max_bytes;
        self.cache.size_func = Some(Box::new(size_func));
        self
    }

    /// Enables a Bloom pre-filter for [`Cache::get`] and [`Cache::get_multi`].
    ///
    /// `key_to_bytes` converts a key to its byte representation; `expected_n` is
    /// the expected number of elements used for Bloom filter sizing. Mirrors
    /// Go's `WithBloomFilter`.
    ///
    /// # Panics
    ///
    /// Panics if the Bloom filter cannot be constructed. As in Go, this is
    /// structurally impossible: `expected_n` is clamped to at least 1 and the
    /// false-positive rate is the constant [`DEFAULT_BLOOM_FP_RATE`].
    pub fn with_bloom_filter<F>(&mut self, key_to_bytes: F, expected_n: usize) -> &mut Self
    where
        F: Fn(&K) -> Vec<u8> + Send + Sync + 'static,
    {
        self.cache.key_to_bytes = Some(Box::new(key_to_bytes));

        let n = expected_n.max(1) as u64;
        let filter = Filter::new_with_estimates(n, DEFAULT_BLOOM_FP_RATE)
            .unwrap_or_else(|err| panic!("lru: bloom filter initialization failed: {err}"));

        self.cache.filter = Some(RwLock::new(filter));
        self
    }

    /// Enables sampling-based eviction with a cost function.
    ///
    /// Higher cost = less desirable to evict. `sample_size` entries are sampled
    /// from the LRU tail; the one with the lowest cost is evicted. The cost
    /// function receives `(access_count, size_bytes)`. Mirrors Go's
    /// `WithCostEviction`.
    pub fn with_cost_eviction<F>(&mut self, sample_size: i64, cost_func: F) -> &mut Self
    where
        F: Fn(i64, i64) -> f64 + Send + Sync + 'static,
    {
        self.cache.sample_size = sample_size;
        self.cache.cost_func = Some(Box::new(cost_func));
        self
    }

    /// Sets a function to clone values before insertion.
    ///
    /// Useful to detach values from shared memory arenas. Mirrors Go's
    /// `WithCloneFunc`.
    pub fn with_clone_func<F>(&mut self, clone: F) -> &mut Self
    where
        F: Fn(&V) -> V + Send + Sync + 'static,
    {
        self.cache.clone_func = Some(Box::new(clone));
        self
    }
}

impl<K, V> Cache<K, V>
where
    K: Eq + Hash + Clone,
{
    /// Creates a new LRU cache, configured by the `configure` closure.
    ///
    /// At least one capacity limit ([`Builder::with_max_entries`] or
    /// [`Builder::with_max_bytes`]) must be set inside the closure; otherwise
    /// this panics, exactly as Go's `New` does.
    ///
    /// # Panics
    ///
    /// Panics with `"lru: at least one capacity limit ..."` if neither a count
    /// nor a size limit is configured.
    ///
    /// # Example
    ///
    /// ```
    /// use cf_alg_lru::Cache;
    ///
    /// let cache: Cache<i64, i64> = Cache::new(|c| {
    ///     c.with_max_bytes(1024, |v: &i64| *v);
    /// });
    /// # let _ = cache;
    /// ```
    pub fn new<F>(configure: F) -> Self
    where
        F: FnOnce(&mut Builder<'_, K, V>),
    {
        let mut cache = Self {
            inner: RwLock::new(Inner::new()),
            max_entries: 0,
            max_size: 0,
            filter: None,
            key_to_bytes: None,
            size_func: None,
            clone_func: None,
            cost_func: None,
            sample_size: 0,
            hits: AtomicI64::new(0),
            misses: AtomicI64::new(0),
            bloom_filtered: AtomicI64::new(0),
        };

        {
            let mut builder = Builder { cache: &mut cache };
            configure(&mut builder);
        }

        assert!(
            cache.max_entries > 0 || cache.max_size > 0,
            "lru: at least one capacity limit (WithMaxEntries or WithMaxBytes) is required"
        );

        cache
    }

    /// Returns the number of entries in the cache.
    ///
    /// Mirrors Go's `Len`.
    #[must_use]
    pub fn len(&self) -> usize {
        let inner = self.read_inner();
        inner.index.len()
    }

    /// Returns `true` when the cache holds no entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    // --- internal helpers shared across modules ---

    /// Acquires the main read lock, recovering from a poisoned lock.
    fn read_inner(&self) -> std::sync::RwLockReadGuard<'_, Inner<K, V>> {
        self.inner
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Acquires the main write lock, recovering from a poisoned lock.
    fn write_inner(&self) -> std::sync::RwLockWriteGuard<'_, Inner<K, V>> {
        self.inner
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Records `n` cache hits.
    fn add_hits(&self, n: i64) {
        self.hits.fetch_add(n, Ordering::Relaxed);
    }

    /// Records `n` cache misses.
    fn add_misses(&self, n: i64) {
        self.misses.fetch_add(n, Ordering::Relaxed);
    }

    /// Records `n` Bloom-filtered (short-circuited) lookups.
    fn add_bloom_filtered(&self, n: i64) {
        self.bloom_filtered.fetch_add(n, Ordering::Relaxed);
    }

    // --- accessors used by the `stats` module ---

    /// Returns the hits counter for atomic reads.
    fn hits_atomic(&self) -> &AtomicI64 {
        &self.hits
    }

    /// Returns the misses counter for atomic reads.
    fn misses_atomic(&self) -> &AtomicI64 {
        &self.misses
    }

    /// Returns the Bloom-filtered counter for atomic reads.
    fn bloom_filtered_atomic(&self) -> &AtomicI64 {
        &self.bloom_filtered
    }

    /// Returns the configured maximum entry count (0 when unset).
    fn max_entries_value(&self) -> i64 {
        self.max_entries
    }

    /// Returns the configured maximum size in bytes (0 when unset).
    fn max_size_value(&self) -> i64 {
        self.max_size
    }

    /// Returns a snapshot of `(entry_count, current_size)` under the read lock.
    fn read_inner_for_stats(&self) -> (usize, i64) {
        let inner = self.read_inner();
        (inner.index.len(), inner.cur_size)
    }

    // --- Bloom-filter lock helpers used by the `ops` module ---
    //
    // Explicit return types are required so type inference succeeds at the call
    // sites (the bare `RwLock::read().unwrap_or_else(PoisonError::into_inner)`
    // cannot infer the guard type on its own).

    /// Acquires a read guard on the Bloom filter, recovering from poisoning.
    fn read_filter(filter_lock: &RwLock<Filter>) -> std::sync::RwLockReadGuard<'_, Filter> {
        filter_lock
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Acquires a write guard on the Bloom filter, recovering from poisoning.
    fn write_filter(filter_lock: &RwLock<Filter>) -> std::sync::RwLockWriteGuard<'_, Filter> {
        filter_lock
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}
