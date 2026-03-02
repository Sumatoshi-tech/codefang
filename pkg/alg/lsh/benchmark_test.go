package lsh

import (
	"fmt"
	"testing"

	"github.com/Sumatoshi-tech/codefang/pkg/alg/minhash"
)

// Benchmark constants.
const (
	// benchBands is the number of bands for benchmarks.
	benchBands = 16

	// benchRows is the number of rows per band for benchmarks.
	benchRows = 8

	// benchNumHashes is the total hash functions (bands * rows).
	benchNumHashes = benchBands * benchRows

	// benchIndexSize is the number of signatures to index for benchmarks.
	benchIndexSize = 1000

	// benchTokensPerSig is the number of tokens per signature.
	benchTokensPerSig = 50
)

func BenchmarkLSHInsert1K(b *testing.B) {
	sigs := make([]*minhash.Signature, benchIndexSize)

	for i := range benchIndexSize {
		sig, err := minhash.New(benchNumHashes)
		if err != nil {
			b.Fatal(err)
		}

		for j := range benchTokensPerSig {
			sig.Add(fmt.Appendf(nil, "sig_%d_tok_%d", i, j))
		}

		sigs[i] = sig
	}

	b.ReportAllocs()
	b.ResetTimer()

	for range b.N {
		idx, idxErr := New(benchBands, benchRows)
		if idxErr != nil {
			b.Fatal(idxErr)
		}

		for i, sig := range sigs {
			insertErr := idx.Insert(fmt.Sprintf("func_%d", i), sig)
			if insertErr != nil {
				b.Fatal(insertErr)
			}
		}
	}
}

func BenchmarkLSHQuery1K(b *testing.B) {
	idx, err := New(benchBands, benchRows)
	if err != nil {
		b.Fatal(err)
	}

	for i := range benchIndexSize {
		sig, sigErr := minhash.New(benchNumHashes)
		if sigErr != nil {
			b.Fatal(sigErr)
		}

		for j := range benchTokensPerSig {
			sig.Add(fmt.Appendf(nil, "sig_%d_tok_%d", i, j))
		}

		insertErr := idx.Insert(fmt.Sprintf("func_%d", i), sig)
		if insertErr != nil {
			b.Fatal(insertErr)
		}
	}

	// Build a query signature similar to func_0.
	querySig, err := minhash.New(benchNumHashes)
	if err != nil {
		b.Fatal(err)
	}

	for j := range benchTokensPerSig {
		querySig.Add(fmt.Appendf(nil, "sig_%d_tok_%d", 0, j))
	}

	b.ReportAllocs()
	b.ResetTimer()

	for range b.N {
		candidates, queryErr := idx.Query(querySig)
		_ = candidates
		_ = queryErr
	}
}

func BenchmarkLSHQueryThreshold1K(b *testing.B) {
	idx, err := New(benchBands, benchRows)
	if err != nil {
		b.Fatal(err)
	}

	for i := range benchIndexSize {
		sig, sigErr := minhash.New(benchNumHashes)
		if sigErr != nil {
			b.Fatal(sigErr)
		}

		for j := range benchTokensPerSig {
			sig.Add(fmt.Appendf(nil, "sig_%d_tok_%d", i, j))
		}

		insertErr := idx.Insert(fmt.Sprintf("func_%d", i), sig)
		if insertErr != nil {
			b.Fatal(insertErr)
		}
	}

	querySig, err := minhash.New(benchNumHashes)
	if err != nil {
		b.Fatal(err)
	}

	for j := range benchTokensPerSig {
		querySig.Add(fmt.Appendf(nil, "sig_%d_tok_%d", 0, j))
	}

	b.ReportAllocs()
	b.ResetTimer()

	for range b.N {
		results, queryErr := idx.QueryThreshold(querySig, 0.5)
		_ = results
		_ = queryErr
	}
}
