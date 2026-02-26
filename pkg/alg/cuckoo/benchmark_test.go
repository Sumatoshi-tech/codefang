package cuckoo

import (
	"testing"
)

// Benchmark constants.
const (
	benchCapacity  = 100000
	benchItemCount = 10000
)

// BenchmarkInsert benchmarks inserting elements into the filter.
func BenchmarkInsert(b *testing.B) {
	f, err := New(benchCapacity)
	if err != nil {
		b.Fatal(err)
	}

	items := make([][]byte, benchItemCount)
	for i := range benchItemCount {
		items[i] = intToBytes(i)
	}

	b.ResetTimer()

	for range b.N {
		f.Reset()

		for _, item := range items {
			f.Insert(item)
		}
	}
}

// BenchmarkLookup benchmarks looking up elements in the filter.
func BenchmarkLookup(b *testing.B) {
	f, err := New(benchCapacity)
	if err != nil {
		b.Fatal(err)
	}

	items := make([][]byte, benchItemCount)
	for i := range benchItemCount {
		items[i] = intToBytes(i)
		f.Insert(items[i])
	}

	b.ResetTimer()

	for range b.N {
		for _, item := range items {
			f.Lookup(item)
		}
	}
}

// BenchmarkDelete benchmarks deleting elements from the filter.
func BenchmarkDelete(b *testing.B) {
	items := make([][]byte, benchItemCount)
	for i := range benchItemCount {
		items[i] = intToBytes(i)
	}

	b.ResetTimer()

	for range b.N {
		b.StopTimer()

		f, err := New(benchCapacity)
		if err != nil {
			b.Fatal(err)
		}

		for _, item := range items {
			f.Insert(item)
		}

		b.StartTimer()

		for _, item := range items {
			f.Delete(item)
		}
	}
}

// BenchmarkMemory benchmarks memory usage for 100K elements.
func BenchmarkMemory(b *testing.B) {
	for range b.N {
		f, err := New(benchCapacity)
		if err != nil {
			b.Fatal(err)
		}

		for i := range benchCapacity {
			f.Insert(intToBytes(i))
		}

		_ = f.Count()
	}
}
