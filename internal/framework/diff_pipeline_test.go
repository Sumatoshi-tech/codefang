package framework_test

import (
	"context"
	"errors"
	"testing"
	"time"

	"github.com/sergi/go-diff/diffmatchpatch"

	"github.com/Sumatoshi-tech/codefang/internal/framework"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

var errInjectedDiff = errors.New("injected diff error")

func TestDiffPipeline_NewDiffPipeline(t *testing.T) {
	t.Parallel()

	ch := make(chan gitlib.WorkerRequest, 1)

	p := framework.NewDiffPipeline(ch, 5)
	if p == nil {
		t.Fatal("NewDiffPipeline returned nil")
	}

	if p.PoolWorkerChan != ch {
		t.Error("PoolWorkerChan not set")
	}

	if p.BufferSize != 5 {
		t.Errorf("BufferSize = %d, want 5", p.BufferSize)
	}
}

func TestDiffPipeline_NewDiffPipelineZeroBufferSize(t *testing.T) {
	t.Parallel()

	ch := make(chan gitlib.WorkerRequest, 1)

	p := framework.NewDiffPipeline(ch, 0)
	if p == nil {
		t.Fatal("NewDiffPipeline returned nil")
	}

	if p.BufferSize != 1 {
		t.Errorf("BufferSize = %d, want 1 (normalized)", p.BufferSize)
	}
}

// TestDiffPipeline_Process_GoFallbackWhenDiffErrors exercises FileDiffFromGoDiffForTest by
// using a mock worker that returns an error for the C diff, so the pipeline falls back to Go.
func TestDiffPipeline_Process_GoFallbackWhenDiffErrors(t *testing.T) {
	t.Parallel()

	repo := framework.NewTestRepo(t)
	defer repo.Close()

	setupTestRepoWithTwoCommits(repo)

	libRepo, err := gitlib.OpenRepository(repo.Path())
	if err != nil {
		t.Fatalf("OpenRepository: %v", err)
	}
	defer libRepo.Free()

	commits := framework.CollectCommits(t, libRepo, 2)
	if len(commits) < 2 {
		t.Fatalf("got %d commits, want 2", len(commits))
	}

	changes, blobCache := getTreeDiffAndBlobs(t, libRepo, commits)

	mockCh := startMockDiffWorker()
	results := runDiffPipeline(t, commits[0], changes, blobCache, mockCh)

	close(mockCh)

	validateDiffResults(t, results)
}

func setupTestRepoWithTwoCommits(repo *framework.TestRepo) {
	repo.CreateFile("f.txt", "line1\nline2\n")
	repo.Commit("first")
	repo.CreateFile("f.txt", "line1\nline2\nline3\n")
	repo.Commit("second")
}

func getTreeDiffAndBlobs(
	t *testing.T, libRepo *gitlib.Repository, commits []*gitlib.Commit,
) (changes gitlib.Changes, blobCache map[gitlib.Hash]*gitlib.CachedBlob) {
	t.Helper()

	// CollectCommits returns newest first: commits[0]=second, commits[1]=first.
	firstHash := commits[1].Hash()
	secondHash := commits[0].Hash()

	reqCh := make(chan gitlib.WorkerRequest, 4)
	worker := gitlib.NewWorker(libRepo, reqCh)
	worker.Start()

	changes = requestTreeDiff(t, reqCh, firstHash, secondHash)
	blobCache = requestBlobs(t, reqCh, changes)

	close(reqCh)
	worker.Stop()

	return changes, blobCache
}

func requestTreeDiff(t *testing.T, reqCh chan<- gitlib.WorkerRequest, firstHash, secondHash gitlib.Hash) gitlib.Changes {
	t.Helper()

	// First, get the tree for the first commit.
	respCh1 := make(chan gitlib.TreeDiffResponse, 1)
	reqCh <- gitlib.TreeDiffRequest{PreviousTree: nil, CommitHash: firstHash, Response: respCh1}

	resp1 := <-respCh1
	if resp1.Error != nil {
		t.Fatalf("TreeDiff for first commit: %v", resp1.Error)
	}

	firstTree := resp1.CurrentTree
	defer firstTree.Free()

	// Now get the diff between first and second commit.
	respCh := make(chan gitlib.TreeDiffResponse, 1)
	reqCh <- gitlib.TreeDiffRequest{PreviousTree: firstTree, CommitHash: secondHash, Response: respCh}

	resp := <-respCh
	if resp.Error != nil {
		t.Fatalf("TreeDiff: %v", resp.Error)
	}

	if resp.CurrentTree != nil {
		resp.CurrentTree.Free()
	}

	if len(resp.Changes) == 0 {
		t.Fatal("expected non-empty Changes")
	}

	return resp.Changes
}

func requestBlobs(t *testing.T, reqCh chan<- gitlib.WorkerRequest, changes gitlib.Changes) map[gitlib.Hash]*gitlib.CachedBlob {
	t.Helper()

	hashes := collectUniqueHashes(changes)

	blobRespCh := make(chan gitlib.BlobBatchResponse, 1)
	reqCh <- gitlib.BlobBatchRequest{Hashes: hashes, Response: blobRespCh}

	blobResp := <-blobRespCh
	blobCache := make(map[gitlib.Hash]*gitlib.CachedBlob)

	for i, h := range hashes {
		if blobResp.Blobs[i] != nil {
			blobCache[h] = blobResp.Blobs[i]
		}
	}

	return blobCache
}

func collectUniqueHashes(changes gitlib.Changes) []gitlib.Hash {
	seen := make(map[gitlib.Hash]struct{})

	var hashes []gitlib.Hash

	for _, c := range changes {
		if _, ok := seen[c.From.Hash]; !ok {
			seen[c.From.Hash] = struct{}{}
			hashes = append(hashes, c.From.Hash)
		}

		if _, ok := seen[c.To.Hash]; !ok {
			seen[c.To.Hash] = struct{}{}
			hashes = append(hashes, c.To.Hash)
		}
	}

	return hashes
}

// unwrapDiffBatch extracts a DiffBatchRequest from a WorkerRequest,
// unwrapping ContextualRequest if present.
func unwrapDiffBatch(req gitlib.WorkerRequest) (gitlib.DiffBatchRequest, bool) {
	inner := req
	if cr, ok := req.(gitlib.ContextualRequest); ok {
		inner = cr.WorkerRequest
	}

	batch, ok := inner.(gitlib.DiffBatchRequest)

	return batch, ok
}

func startMockDiffWorker() chan gitlib.WorkerRequest {
	mockCh := make(chan gitlib.WorkerRequest, 2)

	go func() {
		for req := range mockCh {
			if r, ok := unwrapDiffBatch(req); ok {
				results := make([]gitlib.DiffResult, len(r.Requests))
				for i := range results {
					results[i].Error = errInjectedDiff
				}

				r.Response <- gitlib.DiffBatchResponse{Results: results}
			}
		}
	}()

	return mockCh
}

func runDiffPipeline(
	t *testing.T,
	commit *gitlib.Commit,
	changes gitlib.Changes,
	blobCache map[gitlib.Hash]*gitlib.CachedBlob,
	mockCh chan gitlib.WorkerRequest,
) []framework.CommitData {
	t.Helper()

	blobData := framework.BlobData{
		Commit:    commit,
		Index:     1,
		Changes:   changes,
		BlobCache: blobCache,
		Error:     nil,
	}

	blobs := make(chan framework.BlobData, 1)
	blobs <- blobData

	close(blobs)

	p := framework.NewDiffPipeline(mockCh, 1)

	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()

	out := p.Process(ctx, blobs)

	results := make([]framework.CommitData, 0, 16)
	for d := range out {
		results = append(results, d)
	}

	return results
}

func validateDiffResults(t *testing.T, results []framework.CommitData) {
	t.Helper()

	if len(results) != 1 {
		t.Fatalf("got %d results, want 1", len(results))
	}

	if results[0].Error != nil {
		t.Fatalf("result error: %v", results[0].Error)
	}

	if len(results[0].FileDiffs) == 0 {
		t.Fatal("expected FileDiffs (Go fallback) when C diff errors")
	}

	for path, fd := range results[0].FileDiffs {
		if fd.OldLinesOfCode == 0 && fd.NewLinesOfCode == 0 {
			t.Errorf("FileDiffs[%q]: zero line counts", path)
		}

		if len(fd.Diffs) == 0 {
			t.Errorf("FileDiffs[%q]: empty Diffs", path)
		}
	}
}

// TestDiffPipeline_fileDiffFromGoDiff_sameContent covers the equal-content branch.
func TestDiffPipeline_fileDiffFromGoDiff_sameContent(t *testing.T) {
	t.Parallel()

	// Use a real repo to obtain *CachedBlob (we need two blobs with same content).
	repo := framework.NewTestRepo(t)
	defer repo.Close()

	repo.CreateFile("a.txt", "same\ncontent\n")
	repo.Commit("only")

	libRepo, err := gitlib.OpenRepository(repo.Path())
	if err != nil {
		t.Fatalf("OpenRepository: %v", err)
	}
	defer libRepo.Free()

	commits := framework.CollectCommits(t, libRepo, 1)
	if len(commits) == 0 {
		t.Fatal("no commits")
	}

	respCh := make(chan gitlib.TreeDiffResponse, 1)
	reqCh := make(chan gitlib.WorkerRequest, 2)
	worker := gitlib.NewWorker(libRepo, reqCh)
	worker.Start()

	reqCh <- gitlib.TreeDiffRequest{
		PreviousTree: nil,
		CommitHash:   commits[0].Hash(),
		Response:     respCh,
	}

	resp := <-respCh
	if resp.Error != nil || len(resp.Changes) == 0 {
		t.Fatalf("TreeDiff: %v or no changes", resp.Error)
	}

	blobRespCh := make(chan gitlib.BlobBatchResponse, 1)
	reqCh <- gitlib.BlobBatchRequest{
		Hashes:   []gitlib.Hash{resp.Changes[0].To.Hash},
		Response: blobRespCh,
	}

	blobResp := <-blobRespCh

	close(reqCh)
	worker.Stop()

	if len(blobResp.Blobs) == 0 || blobResp.Blobs[0] == nil {
		t.Fatal("no blob")
	}

	blob := blobResp.Blobs[0]

	p := framework.NewDiffPipeline(make(chan gitlib.WorkerRequest), 1)
	// Same blob for old and new -> equal content path.
	fd := framework.FileDiffFromGoDiffForTest(p, blob, blob, 2, 2)
	if fd.OldLinesOfCode != 2 || fd.NewLinesOfCode != 2 {
		t.Errorf("line counts: old=%d new=%d", fd.OldLinesOfCode, fd.NewLinesOfCode)
	}

	if len(fd.Diffs) != 1 || fd.Diffs[0].Type != diffmatchpatch.DiffEqual {
		t.Errorf("expected single DiffEqual, got %d diffs", len(fd.Diffs))
	}
}
