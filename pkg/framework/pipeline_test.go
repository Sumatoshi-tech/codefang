package framework_test

import (
	"context"
	"testing"

	"github.com/Sumatoshi-tech/codefang/pkg/framework"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

func TestCommitStreamer(t *testing.T) {
	streamer := framework.NewCommitStreamer()

	if streamer.BatchSize != 10 {
		t.Errorf("BatchSize = %d, want 10", streamer.BatchSize)
	}

	if streamer.Lookahead != 2 {
		t.Errorf("Lookahead = %d, want 2", streamer.Lookahead)
	}
}

func TestCommitStreamerStreamEmpty(t *testing.T) {
	streamer := framework.NewCommitStreamer()
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
	config := framework.DefaultCoordinatorConfig()

	if config.CommitBatchSize != 100 {
		t.Errorf("CommitBatchSize = %d, want 100", config.CommitBatchSize)
	}

	if config.Workers < 1 {
		t.Errorf("Workers = %d, want >= 1", config.Workers)
	}

	if config.BufferSize < config.Workers {
		t.Errorf("BufferSize = %d, want >= Workers (%d)", config.BufferSize, config.Workers)
	}

	if config.BatchConfig.BlobBatchSize != 100 {
		t.Errorf("BlobBatchSize = %d, want 100", config.BatchConfig.BlobBatchSize)
	}

	if config.BatchConfig.DiffBatchSize != 50 {
		t.Errorf("DiffBatchSize = %d, want 50", config.BatchConfig.DiffBatchSize)
	}
}

func TestCommitBatch(t *testing.T) {
	batch := framework.CommitBatch{
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
	data := framework.BlobData{
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
	data := framework.CommitData{
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
