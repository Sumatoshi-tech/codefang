//! Integration tests for the LRU cache: lookups, eviction policies, Bloom
//! pre-filtering, batch operations, stats, and concurrency.

use std::collections::HashMap;
use std::sync::Arc;
use std::thread;

use cf_alg_lru::{Cache, Stats};

// --- test constants ---

/// Default max entries for count-based tests.
const TEST_MAX_ENTRIES: i64 = 100;
/// Limits the cache to 3 entries for eviction tests.
const SMALL_MAX_ENTRIES: i64 = 3;
/// Expected element count for Bloom filter tests.
const TEST_BLOOM_EXPECTED_N: i64 = 1000;
/// Number of items to insert for Bloom filter tests.
const TEST_BLOOM_INSERT_COUNT: i64 = 100;
/// Number of absent items to probe.
const TEST_BLOOM_PROBE_COUNT: i64 = 200;
/// Small byte limit for size-based tests.
const TEST_MAX_BYTES: i64 = 100;
/// Number of threads for concurrency tests.
const TEST_CONCURRENT_THREADS: i64 = 50;
/// Number of operations per thread.
const TEST_CONCURRENT_OPS: i64 = 100;
/// Sample size for cost-based eviction tests.
const TEST_EVICTION_SAMPLE_SIZE: i64 = 5;

/// Converts an int key to bytes for Bloom filter tests (big-endian u64).
fn int_to_bytes(k: &i64) -> Vec<u8> {
    (*k as u64).to_be_bytes().to_vec()
}

/// Returns the "size" of an int value for size-based tests.
fn int_value_size(v: &i64) -> i64 {
    *v
}

#[test]
fn test_cache_get_put_count_based() {
    let cache: Cache<i64, String> = Cache::new(|c| {
        c.with_max_entries(TEST_MAX_ENTRIES);
    });

    // Get on empty cache returns None.
    assert!(cache.get(&1).is_none());

    // Put and Get.
    cache.put(1, "hello".to_string());

    assert_eq!(cache.get(&1), Some("hello".to_string()));
}

#[test]
fn test_cache_lru_eviction_count_based() {
    let cache: Cache<i64, String> = Cache::new(|c| {
        c.with_max_entries(SMALL_MAX_ENTRIES);
    });

    cache.put(1, "a".to_string());
    cache.put(2, "b".to_string());
    cache.put(3, "c".to_string());

    // Access key 1 to make it recently used.
    cache.get(&1);

    // Adding key 4 should evict key 2 (LRU).
    cache.put(4, "d".to_string());

    assert!(cache.get(&2).is_none(), "key 2 should be evicted (LRU)");
    assert!(
        cache.get(&1).is_some(),
        "key 1 should still exist (recently accessed)"
    );
    assert!(cache.get(&3).is_some(), "key 3 should still exist");
    assert!(cache.get(&4).is_some(), "key 4 should exist");
}

#[test]
fn test_cache_duplicate_put() {
    let cache: Cache<i64, String> = Cache::new(|c| {
        c.with_max_entries(TEST_MAX_ENTRIES);
    });

    cache.put(1, "first".to_string());
    cache.put(1, "second".to_string());

    assert_eq!(
        cache.get(&1),
        Some("second".to_string()),
        "duplicate Put should update value"
    );
    assert_eq!(cache.len(), 1);
}

#[test]
fn test_cache_clear() {
    let cache: Cache<i64, String> = Cache::new(|c| {
        c.with_max_entries(TEST_MAX_ENTRIES);
    });

    cache.put(1, "a".to_string());
    cache.put(2, "b".to_string());
    assert_eq!(cache.len(), 2);

    cache.clear();

    assert_eq!(cache.len(), 0);
    assert!(cache.get(&1).is_none());
}

#[test]
fn test_cache_stats_count_based() {
    let cache: Cache<i64, String> = Cache::new(|c| {
        c.with_max_entries(TEST_MAX_ENTRIES);
    });

    cache.put(1, "a".to_string());
    cache.get(&1); // Hit.
    cache.get(&2); // Miss.

    let stats = cache.stats();
    assert_eq!(stats.hits, 1);
    assert_eq!(stats.misses, 1);
    assert_eq!(stats.entries, 1);
    assert_eq!(stats.max_entries, TEST_MAX_ENTRIES);
    assert!((stats.hit_rate() - 0.5).abs() < 0.001);
}

