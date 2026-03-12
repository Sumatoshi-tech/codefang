package lru_test

// FRD: specs/frds/FRD-20260302-generic-lru-cache.md.

import (
	"testing"

	"github.com/Sumatoshi-tech/codefang/pkg/alg/lru"
)

const (
	// benchMaxEntries is the cache capacity for benchmarks.
	benchMaxEntries = 10_000

	// benchPreloadCount is the number of items to preload.
	benchPreloadCount = 10_000

	// benchGetMultiBatchSize is the batch size for GetMulti benchmarks.
	benchGetMultiBatchSize = 100

	// benchMissRatio80 is the percentage of lookups targeting absent keys.
	benchMissRatio80 = 80

	// benchPercentDivisor converts percentage to modular comparison threshold.
	benchPercentDivisor = 100
)

// preload inserts benchPreloadCount items into the cache.
func preload(b *testing.B, cache *lru.Cache[int, string]) {
	b.Helper()

	for i := range benchPreloadCount {
		cache.Put(i, "val")
	}
}

// BenchmarkGenericGet_MissHeavy benchmarks Get with 80% miss ratio.
func BenchmarkGenericGet_MissHeavy(b *testing.B) {
	cache := lru.New(
		lru.WithMaxEntries[int, string](benchMaxEntries),
		lru.WithBloomFilter[int, string](intToBytes, uint(benchMaxEntries)),
	)
	preload(b, cache)

	b.ResetTimer()

	for i := range b.N {
		idx := i % benchPreloadCount

		// 80% of lookups target absent keys.
		if i%benchPercentDivisor < benchMissRatio80 {
			idx += benchPreloadCount
		}

		cache.Get(idx)
	}
}

// BenchmarkGenericGet_HitHeavy benchmarks Get with 100% hit ratio.
func BenchmarkGenericGet_HitHeavy(b *testing.B) {
	cache := lru.New(
		lru.WithMaxEntries[int, string](benchMaxEntries),
		lru.WithBloomFilter[int, string](intToBytes, uint(benchMaxEntries)),
	)
	preload(b, cache)

	b.ResetTimer()

	for i := range b.N {
		idx := i % benchPreloadCount
		cache.Get(idx)
	}
}

// BenchmarkGenericGetMulti_MissHeavy benchmarks GetMulti with mixed hit/miss.
func BenchmarkGenericGetMulti_MissHeavy(b *testing.B) {
	cache := lru.New(
		lru.WithMaxEntries[int, string](benchMaxEntries),
		lru.WithBloomFilter[int, string](intToBytes, uint(benchMaxEntries)),
	)
	preload(b, cache)

	batch := make([]int, benchGetMultiBatchSize)

	for i := range benchGetMultiBatchSize {
		idx := i
		if i%benchPercentDivisor < benchMissRatio80 {
			idx += benchPreloadCount
		}

		batch[i] = idx
	}

	b.ResetTimer()

	for range b.N {
		cache.GetMulti(batch)
	}
}

// BenchmarkGenericPut benchmarks Put throughput.
func BenchmarkGenericPut(b *testing.B) {
	cache := lru.New(lru.WithMaxEntries[int, string](benchMaxEntries))

	b.ResetTimer()

	for i := range b.N {
		cache.Put(i%benchPreloadCount, "val")
	}
}
