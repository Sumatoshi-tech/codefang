package cms_test

import (
	"testing"

	"github.com/Sumatoshi-tech/codefang/pkg/alg/cms"
)

const (
	benchEpsilon  = 0.001
	benchDelta    = 0.001
	benchPreloadN = 100_000
)

func newBenchSketch(b *testing.B) *cms.Sketch {
	b.Helper()

	sk, err := cms.New(benchEpsilon, benchDelta)
	if err != nil {
		b.Fatal(err)
	}

	return sk
}

func preloadSketch(b *testing.B, sk *cms.Sketch, count int) {
	b.Helper()

	for i := range count {
		sk.Add(uint64ToBytes(uint64(i)), 1)
	}
}

// BenchmarkCMSAdd measures single-element insertion throughput.
func BenchmarkCMSAdd(b *testing.B) {
	sk := newBenchSketch(b)

	b.ResetTimer()

	for i := range b.N {
		sk.Add(uint64ToBytes(uint64(i)), 1)
	}
}

// BenchmarkCMSCount measures single-element count throughput on a populated sketch.
func BenchmarkCMSCount(b *testing.B) {
	sk := newBenchSketch(b)
	preloadSketch(b, sk, benchPreloadN)

	b.ResetTimer()

	for i := range b.N {
		sk.Count(uint64ToBytes(uint64(i % benchPreloadN)))
	}
}

// BenchmarkMapFreq is the comparison baseline using map[string]int64 frequency counting.
func BenchmarkMapFreq(b *testing.B) {
	m := make(map[string]int64, benchPreloadN)

	b.ResetTimer()

	for i := range b.N {
		m[string(uint64ToBytes(uint64(i)))]++
	}
}

// BenchmarkMapFreqLookup is the comparison baseline for map frequency lookup.
func BenchmarkMapFreqLookup(b *testing.B) {
	m := make(map[string]int64, benchPreloadN)

	for i := range benchPreloadN {
		m[string(uint64ToBytes(uint64(i)))] = int64(i)
	}

	b.ResetTimer()

	for i := range b.N {
		_ = m[string(uint64ToBytes(uint64(i%benchPreloadN)))]
	}
}

// BenchmarkCMSMemory measures the memory allocation for sketch creation.
func BenchmarkCMSMemory(b *testing.B) {
	b.ReportAllocs()

	for range b.N {
		sk, err := cms.New(benchEpsilon, benchDelta)
		if err != nil {
			b.Fatal(err)
		}

		// Prevent compiler from optimizing away the allocation.
		if sk.Width() == 0 {
			b.Fatal("unexpected zero width")
		}
	}
}