#[test]
fn test_stats_hit_rate_empty() {
    let stats = Stats::default();
    assert!((stats.hit_rate() - 0.0).abs() < 0.001);
}

#[test]
fn test_cache_cache_hits_misses() {
    let cache: Cache<i64, String> = Cache::new(|c| {
        c.with_max_entries(TEST_MAX_ENTRIES);
    });

    cache.put(1, "a".to_string());
    cache.get(&1);
    cache.get(&2);

    assert_eq!(cache.cache_hits(), 1);
    assert_eq!(cache.cache_misses(), 1);
}

#[test]
fn test_cache_size_based() {
    // Size = value itself. Max 100 bytes.
    let cache: Cache<i64, i64> = Cache::new(|c| {
        c.with_max_bytes(TEST_MAX_BYTES, int_value_size);
    });

    cache.put(1, 40);
    cache.put(2, 40);

    // Both should fit (80 < 100).
    assert!(cache.get(&1).is_some());
    assert!(cache.get(&2).is_some());

    // Access key 2 to make key 1 LRU.
    cache.get(&2);

    // Adding value=40 would exceed 100, so key 1 is evicted.
    cache.put(3, 40);

    assert!(cache.get(&1).is_none(), "key 1 should be evicted (size limit)");
    assert!(cache.get(&2).is_some(), "key 2 should still exist");

    let stats = cache.stats();
    assert_eq!(stats.max_size, TEST_MAX_BYTES);
}

#[test]
fn test_cache_size_based_reject_oversized() {
    let cache: Cache<i64, i64> = Cache::new(|c| {
        c.with_max_bytes(TEST_MAX_BYTES, int_value_size);
    });

    // Value larger than entire cache should be rejected.
    cache.put(1, 200);

    assert!(cache.get(&1).is_none(), "oversized value should not be cached");
}

#[test]
fn test_cache_size_based_current_size() {
    let cache: Cache<i64, i64> = Cache::new(|c| {
        c.with_max_bytes(TEST_MAX_BYTES, int_value_size);
    });

    cache.put(1, 30);
    cache.put(2, 20);

    let stats = cache.stats();
    assert_eq!(stats.current_size, 50);

    cache.clear();

    let stats = cache.stats();
    assert_eq!(stats.current_size, 0);
}

#[test]
fn test_cache_bloom_filter() {
    let cache: Cache<i64, String> = Cache::new(|c| {
        c.with_max_entries(TEST_BLOOM_EXPECTED_N);
        c.with_bloom_filter(int_to_bytes, TEST_BLOOM_EXPECTED_N as usize);
    });

    // Insert items.
    for i in 0..TEST_BLOOM_INSERT_COUNT {
        cache.put(i, "val".to_string());
    }

    // Query absent items — Bloom should filter most.
    for i in TEST_BLOOM_INSERT_COUNT..(TEST_BLOOM_INSERT_COUNT + TEST_BLOOM_PROBE_COUNT) {
        assert!(cache.get(&i).is_none());
    }

    let stats = cache.stats();
    assert!(
        stats.bloom_filtered > 0,
        "Bloom filter should short-circuit at least some absent lookups"
    );
}

#[test]
fn test_cache_bloom_filter_no_false_negatives() {
    let cache: Cache<i64, String> = Cache::new(|c| {
        c.with_max_entries(TEST_BLOOM_EXPECTED_N);
        c.with_bloom_filter(int_to_bytes, TEST_BLOOM_EXPECTED_N as usize);
    });

    for i in 0..TEST_BLOOM_INSERT_COUNT {
        cache.put(i, "val".to_string());
    }

    // Every inserted item must be found (no false negatives).
    for i in 0..TEST_BLOOM_INSERT_COUNT {
        assert!(
            cache.get(&i).is_some(),
            "inserted key {i} must be found (no false negatives)"
        );
    }
}

#[test]
fn test_cache_bloom_filter_reset_on_clear() {
    let cache: Cache<i64, String> = Cache::new(|c| {
        c.with_max_entries(TEST_BLOOM_EXPECTED_N);
        c.with_bloom_filter(int_to_bytes, TEST_BLOOM_EXPECTED_N as usize);
    });

    cache.put(1, "val".to_string());

    assert!(cache.get(&1).is_some());

    cache.clear();

    assert!(cache.get(&1).is_none(), "cleared key should not be found");

    let stats = cache.stats();
    assert!(
        stats.bloom_filtered > 0,
        "lookup after clear should be Bloom-filtered"
    );
}

