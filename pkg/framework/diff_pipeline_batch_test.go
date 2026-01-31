package framework_test

import (
	"context"
	"testing"
	"time"

	"github.com/Sumatoshi-tech/codefang/pkg/framework"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

func TestDiffPipeline_CrossCommitBatching(t *testing.T) {
	// Setup channels.
	poolCh := make(chan gitlib.WorkerRequest, 10)

	// Create pipeline with batching (buffer size 10).
	pipeline := framework.NewDiffPipeline(poolCh, 10)

	// Create dummy data.
	hashA := gitlib.Hash{0: 0xA}
	hashB := gitlib.Hash{0: 0xB}
	hashC := gitlib.Hash{0: 0xC}
	hashD := gitlib.Hash{0: 0xD}

	// Blobs must exist in cache for DiffPipeline to pick them up.
	blobA := gitlib.NewCachedBlobWithHashForTest(hashA, []byte("A"))
	blobB := gitlib.NewCachedBlobWithHashForTest(hashB, []byte("B"))
	blobC := gitlib.NewCachedBlobWithHashForTest(hashC, []byte("C"))
	blobD := gitlib.NewCachedBlobWithHashForTest(hashD, []byte("D"))

	cache1 := map[gitlib.Hash]*gitlib.CachedBlob{hashA: blobA, hashB: blobB}
	cache2 := map[gitlib.Hash]*gitlib.CachedBlob{hashC: blobC, hashD: blobD}

	// Define changes.
	changes1 := gitlib.Changes{
		{Action: gitlib.Modify, From: gitlib.ChangeEntry{Hash: hashA, Name: "file1"}, To: gitlib.ChangeEntry{Hash: hashB, Name: "file1"}},
	}
	changes2 := gitlib.Changes{
		{Action: gitlib.Modify, From: gitlib.ChangeEntry{Hash: hashC, Name: "file2"}, To: gitlib.ChangeEntry{Hash: hashD, Name: "file2"}},
	}

	// Input blobs.
	blobData1 := framework.BlobData{
		Commit:    gitlib.NewCommitForTest(gitlib.Hash{0: 1}),
		Changes:   changes1,
		BlobCache: cache1,
	}
	blobData2 := framework.BlobData{
		Commit:    gitlib.NewCommitForTest(gitlib.Hash{0: 2}),
		Changes:   changes2,
		BlobCache: cache2,
	}

	inputCh := make(chan framework.BlobData, 2)

	inputCh <- blobData1

	inputCh <- blobData2

	close(inputCh)

	ctx, cancel := context.WithTimeout(context.Background(), 2*time.Second)
	defer cancel()

	// Start pipeline.
	outCh := pipeline.Process(ctx, inputCh)

	// Mock Pool Worker (Diff Batch).
	go func() {
		for {
			select {
			case req := <-poolCh:
				diffReq, ok := req.(gitlib.DiffBatchRequest)
				if !ok {
					continue
				}

				t.Logf("Received diff batch size: %d", len(diffReq.Requests))

				// Send response.
				results := make([]gitlib.DiffResult, len(diffReq.Requests))
				for i := range results {
					results[i] = gitlib.DiffResult{
						OldLines: 1,
						NewLines: 1,
						Ops:      []gitlib.DiffOp{{Type: gitlib.DiffOpEqual, LineCount: 1}},
					}
				}

				diffReq.Response <- gitlib.DiffBatchResponse{Results: results}

			case <-ctx.Done():
				return
			}
		}
	}()

	// Consume output.
	count := 0

	for data := range outCh {
		count++

		if len(data.FileDiffs) != 1 {
			t.Errorf("Commit %d: Expected 1 file diff, got %d", count, len(data.FileDiffs))
		}

		// Verify diff data has proper structure.
		for path, fd := range data.FileDiffs {
			if fd.OldLinesOfCode == 0 && fd.NewLinesOfCode == 0 && len(fd.Diffs) == 0 {
				t.Errorf("File %s: FileDiffData is empty", path)
			}
		}
	}

	if count != 2 {
		t.Errorf("Expected 2 output items, got %d", count)
	}
}
