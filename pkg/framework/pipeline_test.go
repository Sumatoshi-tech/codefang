package framework

import (
	"context"
	"testing"

	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

func TestCommitStreamer(t *testing.T) {
	streamer := NewCommitStreamer()

	if streamer.BatchSize != 10 {
		t.Errorf("BatchSize = %d, want 10", streamer.BatchSize)
	}
	if streamer.Lookahead != 2 {
		t.Errorf("Lookahead = %d, want 2", streamer.Lookahead)
	}
}

func TestCommitStreamerStreamEmpty(t *testing.T) {
	streamer := NewCommitStreamer()
	ctx := context.Background()

	// Stream empty commits slice
	ch := streamer.Stream(ctx, []*gitlib.Commit{})

	// Should get no batches
	count := 0
	for range ch {
		count++
	}

	if count != 0 {
		t.Errorf("Expected 0 batches, got %d", count)
	}
}

func TestDefaultCoordinatorConfig(t *testing.T) {
	config := DefaultCoordinatorConfig()

	if config.CommitBatchSize != 1 {
		t.Errorf("CommitBatchSize = %d, want 1", config.CommitBatchSize)
	}
	if config.Workers != 1 {
		t.Errorf("Workers = %d, want 1", config.Workers)
	}
	if config.BufferSize != 10 {
		t.Errorf("BufferSize = %d, want 10", config.BufferSize)
	}
	if config.BatchConfig.BlobBatchSize != 100 {
		t.Errorf("BlobBatchSize = %d, want 100", config.BatchConfig.BlobBatchSize)
	}
	if config.BatchConfig.DiffBatchSize != 50 {
		t.Errorf("DiffBatchSize = %d, want 50", config.BatchConfig.DiffBatchSize)
	}
}

func TestCommitBatch(t *testing.T) {
	batch := CommitBatch{
		Commits:    nil,
		StartIndex: 10,
		BatchID:    5,
	}

	if batch.StartIndex != 10 {
		t.Errorf("StartIndex = %d, want 10", batch.StartIndex)
	}
	if batch.BatchID != 5 {
		t.Errorf("BatchID = %d, want 5", batch.BatchID)
	}
}

func TestBlobData(t *testing.T) {
	data := BlobData{
		Index:     42,
		BlobCache: make(map[gitlib.Hash]*gitlib.CachedBlob),
	}

	if data.Index != 42 {
		t.Errorf("Index = %d, want 42", data.Index)
	}
	if data.BlobCache == nil {
		t.Error("BlobCache should not be nil")
	}
}

func TestCommitData(t *testing.T) {
	data := CommitData{
		Index:     42,
		BlobCache: make(map[gitlib.Hash]*gitlib.CachedBlob),
		FileDiffs: nil,
	}

	if data.Index != 42 {
		t.Errorf("Index = %d, want 42", data.Index)
	}
	if data.BlobCache == nil {
		t.Error("BlobCache should not be nil")
	}
}
