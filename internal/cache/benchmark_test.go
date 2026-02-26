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

	// benchBloomSetElements is the number of elements for BloomHashSet benchmarks.
	benchBloomSetElements = 100_000

	// benchBloomSetFPRate is the false-positive rate for BloomHashSet benchmarks.
	benchBloomSetFPRate = 0.01
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

// benchHashU32 creates a Hash from a uint32, spreading bytes across the first 4 positions.
func benchHashU32(val uint32) gitlib.Hash {
	var h gitlib.Hash

	h[0] = byte(val >> 24)
	h[1] = byte(val >> 16)
	h[2] = byte(val >> 8)
	h[3] = byte(val)

	return h
}

// BenchmarkBloomHashSet_Add benchmarks BloomHashSet Add throughput.
func BenchmarkBloomHashSet_Add(b *testing.B) {
	bs, err := cache.NewBloomHashSet(benchBloomSetElements, benchBloomSetFPRate)
	if err != nil {
		b.Fatal(err)
	}

	b.ResetTimer()

	for i := range b.N {
		bs.Add(benchHashU32(uint32(i)))
	}
}

// BenchmarkBloomHashSet_Contains benchmarks BloomHashSet Contains throughput.
func BenchmarkBloomHashSet_Contains(b *testing.B) {
	bs, err := cache.NewBloomHashSet(benchBloomSetElements, benchBloomSetFPRate)
	if err != nil {
		b.Fatal(err)
	}

	// Preload elements.
	for i := range benchBloomSetElements {
		bs.Add(benchHashU32(uint32(i)))
	}

	b.ResetTimer()

	for i := range b.N {
		bs.Contains(benchHashU32(uint32(i % benchBloomSetElements)))
	}
}

// BenchmarkHashSet_Add benchmarks exact HashSet Add throughput for comparison.
func BenchmarkHashSet_Add(b *testing.B) {
	hs := cache.NewHashSet()

	b.ResetTimer()

	for i := range b.N {
		hs.Add(benchHashU32(uint32(i)))
	}
}

// BenchmarkHashSet_Contains benchmarks exact HashSet Contains throughput for comparison.
func BenchmarkHashSet_Contains(b *testing.B) {
	hs := cache.NewHashSet()

	// Preload elements.
	for i := range benchBloomSetElements {
		hs.Add(benchHashU32(uint32(i)))
	}

	b.ResetTimer()

	for i := range b.N {
		hs.Contains(benchHashU32(uint32(i % benchBloomSetElements)))
	}
}

// BenchmarkBloomHashSet_Memory reports BloomHashSet memory for 100K elements.
func BenchmarkBloomHashSet_Memory(b *testing.B) {
	for range b.N {
		bs, err := cache.NewBloomHashSet(benchBloomSetElements, benchBloomSetFPRate)
		if err != nil {
			b.Fatal(err)
		}

		for i := range benchBloomSetElements {
			bs.Add(benchHashU32(uint32(i)))
		}
	}
}
