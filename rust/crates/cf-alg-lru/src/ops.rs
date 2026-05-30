//! Cache operations: lookups, insertion, batch ops, clearing, and the
//! intrusive-list / eviction internals.
//!
//! This is the Rust port of the Go `ops.go` file. Each public method mirrors
//! its Go counterpart one-to-one; the private helpers reproduce the linked-list
//! and eviction logic over the index-based slab (see [`crate::Inner`]).

use std::collections::HashMap;
use std::hash::Hash;

use crate::{Cache, Entry, Inner, NIL};

impl<K, V> Cache<K, V>
where
    K: Eq + Hash + Clone,
{
    /// Retrieves a value from the cache.
    ///
    /// On a hit the entry is moved to the most-recently-used position and its
    /// access count is incremented. If a Bloom filter is configured, definite
    /// misses are short-circuited without acquiring the main lock. Returns
    /// [`None`] (and records a miss) when the key is absent.
    ///
    /// Mirrors Go's `Get`, which returns `(V, bool)`.
    pub fn get(&self, key: &K) -> Option<V>
    where
        V: Clone,
    {
        if let Some(filter_lock) = &self.filter {
            let key_bytes = self.key_bytes(key);
            let present = {
                let filter = Self::read_filter(filter_lock);
                filter.test(&key_bytes)
            };
            if !present {
                self.add_bloom_filtered(1);
                self.add_misses(1);
                return None;
            }
        }

        let mut inner = self.write_inner();

        let Some(&idx) = inner.index.get(key) else {
            self.add_misses(1);
            return None;
        };

        self.add_hits(1);

        inner.entry_mut(idx).access_count += 1;
        Self::move_to_front(&mut inner, idx);

        Some(inner.entry(idx).value.clone())
    }

    /// Adds or updates a key-value pair in the cache.
    ///
    /// If the value exceeds the maximum cache size it is silently skipped. When
    /// a clone function is configured the value is cloned before insertion.
    ///
    /// Mirrors Go's `Put`.
    pub fn put(&self, key: K, value: V) {
        let val_size = self.value_size(&value);

        // Reject values larger than the entire cache.
        if self.max_size > 0 && val_size > self.max_size {
            return;
        }

        let value = self.maybe_clone(value);

        let mut inner = self.write_inner();
        self.put_locked(&mut inner, key, value, val_size);
    }

    /// Inserts or updates an entry under the write lock.
    ///
    /// Mirrors Go's `putLocked`.
    fn put_locked(&self, inner: &mut Inner<K, V>, key: K, value: V, val_size: i64) {
        // Update existing entry.
        if let Some(&idx) = inner.index.get(&key) {
            let old_size = inner.entry(idx).size;
            inner.cur_size -= old_size;
            {
                let ent = inner.entry_mut(idx);
                ent.value = value;
                ent.size = val_size;
                ent.access_count += 1;
            }
            inner.cur_size += val_size;
            Self::move_to_front(inner, idx);
            return;
        }

        self.evict_until_fits(inner, val_size);

        // If still can't fit after full eviction, skip.
        if self.max_size > 0 && inner.cur_size + val_size > self.max_size {
            return;
        }

        let idx = inner.alloc(Entry {
            key: key.clone(),
            value,
            size: val_size,
            access_count: 1,
            prev: NIL,
            next: NIL,
        });

        inner.index.insert(key.clone(), idx);
        inner.cur_size += val_size;
        Self::add_to_front(inner, idx);

        if let Some(filter_lock) = &self.filter {
            let key_bytes = self.key_bytes(&key);
            let mut filter = Self::write_filter(filter_lock);
            filter.add(&key_bytes);
        }
    }

    /// Retrieves multiple values from the cache.
    ///
    /// Returns a map of found key-value pairs and a vector of missing keys. When
    /// a Bloom filter is configured, definite misses are partitioned out before
    /// the main lock is taken.
    ///
    /// Mirrors Go's `GetMulti`, which returns `(map[K]V, []K)`.
    #[must_use]
    pub fn get_multi(&self, keys: &[K]) -> (HashMap<K, V>, Vec<K>)
    where
        V: Clone,
    {
        let mut found: HashMap<K, V> = HashMap::new();
        let mut missing: Vec<K> = Vec::new();

        // Partition keys using the Bloom filter if available.
        let candidates = self.bloom_partition(keys, &mut missing);

        if candidates.is_empty() {
            return (found, missing);
        }

        let mut inner = self.write_inner();

        for key in candidates {
            if let Some(&idx) = inner.index.get(&key) {
                self.add_hits(1);
                inner.entry_mut(idx).access_count += 1;
                Self::move_to_front(&mut inner, idx);
                let value = inner.entry(idx).value.clone();
                found.insert(key, value);
            } else {
                self.add_misses(1);
                missing.push(key);
            }
        }

        (found, missing)
    }

    /// Adds multiple key-value pairs to the cache using a single lock
    /// acquisition for the entire batch.
    ///
    /// Mirrors Go's `PutMulti`. Iteration order over `items` is unspecified, as
    /// in Go's map range.
    pub fn put_multi(&self, items: HashMap<K, V>) {
        let mut inner = self.write_inner();

        for (key, value) in items {
            let val_size = self.value_size(&value);

            if self.max_size > 0 && val_size > self.max_size {
                continue;
            }

            let value = self.maybe_clone(value);
            self.put_locked(&mut inner, key, value, val_size);
        }
    }

    /// Adds multiple key-value pairs without cloning.
    ///
    /// The caller guarantees the values are exclusively owned and safe to store
    /// directly, avoiding the clone cost when values have already been detached
    /// from shared memory.
    ///
    /// Mirrors Go's `PutMultiOwned`.
    pub fn put_multi_owned(&self, items: HashMap<K, V>) {
        let mut inner = self.write_inner();

        for (key, value) in items {
            let val_size = self.value_size(&value);

            if self.max_size > 0 && val_size > self.max_size {
                continue;
            }

            self.put_locked(&mut inner, key, value, val_size);
        }
    }

    /// Removes all entries and resets the Bloom filter.
    ///
    /// Mirrors Go's `Clear`.
    pub fn clear(&self) {
        let mut inner = self.write_inner();

        inner.slots.clear();
        inner.free.clear();
        inner.index.clear();
        inner.head = NIL;
        inner.tail = NIL;
        inner.cur_size = 0;

        if let Some(filter_lock) = &self.filter {
            let mut filter = Self::write_filter(filter_lock);
            filter.reset();
        }
    }

    // --- private helpers (port of the unexported Go methods) ---

    /// Returns the size of a value using the configured size function, or 1 if
    /// none is configured. Mirrors Go's `valueSize`.
    fn value_size(&self, value: &V) -> i64 {
        match &self.size_func {
            Some(f) => f(value),
            None => 1,
        }
    }

    /// Clones `value` via the configured clone function, or returns it
    /// unchanged. Mirrors the inline `if c.cloneFunc != nil` checks in Go.
    fn maybe_clone(&self, value: V) -> V {
        match &self.clone_func {
            Some(f) => f(&value),
            None => value,
        }
    }

    /// Converts a key to bytes via the configured `key_to_bytes` function.
    ///
    /// Only called when a Bloom filter is configured (which requires the
    /// function), so an absent function is an internal invariant violation.
    fn key_bytes(&self, key: &K) -> Vec<u8> {
        let f = self
            .key_to_bytes
            .as_ref()
            .expect("lru: key_to_bytes missing while a Bloom filter is configured");
        f(key)
    }

    /// Separates keys into Bloom-positive candidates and definite misses.
    /// Without a Bloom filter, all keys are candidates. Mirrors Go's
    /// `bloomPartition` (the `missing` out-parameter becomes a `&mut Vec`).
    fn bloom_partition(&self, keys: &[K], missing: &mut Vec<K>) -> Vec<K> {
        let Some(filter_lock) = &self.filter else {
            return keys.to_vec();
        };

        let mut candidates: Vec<K> = Vec::with_capacity(keys.len());
        let filter = Self::read_filter(filter_lock);

        for key in keys {
            if filter.test(&self.key_bytes(key)) {
                candidates.push(key.clone());
            } else {
                self.add_bloom_filtered(1);
                self.add_misses(1);
                missing.push(key.clone());
            }
        }

        candidates
    }

    /// Removes entries until the new value fits. Mirrors Go's `evictUntilFits`.
    fn evict_until_fits(&self, inner: &mut Inner<K, V>, val_size: i64) {
        // Count-based eviction.
        while self.max_entries > 0
            && inner.index.len() as i64 >= self.max_entries
            && inner.tail != NIL
        {
            self.evict_one(inner);
        }

        // Size-based eviction.
        while self.max_size > 0
            && inner.cur_size + val_size > self.max_size
            && inner.tail != NIL
        {
            self.evict_one(inner);
        }
    }

    /// Removes one entry using cost-based sampling or simple LRU. Mirrors Go's
    /// `evictOne`.
    fn evict_one(&self, inner: &mut Inner<K, V>) {
        if self.cost_func.is_some() && self.sample_size > 0 {
            self.evict_lowest_cost(inner);
            return;
        }

        Self::evict_tail(inner);
    }

    /// Removes the least recently used entry. Mirrors Go's `evictTail`.
    fn evict_tail(inner: &mut Inner<K, V>) {
        if inner.tail == NIL {
            return;
        }

        let victim = inner.tail;
        Self::remove_from_list(inner, victim);
        let entry = inner.dealloc(victim);
        inner.index.remove(&entry.key);
        inner.cur_size -= entry.size;
    }

    /// Samples entries from the tail and evicts the lowest-cost one. Mirrors
    /// Go's `evictLowestCost`.
    fn evict_lowest_cost(&self, inner: &mut Inner<K, V>) {
        if inner.tail == NIL {
            return;
        }

        let cost_func = self
            .cost_func
            .as_ref()
            .expect("lru: cost_func missing while cost eviction is enabled");

        let mut victim = inner.tail;
        let mut lowest_cost = {
            let ent = inner.entry(victim);
            cost_func(ent.access_count, ent.size)
        };

        let mut count: i64 = 1;
        let mut cur = inner.entry(victim).prev;

        while cur != NIL && count < self.sample_size {
            let cost = {
                let ent = inner.entry(cur);
                cost_func(ent.access_count, ent.size)
            };
            if cost < lowest_cost {
                lowest_cost = cost;
                victim = cur;
            }

            count += 1;
            cur = inner.entry(cur).prev;
        }

        Self::remove_from_list(inner, victim);
        let entry = inner.dealloc(victim);
        inner.index.remove(&entry.key);
        inner.cur_size -= entry.size;
    }

    /// Moves an entry to the head of the LRU list. Mirrors Go's `moveToFront`.
    fn move_to_front(inner: &mut Inner<K, V>, idx: usize) {
        if idx == inner.head {
            return;
        }

        Self::remove_from_list(inner, idx);
        Self::add_to_front(inner, idx);
    }

    /// Adds an entry at the head of the LRU list. Mirrors Go's `addToFront`.
    fn add_to_front(inner: &mut Inner<K, V>, idx: usize) {
        let old_head = inner.head;

        {
            let ent = inner.entry_mut(idx);
            ent.prev = NIL;
            ent.next = old_head;
        }

        if old_head != NIL {
            inner.entry_mut(old_head).prev = idx;
        }

        inner.head = idx;

        if inner.tail == NIL {
            inner.tail = idx;
        }
    }

    /// Removes an entry from the LRU list. Mirrors Go's `removeFromList`.
    fn remove_from_list(inner: &mut Inner<K, V>, idx: usize) {
        let (prev, next) = {
            let ent = inner.entry(idx);
            (ent.prev, ent.next)
        };

        if prev != NIL {
            inner.entry_mut(prev).next = next;
        } else {
            inner.head = next;
        }

        if next != NIL {
            inner.entry_mut(next).prev = prev;
        } else {
            inner.tail = prev;
        }
    }
}
