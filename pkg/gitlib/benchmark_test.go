package gitlib

import (
	"testing"
)

// BenchmarkBatchConfig measures config allocation overhead.
func BenchmarkBatchConfig(b *testing.B) {
	for i := 0; i < b.N; i++ {
		_ = DefaultBatchConfig()
	}
}

// BenchmarkHashCreation measures hash creation overhead.
func BenchmarkHashCreation(b *testing.B) {
	for i := 0; i < b.N; i++ {
		_ = Hash{
			byte(i), byte(i + 1), byte(i + 2), byte(i + 3), byte(i + 4),
			byte(i + 5), byte(i + 6), byte(i + 7), byte(i + 8), byte(i + 9),
			byte(i + 10), byte(i + 11), byte(i + 12), byte(i + 13), byte(i + 14),
			byte(i + 15), byte(i + 16), byte(i + 17), byte(i + 18), byte(i + 19),
		}
	}
}

// BenchmarkDiffRequestCreation measures DiffRequest creation overhead.
func BenchmarkDiffRequestCreation(b *testing.B) {
	hash1 := Hash{1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20}
	hash2 := Hash{21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40}

	for i := 0; i < b.N; i++ {
		_ = DiffRequest{
			OldHash: hash1,
			NewHash: hash2,
			HasOld:  true,
			HasNew:  true,
		}
	}
}

// BenchmarkDiffOpSlice measures DiffOp slice allocation.
func BenchmarkDiffOpSlice(b *testing.B) {
	for i := 0; i < b.N; i++ {
		ops := make([]DiffOp, 100)
		for j := range ops {
			ops[j] = DiffOp{Type: DiffOpEqual, LineCount: 1}
		}
	}
}

// BenchmarkBlobBatchCreation measures batch creation overhead.
func BenchmarkBlobBatchCreation(b *testing.B) {
	for i := 0; i < b.N; i++ {
		_ = BlobBatch{
			BatchID: i,
			Blobs:   make([]*CachedBlob, 100),
			Results: make([]BlobResult, 100),
		}
	}
}

// BenchmarkDiffBatchCreation measures diff batch creation overhead.
func BenchmarkDiffBatchCreation(b *testing.B) {
	for i := 0; i < b.N; i++ {
		_ = DiffBatch{
			BatchID:  i,
			Diffs:    make([]DiffResult, 50),
			Requests: make([]DiffRequest, 50),
		}
	}
}
