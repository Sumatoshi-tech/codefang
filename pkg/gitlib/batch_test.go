package gitlib

import (
	"testing"
)

func TestDefaultBatchConfig(t *testing.T) {
	config := DefaultBatchConfig()

	if config.BlobBatchSize != 100 {
		t.Errorf("BlobBatchSize = %d, want 100", config.BlobBatchSize)
	}

	if config.DiffBatchSize != 50 {
		t.Errorf("DiffBatchSize = %d, want 50", config.DiffBatchSize)
	}

	if config.Workers != 1 {
		t.Errorf("Workers = %d, want 1", config.Workers)
	}
}

func TestBlobBatch(t *testing.T) {
	batch := BlobBatch{
		BatchID: 42,
		Blobs:   []*CachedBlob{},
	}

	if batch.BatchID != 42 {
		t.Errorf("BatchID = %d, want 42", batch.BatchID)
	}
}

func TestDiffBatch(t *testing.T) {
	batch := DiffBatch{
		BatchID:  42,
		Diffs:    []DiffResult{},
		Requests: []DiffRequest{},
	}

	if batch.BatchID != 42 {
		t.Errorf("BatchID = %d, want 42", batch.BatchID)
	}
}

func TestDiffRequest(t *testing.T) {
	hash1 := Hash{1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20}
	hash2 := Hash{21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40}

	req := DiffRequest{
		OldHash: hash1,
		NewHash: hash2,
		HasOld:  true,
		HasNew:  true,
	}

	if req.OldHash != hash1 {
		t.Error("OldHash mismatch")
	}
	if req.NewHash != hash2 {
		t.Error("NewHash mismatch")
	}
	if !req.HasOld {
		t.Error("HasOld should be true")
	}
	if !req.HasNew {
		t.Error("HasNew should be true")
	}
}