#[test]
fn test_cache_bloom_filter_empty_cache() {
    let cache: Cache<i64, String> = Cache::new(|c| {
        c.with_max_entries(TEST_BLOOM_EXPECTED_N);
        c.with_bloom_filter(int_to_bytes, TEST_BLOOM_EXPECTED_N as usize);
    });

    // Query absent keys on empty cache.
    for i in 0..TEST_BLOOM_PROBE_COUNT {
        cache.get(&i);
    }

    let stats = cache.stats();
    assert_eq!(stats.misses, TEST_BLOOM_PROBE_COUNT);
    assert_eq!(
        stats.bloom_filtered, TEST_BLOOM_PROBE_COUNT,
        "all lookups on empty cache should be Bloom-filtered"
    );
}

#[test]
fn test_cache_cost_eviction() {
    // Cost = accessCount / sizeKB. Lower cost = evicted first.
    // Large, rarely-accessed items should be evicted before small,
    // frequently-accessed ones.
    let cost_fn = |access_count: i64, size_bytes: i64| -> f64 {
        let mut size_kb = size_bytes as f64 / 1024.0;
        if size_kb < 1.0 {
            size_kb = 1.0;
        }
        access_count as f64 / size_kb
    };

    let cache: Cache<i64, i64> = Cache::new(|c| {
        c.with_max_bytes(TEST_MAX_BYTES, int_value_size);
        c.with_cost_eviction(TEST_EVICTION_SAMPLE_SIZE, cost_fn);
    });

    // Insert a small item (size=10) and access it many times.
    cache.put(1, 10);

    for _ in 0..10 {
        cache.get(&1);
    }

    // Insert a large item (size=40).
    cache.put(2, 40);

    // Insert another item that triggers eviction.
    cache.put(3, 40);

    // Key 2 (large, low access) should be evicted before key 1 (small, high
    // access).
    assert!(
        cache.get(&1).is_some(),
        "key 1 (small, frequently accessed) should survive"
    );
    assert!(cache.get(&3).is_some(), "key 3 (just inserted) should survive");
}

#[test]
fn test_cache_clone_func() {
    use std::sync::atomic::{AtomicBool, Ordering};

    let clone_called = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&clone_called);
    let clone_fn = move |v: &Vec<u8>| -> Vec<u8> {
        flag.store(true, Ordering::SeqCst);
        v.clone()
    };

    let cache: Cache<i64, Vec<u8>> = Cache::new(|c| {
        c.with_max_entries(TEST_MAX_ENTRIES);
        c.with_clone_func(clone_fn);
    });

    let mut original = b"hello".to_vec();
    cache.put(1, original.clone());

    assert!(
        clone_called.load(Ordering::SeqCst),
        "clone function should be called on Put"
    );

    assert_eq!(cache.get(&1), Some(b"hello".to_vec()));

    // Modifying original should not affect cached value.
    original[0] = b'X';
    let got2 = cache.get(&1).unwrap();
    assert_eq!(
        got2[0], b'h',
        "cached value should be independent of original"
    );
}

#[test]
fn test_cache_get_multi() {
    let cache: Cache<i64, String> = Cache::new(|c| {
        c.with_max_entries(TEST_MAX_ENTRIES);
    });

    cache.put(1, "a".to_string());
    cache.put(2, "b".to_string());

    let (found, missing) = cache.get_multi(&[1, 2, 3]);

    assert_eq!(found.len(), 2);
    assert_eq!(missing.len(), 1);
    assert_eq!(missing[0], 3);
    assert_eq!(found.get(&1), Some(&"a".to_string()));
    assert_eq!(found.get(&2), Some(&"b".to_string()));
}

