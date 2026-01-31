package framework_test

import (
	"context"
	"testing"
	"time"

	"github.com/Sumatoshi-tech/codefang/pkg/framework"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

func TestBlobPipeline_CrossCommitBatching(t *testing.T) {
	// Setup channels.
	seqCh := make(chan gitlib.WorkerRequest, 10)
	poolCh := make(chan gitlib.WorkerRequest, 10)

	pipeline := framework.NewBlobPipeline(seqCh, poolCh, 10, 2)

	// Create dummy commits and hashes.
	commitHash1 := gitlib.Hash{0: 0x1}
	commitHash2 := gitlib.Hash{0: 0x2}

	hashA := gitlib.Hash{0: 0xA}
	hashB := gitlib.Hash{0: 0xB}
	hashC := gitlib.Hash{0: 0xC}
	hashD := gitlib.Hash{0: 0xD}
	hashE := gitlib.Hash{0: 0xE}

	commit1 := gitlib.NewCommitForTest(commitHash1)
	commit2 := gitlib.NewCommitForTest(commitHash2)

	// Prepare input batch.
	batch := framework.CommitBatch{
		Commits:    []*gitlib.Commit{commit1, commit2},
		StartIndex: 0,
	}

	inputCh := make(chan framework.CommitBatch, 1)

	inputCh <- batch

	close(inputCh)

	ctx, cancel := context.WithTimeout(context.Background(), 2*time.Second)
	defer cancel()

	// Start pipeline.
	outCh := pipeline.Process(ctx, inputCh)

	// Mock Sequential Worker (Tree Diff).
	go func() {
		for {
			select {
			case req := <-seqCh:
				treeReq, ok := req.(gitlib.TreeDiffRequest)
				if !ok {
					continue
				}

				// Determine changes based on commit (mock logic).
				var changes gitlib.Changes

				if treeReq.CommitHash == commitHash1 {
					changes = gitlib.Changes{
						{Action: gitlib.Insert, To: gitlib.ChangeEntry{Hash: hashA}},
						{Action: gitlib.Insert, To: gitlib.ChangeEntry{Hash: hashB}},
						{Action: gitlib.Insert, To: gitlib.ChangeEntry{Hash: hashD}},
					}
				} else {
					changes = gitlib.Changes{
						{Action: gitlib.Insert, To: gitlib.ChangeEntry{Hash: hashC}},
						{Action: gitlib.Insert, To: gitlib.ChangeEntry{Hash: hashE}},
						{Action: gitlib.Modify, From: gitlib.ChangeEntry{Hash: hashA}, To: gitlib.ChangeEntry{Hash: hashA}},
					}
				}

				treeReq.Response <- gitlib.TreeDiffResponse{
					Changes:     changes,
					CurrentTree: nil,
					Error:       nil,
				}
			case <-ctx.Done():
				return
			}
		}
	}()

	// Mock Pool Worker (Blob Batch) and verify batching.
	go func() {
		seenHashes := make(map[gitlib.Hash]bool)
		requestCount := 0

		for len(seenHashes) < 5 {
			select {
			case req := <-poolCh:
				batchReq, ok := req.(gitlib.BlobBatchRequest)
				if !ok {
					continue
				}

				requestCount++

				if len(batchReq.Hashes) == 5 {
					t.Logf("Expected sharded batch, got one big batch of 5. Hashes: %v", batchReq.Hashes)
				}

				for _, h := range batchReq.Hashes {
					seenHashes[h] = true
				}

				// Send response.
				blobs := make([]*gitlib.CachedBlob, len(batchReq.Hashes))
				for i, h := range batchReq.Hashes {
					blobs[i] = gitlib.NewCachedBlobWithHashForTest(h, []byte("data"))
				}

				batchReq.Response <- gitlib.BlobBatchResponse{
					Blobs:   blobs,
					Results: nil,
				}

			case <-ctx.Done():
				return
			}
		}

		if requestCount < 2 {
			t.Errorf("Expected at least 2 sharded requests, got %d", requestCount)
		}
	}()

	// Wait for output.
	count := 0

	for range outCh {
		count++
	}

	if count != 2 {
		t.Errorf("Expected 2 output items, got %d", count)
	}
}
