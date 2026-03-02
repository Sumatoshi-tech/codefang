package minhash

import (
	"fmt"
	"testing"
)

// Benchmark constants.
const (
	// benchNumHashes is the number of hash functions for benchmarks.
	benchNumHashes = 128

	// benchTokenCount is the number of tokens for signature generation benchmarks.
	benchTokenCount = 1000
)

func BenchmarkAdd_128(b *testing.B) {
	sig, err := New(benchNumHashes)
	if err != nil {
		b.Fatal(err)
	}

	token := []byte("benchmark_token")

	b.ReportAllocs()
	b.ResetTimer()

	for range b.N {
		sig.Add(token)
	}
}

func BenchmarkSimilarity_128(b *testing.B) {
	sigA, err := New(benchNumHashes)
	if err != nil {
		b.Fatal(err)
	}

	sigB, err := New(benchNumHashes)
	if err != nil {
		b.Fatal(err)
	}

	for i := range benchTokenCount {
		sigA.Add(fmt.Appendf(nil, "token_%d", i))
		sigB.Add(fmt.Appendf(nil, "token_%d", i+benchTokenCount/2))
	}

	b.ReportAllocs()
	b.ResetTimer()

	for range b.N {
		sim, simErr := sigA.Similarity(sigB)
		_ = sim
		_ = simErr
	}
}

func BenchmarkSignature_1KTokens(b *testing.B) {
	tokens := make([][]byte, benchTokenCount)
	for i := range benchTokenCount {
		tokens[i] = fmt.Appendf(nil, "token_%d", i)
	}

	b.ReportAllocs()
	b.ResetTimer()

	for range b.N {
		sig, newErr := New(benchNumHashes)
		_ = newErr

		for _, tok := range tokens {
			sig.Add(tok)
		}
	}
}

func BenchmarkMerge_128(b *testing.B) {
	sigA, err := New(benchNumHashes)
	if err != nil {
		b.Fatal(err)
	}

	sigB, err := New(benchNumHashes)
	if err != nil {
		b.Fatal(err)
	}

	for i := range benchTokenCount {
		sigA.Add(fmt.Appendf(nil, "tokenA_%d", i))
		sigB.Add(fmt.Appendf(nil, "tokenB_%d", i))
	}

	b.ReportAllocs()
	b.ResetTimer()

	for range b.N {
		clone := sigA.Clone()
		mergeErr := clone.Merge(sigB)
		_ = mergeErr
	}
}

func BenchmarkBytes_128(b *testing.B) {
	sig, err := New(benchNumHashes)
	if err != nil {
		b.Fatal(err)
	}

	for i := range benchTokenCount {
		sig.Add(fmt.Appendf(nil, "token_%d", i))
	}

	b.ReportAllocs()
	b.ResetTimer()

	for range b.N {
		_ = sig.Bytes()
	}
}

func BenchmarkFromBytes_128(b *testing.B) {
	sig, err := New(benchNumHashes)
	if err != nil {
		b.Fatal(err)
	}

	for i := range benchTokenCount {
		sig.Add(fmt.Appendf(nil, "token_%d", i))
	}

	data := sig.Bytes()

	b.ReportAllocs()
	b.ResetTimer()

	for range b.N {
		restored, fromErr := FromBytes(data)
		_ = restored
		_ = fromErr
	}
}
