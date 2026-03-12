package cache_test

import (
	"testing"

	"github.com/Sumatoshi-tech/codefang/internal/cache"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

const (
	// benchCacheSize is the cache size for benchmarks (1 MB).
	benchCacheSize = 1024 * 1024

	// benchPreloadCount is the number of items to preload for benchmarks.
	benchPreloadCount = 10_000

	// benchGetMultiBatchSize is the batch size for GetMulti benchmarks.
	benchGetMultiBatchSize = 100

	// benchMissRatio80 is the fraction of lookups that target absent keys (80%).
	benchMissRatio80 = 80

	// benchPercentDivisor converts a percentage to a threshold for modular comparison.
	benchPercentDivisor = 100

	// benchBlobSize is the blob data size used in benchmarks (64 bytes).
	benchBlobSize = 64
)

// benchBlob returns a reusable test blob for benchmarks.
func benchBlob() *gitlib.CachedBlob {
	return gitlib.NewCachedBlobForTest(make([]byte, benchBlobSize))
}

// preloadLRU inserts benchPreloadCount items into the cache.
func preloadLRU(b *testing.B, lru *cache.LRUBlobCache) {
	b.Helper()

	blob := benchBlob()

	for i := range benchPreloadCount {
		hash := makeTestHashU16(uint16(i))
		lru.Put(hash, blob)
	}
}

// BenchmarkLRUGet_MissHeavy benchmarks Get with 80% miss ratio.
// Bloom pre-filter short-circuits most misses without lock acquisition.
func BenchmarkLRUGet_MissHeavy(b *testing.B) {
	lru := cache.NewLRUBlobCache(benchCacheSize)
	preloadLRU(b, lru)

	b.ResetTimer()

	for i := range b.N {
		idx := uint16(i % benchPreloadCount)

		// 80% of lookups target absent keys (offset beyond preloaded range).
		if i%benchPercentDivisor < benchMissRatio80 {
			idx += benchPreloadCount
		}

		lru.Get(makeTestHashU16(idx))
	}
}

// BenchmarkLRUGet_HitHeavy benchmarks Get with 100% hit ratio.
// Measures Bloom filter overhead when all lookups are hits.
func BenchmarkLRUGet_HitHeavy(b *testing.B) {
	lru := cache.NewLRUBlobCache(benchCacheSize)
	preloadLRU(b, lru)

	b.ResetTimer()

	for i := range b.N {
		idx := uint16(i % benchPreloadCount)
		lru.Get(makeTestHashU16(idx))
	}
}

// BenchmarkLRUGetMulti_MissHeavy benchmarks GetMulti with 80% miss ratio.
// Bloom pre-filter partitions the batch, reducing lock-protected map lookups.
func BenchmarkLRUGetMulti_MissHeavy(b *testing.B) {
	lru := cache.NewLRUBlobCache(benchCacheSize)
	preloadLRU(b, lru)

	// Build a batch: 20% present, 80% absent.
	batch := make([]gitlib.Hash, benchGetMultiBatchSize)

	for i := range benchGetMultiBatchSize {
		idx := uint16(i)
		if i%benchPercentDivisor < benchMissRatio80 {
			idx += benchPreloadCount
		}

		batch[i] = makeTestHashU16(idx)
	}

	b.ResetTimer()

	for range b.N {
		lru.GetMulti(batch)
	}
}

// BenchmarkLRUPut benchmarks Put throughput with Bloom filter addition.
func BenchmarkLRUPut(b *testing.B) {
	lru := cache.NewLRUBlobCache(benchCacheSize)
	blob := benchBlob()

	b.ResetTimer()

	for i := range b.N {
		lru.Put(makeTestHashU16(uint16(i%benchPreloadCount)), blob)
	}
}
