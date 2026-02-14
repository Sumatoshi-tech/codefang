package gitlib_test

import (
	"testing"

	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

func TestWorker_TreeDiffInitial(t *testing.T) {
	t.Parallel()

	tr := newTestRepo(t)
	defer tr.cleanup()

	tr.createFile("f.txt", "content")
	firstHash := tr.commit("first")

	repo, err := gitlib.OpenRepository(tr.path)
	require.NoError(t, err)

	defer repo.Free()

	reqCh := make(chan gitlib.WorkerRequest, 2)
	worker := gitlib.NewWorker(repo, reqCh)
	worker.Start()

	respCh := make(chan gitlib.TreeDiffResponse, 1)
	reqCh <- gitlib.TreeDiffRequest{
		PreviousTree: nil,
		CommitHash:   firstHash,
		Response:     respCh,
	}

	resp := <-respCh
	require.NoError(t, resp.Error)
	require.NotEmpty(t, resp.Changes)

	close(reqCh)
	worker.Stop()
}

func TestWorker_TreeDiffTwoCommits(t *testing.T) {
	t.Parallel()

	tr := newTestRepo(t)
	defer tr.cleanup()

	tr.createFile("f.txt", "v1")
	firstHash := tr.commit("first")
	tr.createFile("f.txt", "v2")
	secondHash := tr.commit("second")

	repo, err := gitlib.OpenRepository(tr.path)
	require.NoError(t, err)

	defer repo.Free()

	reqCh := make(chan gitlib.WorkerRequest, 4)
	worker := gitlib.NewWorker(repo, reqCh)
	worker.Start()

	// Initial.
	respCh := make(chan gitlib.TreeDiffResponse, 1)
	reqCh <- gitlib.TreeDiffRequest{PreviousTree: nil, CommitHash: firstHash, Response: respCh}

	resp := <-respCh
	require.NoError(t, resp.Error)

	firstTree := resp.CurrentTree
	defer firstTree.Free()

	// Second commit vs first.
	respCh2 := make(chan gitlib.TreeDiffResponse, 1)
	reqCh <- gitlib.TreeDiffRequest{PreviousTree: firstTree, CommitHash: secondHash, Response: respCh2}

	resp2 := <-respCh2
	require.NoError(t, resp2.Error)
	require.Len(t, resp2.Changes, 1)
	require.Equal(t, gitlib.Modify, resp2.Changes[0].Action)

	close(reqCh)
	worker.Stop()
}

func TestWorker_BlobBatchRequest(t *testing.T) {
	t.Parallel()

	tr := newTestRepo(t)
	defer tr.cleanup()

	tr.createFile("a.txt", "aaa")
	firstHash := tr.commit("first")

	repo, err := gitlib.OpenRepository(tr.path)
	require.NoError(t, err)

	defer repo.Free()

	reqCh := make(chan gitlib.WorkerRequest, 4)
	worker := gitlib.NewWorker(repo, reqCh)
	worker.Start()

	respCh := make(chan gitlib.TreeDiffResponse, 1)
	reqCh <- gitlib.TreeDiffRequest{PreviousTree: nil, CommitHash: firstHash, Response: respCh}

	resp := <-respCh
	require.NoError(t, resp.Error)
	require.NotEmpty(t, resp.Changes)

	if resp.CurrentTree != nil {
		defer resp.CurrentTree.Free()
	}

	hashes := make([]gitlib.Hash, 0, len(resp.Changes))
	for _, c := range resp.Changes {
		hashes = append(hashes, c.To.Hash)
	}

	blobRespCh := make(chan gitlib.BlobBatchResponse, 1)
	reqCh <- gitlib.BlobBatchRequest{Hashes: hashes, Response: blobRespCh}

	blobResp := <-blobRespCh
	require.Len(t, blobResp.Blobs, len(hashes))

	for i, b := range blobResp.Blobs {
		require.NotNil(t, b, "blob %d", i)
		require.Equal(t, hashes[i], b.Hash())
		require.NotNil(t, b.Data)
	}

	close(reqCh)
	worker.Stop()
}

func TestWorker_BlobBatchRequestEmpty(t *testing.T) {
	t.Parallel()

	tr := newTestRepo(t)
	defer tr.cleanup()

	tr.createFile("x.txt", "x")
	tr.commit("only")

	repo, err := gitlib.OpenRepository(tr.path)
	require.NoError(t, err)

	defer repo.Free()

	reqCh := make(chan gitlib.WorkerRequest, 2)
	worker := gitlib.NewWorker(repo, reqCh)
	worker.Start()

	blobRespCh := make(chan gitlib.BlobBatchResponse, 1)
	reqCh <- gitlib.BlobBatchRequest{Hashes: nil, Response: blobRespCh}

	blobResp := <-blobRespCh
	require.NotNil(t, blobResp)
	require.Empty(t, blobResp.Blobs)

	close(reqCh)
	worker.Stop()
}

func TestWorker_TreeDiffRequestInvalidHash(t *testing.T) {
	t.Parallel()

	tr := newTestRepo(t)
	defer tr.cleanup()

	tr.createFile("f.txt", "x")
	tr.commit("first")

	repo, err := gitlib.OpenRepository(tr.path)
	require.NoError(t, err)

	defer repo.Free()

	reqCh := make(chan gitlib.WorkerRequest, 2)
	worker := gitlib.NewWorker(repo, reqCh)
	worker.Start()

	invalidHash := gitlib.NewHash("0000000000000000000000000000000000000000")

	respCh := make(chan gitlib.TreeDiffResponse, 1)
	reqCh <- gitlib.TreeDiffRequest{PreviousTree: nil, CommitHash: invalidHash, Response: respCh}

	resp := <-respCh
	require.Error(t, resp.Error)
	require.Nil(t, resp.Changes)

	close(reqCh)
	worker.Stop()
}

func TestWorker_DiffBatchRequest(t *testing.T) {
	t.Parallel()

	tr := newTestRepo(t)
	defer tr.cleanup()

	tr.createFile("f.txt", "line1\nline2\n")
	firstHash := tr.commit("first")
	tr.createFile("f.txt", "line1\nline2\nline3\n")
	secondHash := tr.commit("second")

	repo, err := gitlib.OpenRepository(tr.path)
	require.NoError(t, err)

	defer repo.Free()

	reqCh := make(chan gitlib.WorkerRequest, 4)
	worker := gitlib.NewWorker(repo, reqCh)
	worker.Start()

	// Get first tree.
	respCh1 := make(chan gitlib.TreeDiffResponse, 1)
	reqCh <- gitlib.TreeDiffRequest{PreviousTree: nil, CommitHash: firstHash, Response: respCh1}

	resp1 := <-respCh1
	require.NoError(t, resp1.Error)

	firstTree := resp1.CurrentTree
	defer firstTree.Free()

	// Get changes to find blob hashes.
	respCh := make(chan gitlib.TreeDiffResponse, 1)
	reqCh <- gitlib.TreeDiffRequest{PreviousTree: firstTree, CommitHash: secondHash, Response: respCh}

	resp := <-respCh
	require.NoError(t, resp.Error)
	require.Len(t, resp.Changes, 1)
	ch := resp.Changes[0]
	require.Equal(t, gitlib.Modify, ch.Action)

	diffReq := gitlib.DiffRequest{OldHash: ch.From.Hash, NewHash: ch.To.Hash, HasOld: true, HasNew: true}

	diffRespCh := make(chan gitlib.DiffBatchResponse, 1)
	reqCh <- gitlib.DiffBatchRequest{Requests: []gitlib.DiffRequest{diffReq}, Response: diffRespCh}

	diffResp := <-diffRespCh
	require.Len(t, diffResp.Results, 1)
	require.NoError(t, diffResp.Results[0].Error)
	require.NotEmpty(t, diffResp.Results[0].Ops)

	close(reqCh)
	worker.Stop()
}

// TestCGOBridge_BatchLoadBlobsInvalidHash triggers C lookup failure and covers cgoBlobError.
func TestCGOBridge_BatchLoadBlobsInvalidHash(t *testing.T) {
	t.Parallel()

	tr := newTestRepo(t)
	defer tr.cleanup()

	tr.createFile("f.txt", "x")
	tr.commit("only")

	repo, err := gitlib.OpenRepository(tr.path)
	require.NoError(t, err)

	defer repo.Free()

	bridge := gitlib.NewCGOBridge(repo)
	results := bridge.BatchLoadBlobs([]gitlib.Hash{gitlib.ZeroHash()})
	require.Len(t, results, 1)
	require.Error(t, results[0].Error)
	require.Equal(t, gitlib.ErrBlobLookup, results[0].Error)
}

// TestCGOBridge_BatchDiffBlobsInvalidHash triggers C diff lookup failure and covers cgoDiffError.
func TestCGOBridge_BatchDiffBlobsInvalidHash(t *testing.T) {
	t.Parallel()

	tr := newTestRepo(t)
	defer tr.cleanup()

	tr.createFile("f.txt", "a")
	tr.commit("only")

	repo, err := gitlib.OpenRepository(tr.path)
	require.NoError(t, err)

	defer repo.Free()

	bridge := gitlib.NewCGOBridge(repo)
	req := gitlib.DiffRequest{
		OldHash: gitlib.ZeroHash(),
		NewHash: gitlib.ZeroHash(),
		HasOld:  true,
		HasNew:  true,
	}
	results := bridge.BatchDiffBlobs([]gitlib.DiffRequest{req})
	require.Len(t, results, 1)
	require.Error(t, results[0].Error)
	require.Equal(t, gitlib.ErrDiffLookup, results[0].Error)
}

// TestCGOBridge_TreeDiffSameHash verifies TreeDiff returns empty when both tree hashes are equal (skip path).
func TestCGOBridge_TreeDiffSameHash(t *testing.T) {
	t.Parallel()

	tr := newTestRepo(t)
	defer tr.cleanup()

	tr.createFile("a.txt", "a")
	hash := tr.commit("only")

	repo, err := gitlib.OpenRepository(tr.path)
	require.NoError(t, err)

	defer repo.Free()

	commit, err := repo.LookupCommit(hash)
	require.NoError(t, err)

	defer commit.Free()

	treeHash := commit.TreeHash()
	require.False(t, treeHash.IsZero())

	bridge := gitlib.NewCGOBridge(repo)
	changes, err := bridge.TreeDiff(treeHash, treeHash)
	require.NoError(t, err)
	require.Empty(t, changes)
}