#[test]
fn test_cache_get_multi_with_bloom() {
    let cache: Cache<i64, String> = Cache::new(|c| {
        c.with_max_entries(TEST_BLOOM_EXPECTED_N);
        c.with_bloom_filter(int_to_bytes, TEST_BLOOM_EXPECTED_N as usize);
    });

    // Insert only even-numbered keys.
    for i in 0..TEST_BLOOM_INSERT_COUNT {
        cache.put(i * 2, "val".to_string());
    }

    // Build batch with alternating present/absent keys.
    let mut keys: Vec<i64> = Vec::with_capacity((TEST_BLOOM_INSERT_COUNT * 2) as usize);
    for i in 0..TEST_BLOOM_INSERT_COUNT {
        keys.push(i * 2);
        keys.push(i * 2 + 1);
    }

    let (found, missing) = cache.get_multi(&keys);

    assert_eq!(found.len(), TEST_BLOOM_INSERT_COUNT as usize);
    assert_eq!(missing.len(), TEST_BLOOM_INSERT_COUNT as usize);

    let stats = cache.stats();
    assert!(
        stats.bloom_filtered > 0,
        "GetMulti should Bloom-filter absent keys"
    );
}

#[test]
fn test_cache_put_multi() {
    let cache: Cache<i64, String> = Cache::new(|c| {
        c.with_max_entries(TEST_MAX_ENTRIES);
    });

    let mut items: HashMap<i64, String> = HashMap::new();
    items.insert(1, "a".to_string());
    items.insert(2, "b".to_string());
    items.insert(3, "c".to_string());

    cache.put_multi(items.clone());

    assert_eq!(cache.len(), 3);

    for (k, want) in &items {
        assert_eq!(cache.get(k), Some(want.clone()));
    }
}

#[test]
fn test_cache_concurrent_access() {
    let cache: Arc<Cache<i64, String>> = Arc::new(Cache::new(|c| {
        c.with_max_entries(TEST_MAX_ENTRIES);
    }));

    let mut handles = Vec::with_capacity(TEST_CONCURRENT_THREADS as usize);

    for g in 0..TEST_CONCURRENT_THREADS {
        let cache = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for i in 0..TEST_CONCURRENT_OPS {
                let key = (g * TEST_CONCURRENT_OPS + i) % TEST_MAX_ENTRIES;
                cache.put(key, "data".to_string());
                cache.get(&key);
            }
        }));
    }

    for h in handles {
        h.join().expect("worker thread panicked");
    }

    let stats = cache.stats();
    assert!(stats.entries > 0);
}

#[test]
#[should_panic(expected = "at least one capacity limit")]
fn test_cache_no_panic_on_empty_options() {
    // Passing no capacity option should panic.
    let _cache: Cache<i64, String> = Cache::new(|_c| {});
}

#[test]
fn test_cache_both_limits() {
    // Both count and size limits. Whichever is hit first wins.
    let cache: Cache<i64, i64> = Cache::new(|c| {
        c.with_max_entries(10);
        c.with_max_bytes(TEST_MAX_BYTES, int_value_size);
    });

    // Insert items of size 30 each. After 3 items (90 bytes), the 4th exceeds
    // 100.
    cache.put(1, 30);
    cache.put(2, 30);
    cache.put(3, 30);
    cache.put(4, 30);

    // Key 1 should be evicted (size limit reached before count limit).
    assert!(
        cache.get(&1).is_none(),
        "key 1 should be evicted due to size limit"
    );

    assert!(cache.len() <= 10);
}

#[test]
fn test_cache_len() {
    let cache: Cache<i64, String> = Cache::new(|c| {
        c.with_max_entries(TEST_MAX_ENTRIES);
    });

    assert_eq!(cache.len(), 0);

    cache.put(1, "a".to_string());
    assert_eq!(cache.len(), 1);

    cache.put(2, "b".to_string());
    assert_eq!(cache.len(), 2);

    cache.put(1, "updated".to_string());
    assert_eq!(cache.len(), 2, "duplicate Put should not increase Len");
}

#[test]
fn test_cache_put_multi_owned() {
    // Verify put_multi_owned stores values without a
    // configured clone function (matching put_multi behavior for owned values).
    let cache: Cache<i64, String> = Cache::new(|c| {
        c.with_max_entries(TEST_MAX_ENTRIES);
    });

    let mut items: HashMap<i64, String> = HashMap::new();
    items.insert(1, "x".to_string());
    items.insert(2, "y".to_string());

    cache.put_multi_owned(items);

    assert_eq!(cache.len(), 2);
    assert_eq!(cache.get(&1), Some("x".to_string()));
    assert_eq!(cache.get(&2), Some("y".to_string()));
}
