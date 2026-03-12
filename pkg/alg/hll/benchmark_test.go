package hll_test

import (
	"testing"

	"github.com/Sumatoshi-tech/codefang/pkg/alg/hll"
)

const (
	benchPrecision = uint8(14)
	benchPreloadN  = 100_000
	benchBulkN     = 1000
)

func newBenchSketch(b *testing.B) *hll.Sketch {
	b.Helper()

	sk, err := hll.New(benchPrecision)
	if err != nil {
		b.Fatal(err)
	}

	return sk
}

func preloadSketch(b *testing.B, sk *hll.Sketch, count int) {
	b.Helper()

	for i := range count {
		sk.Add(uint64ToBytes(uint64(i)))
	}
}

// BenchmarkHLLAdd measures single-element insertion throughput.
func BenchmarkHLLAdd(b *testing.B) {
	sk := newBenchSketch(b)

	b.ResetTimer()

	for i := range b.N {
		sk.Add(uint64ToBytes(uint64(i)))
	}
}

// BenchmarkHLLCount measures cardinality estimation throughput on a populated sketch.
func BenchmarkHLLCount(b *testing.B) {
	sk := newBenchSketch(b)
	preloadSketch(b, sk, benchPreloadN)

	b.ResetTimer()

	for range b.N {
		sk.Count()
	}
}

// BenchmarkHLLMerge measures sketch merge throughput.
func BenchmarkHLLMerge(b *testing.B) {
	sk1 := newBenchSketch(b)
	preloadSketch(b, sk1, benchPreloadN)

	sk2 := newBenchSketch(b)
	preloadSketch(b, sk2, benchPreloadN)

	b.ResetTimer()

	for range b.N {
		err := sk1.Merge(sk2)
		if err != nil {
			b.Fatal(err)
		}
	}
}

// BenchmarkMapCard is the comparison baseline using map[string]struct{} cardinality.
func BenchmarkMapCard(b *testing.B) {
	m := make(map[string]struct{}, benchPreloadN)

	for i := range benchPreloadN {
		m[string(uint64ToBytes(uint64(i)))] = struct{}{}
	}

	b.ResetTimer()

	for range b.N {
		_ = len(m)
	}
}

// BenchmarkMapAdd is the comparison baseline using map[string]struct{} insertion.
func BenchmarkMapAdd(b *testing.B) {
	m := make(map[string]struct{}, benchPreloadN)

	b.ResetTimer()

	for i := range b.N {
		m[string(uint64ToBytes(uint64(i)))] = struct{}{}
	}
}

// BenchmarkHLLMemory measures the memory allocation for sketch creation.
func BenchmarkHLLMemory(b *testing.B) {
	b.ReportAllocs()

	for range b.N {
		sk, err := hll.New(benchPrecision)
		if err != nil {
			b.Fatal(err)
		}

		// Prevent compiler from optimizing away the allocation.
		if sk.RegisterCount() == 0 {
			b.Fatal("unexpected zero register count")
		}
	}
}
