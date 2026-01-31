package gitlib_test

import (
	"testing"

	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

// BenchmarkBatchConfig measures config allocation overhead.
func BenchmarkBatchConfig(b *testing.B) {
	for b.Loop() {
		_ = gitlib.DefaultBatchConfig()
	}
}

// BenchmarkHashCreation measures hash creation overhead.
func BenchmarkHashCreation(b *testing.B) {
	i := 0
	for b.Loop() {
		_ = gitlib.Hash{
			byte(i), byte(i + 1), byte(i + 2), byte(i + 3), byte(i + 4),
			byte(i + 5), byte(i + 6), byte(i + 7), byte(i + 8), byte(i + 9),
			byte(i + 10), byte(i + 11), byte(i + 12), byte(i + 13), byte(i + 14),
			byte(i + 15), byte(i + 16), byte(i + 17), byte(i + 18), byte(i + 19),
		}
		i++
	}
}

// BenchmarkDiffRequestCreation measures DiffRequest creation overhead.
func BenchmarkDiffRequestCreation(b *testing.B) {
	hash1 := gitlib.Hash{1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20}
	hash2 := gitlib.Hash{21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40}

	for b.Loop() {
		_ = gitlib.DiffRequest{
			OldHash: hash1,
			NewHash: hash2,
			HasOld:  true,
			HasNew:  true,
		}
	}
}

// BenchmarkDiffOpSlice measures DiffOp slice allocation.
func BenchmarkDiffOpSlice(b *testing.B) {
	for b.Loop() {
		ops := make([]gitlib.DiffOp, 100)
		for j := range ops {
			ops[j] = gitlib.DiffOp{Type: gitlib.DiffOpEqual, LineCount: 1}
		}
	}
}

// BenchmarkBlobBatchCreation measures batch creation overhead.
func BenchmarkBlobBatchCreation(b *testing.B) {
	i := 0
	for b.Loop() {
		_ = gitlib.BlobBatch{
			BatchID: i,
			Blobs:   make([]*gitlib.CachedBlob, 100),
			Results: make([]gitlib.BlobResult, 100),
		}
		i++
	}
}

// BenchmarkDiffBatchCreation measures diff batch creation overhead.
func BenchmarkDiffBatchCreation(b *testing.B) {
	i := 0
	for b.Loop() {
		_ = gitlib.DiffBatch{
			BatchID:  i,
			Diffs:    make([]gitlib.DiffResult, 50),
			Requests: make([]gitlib.DiffRequest, 50),
		}
		i++
	}
}
