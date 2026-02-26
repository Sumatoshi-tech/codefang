package interval

import (
	"testing"
)

// Benchmark constants.
const (
	benchIntervalCount = 10000
	benchSpacing       = 10
	benchWidth         = 5
	benchQueryLow      = 500
	benchQueryHigh     = 1500
)

// BenchmarkInsert benchmarks inserting intervals.
func BenchmarkInsert(b *testing.B) {
	for range b.N {
		tree := New()

		for i := range benchIntervalCount {
			low := uint32(i * benchSpacing)
			high := low + benchWidth

			tree.Insert(low, high, uint32(i))
		}
	}
}

// BenchmarkQueryOverlap benchmarks overlap queries.
func BenchmarkQueryOverlap(b *testing.B) {
	tree := New()

	for i := range benchIntervalCount {
		low := uint32(i * benchSpacing)
		high := low + benchWidth

		tree.Insert(low, high, uint32(i))
	}

	b.ResetTimer()

	for range b.N {
		tree.QueryOverlap(benchQueryLow, benchQueryHigh)
	}
}

// BenchmarkQueryPoint benchmarks point queries.
func BenchmarkQueryPoint(b *testing.B) {
	tree := New()

	for i := range benchIntervalCount {
		low := uint32(i * benchSpacing)
		high := low + benchWidth

		tree.Insert(low, high, uint32(i))
	}

	b.ResetTimer()

	for range b.N {
		tree.QueryPoint(benchQueryLow)
	}
}

// BenchmarkDelete benchmarks deleting all intervals.
func BenchmarkDelete(b *testing.B) {
	type ivl struct {
		low, high, value uint32
	}

	intervals := make([]ivl, benchIntervalCount)
	for i := range benchIntervalCount {
		intervals[i] = ivl{
			low:   uint32(i * benchSpacing),
			high:  uint32(i*benchSpacing + benchWidth),
			value: uint32(i),
		}
	}

	b.ResetTimer()

	for range b.N {
		b.StopTimer()

		tree := New()
		for _, iv := range intervals {
			tree.Insert(iv.low, iv.high, iv.value)
		}

		b.StartTimer()

		for _, iv := range intervals {
			tree.Delete(iv.low, iv.high, iv.value)
		}
	}
}
