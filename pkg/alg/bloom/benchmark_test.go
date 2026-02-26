package bloom_test

import (
	"testing"

	"github.com/Sumatoshi-tech/codefang/pkg/alg/bloom"
)

const (
	benchN        = uint(1_000_000)
	benchFP       = 0.01
	benchBulkSize = 100
	benchMemN     = uint(10_000_000)
	benchLookupN  = 100_000
)

func newBenchFilter(b *testing.B) *bloom.Filter {
	b.Helper()

	f, err := bloom.NewWithEstimates(benchN, benchFP)
	if err != nil {
		b.Fatal(err)
	}

	return f
}

func preloadFilter(b *testing.B, f *bloom.Filter, count int) {
	b.Helper()

	for i := range count {
		f.Add(uint64ToBytes(uint64(i)))
	}
}

// BenchmarkBloomAdd measures single-element insertion throughput.
func BenchmarkBloomAdd(b *testing.B) {
	f := newBenchFilter(b)

	b.ResetTimer()

	for i := range b.N {
		f.Add(uint64ToBytes(uint64(i)))
	}
}

// BenchmarkBloomTest measures single-element lookup throughput on a populated filter.
func BenchmarkBloomTest(b *testing.B) {
	f := newBenchFilter(b)
	preloadFilter(b, f, benchLookupN)

	b.ResetTimer()

	for i := range b.N {
		f.Test(uint64ToBytes(uint64(i % benchLookupN)))
	}
}

// BenchmarkBloomTestMiss measures lookup throughput when elements are absent.
func BenchmarkBloomTestMiss(b *testing.B) {
	f := newBenchFilter(b)
	preloadFilter(b, f, benchLookupN)

	// Query keys that were never inserted (offset past inserted range).
	offset := uint64(benchLookupN * 10)

	b.ResetTimer()

	for i := range b.N {
		f.Test(uint64ToBytes(offset + uint64(i)))
	}
}

// BenchmarkBloomTestAndAdd measures atomic test-and-add throughput.
func BenchmarkBloomTestAndAdd(b *testing.B) {
	f := newBenchFilter(b)

	b.ResetTimer()

	for i := range b.N {
		f.TestAndAdd(uint64ToBytes(uint64(i)))
	}
}

// BenchmarkBloomAddBulk measures bulk insertion throughput.
func BenchmarkBloomAddBulk(b *testing.B) {
	f := newBenchFilter(b)

	items := make([][]byte, benchBulkSize)
	for i := range items {
		items[i] = uint64ToBytes(uint64(i))
	}

	b.ResetTimer()

	for range b.N {
		f.AddBulk(items)
	}
}

// BenchmarkBloomTestBulk measures bulk lookup throughput.
func BenchmarkBloomTestBulk(b *testing.B) {
	f := newBenchFilter(b)
	preloadFilter(b, f, benchLookupN)

	items := make([][]byte, benchBulkSize)
	for i := range items {
		items[i] = uint64ToBytes(uint64(i))
	}

	b.ResetTimer()

	for range b.N {
		f.TestBulk(items)
	}
}

// BenchmarkMapAdd is the comparison baseline using map[string]bool insertion.
func BenchmarkMapAdd(b *testing.B) {
	m := make(map[string]bool, benchN)

	b.ResetTimer()

	for i := range b.N {
		m[string(uint64ToBytes(uint64(i)))] = true
	}
}

// BenchmarkMapTest is the comparison baseline using map[string]bool lookup.
func BenchmarkMapTest(b *testing.B) {
	m := make(map[string]bool, benchLookupN)

	for i := range benchLookupN {
		m[string(uint64ToBytes(uint64(i)))] = true
	}

	b.ResetTimer()

	for i := range b.N {
		_ = m[string(uint64ToBytes(uint64(i%benchLookupN)))]
	}
}

// BenchmarkBloomMemory10M measures the memory allocation for a 10M-element filter.
func BenchmarkBloomMemory10M(b *testing.B) {
	b.ReportAllocs()

	for range b.N {
		f, err := bloom.NewWithEstimates(benchMemN, benchFP)
		if err != nil {
			b.Fatal(err)
		}

		// Prevent compiler from optimizing away the allocation.
		if f.BitCount() == 0 {
			b.Fatal("unexpected zero bit count")
		}
	}
}
