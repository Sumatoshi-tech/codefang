package gitlib_test

import (
	"context"
	"testing"

	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

func TestNewDiffStreamer_ZeroBatchSize(t *testing.T) {
	tr := newTestRepo(t)
	defer tr.cleanup()

	tr.createFile("x.txt", "x")
	tr.commit("init")

	repo, err := gitlib.OpenRepository(tr.path)
	require.NoError(t, err)

	defer repo.Free()

	s := gitlib.NewDiffStreamer(repo, gitlib.BatchConfig{DiffBatchSize: 0})
	require.NotNil(t, s)
}

func TestBatchProcessor_NewBatchProcessor(t *testing.T) {
	tr := newTestRepo(t)
	defer tr.cleanup()

	tr.createFile("x.txt", "x")
	tr.commit("first")

	repo, err := gitlib.OpenRepository(tr.path)
	require.NoError(t, err)

	defer repo.Free()

	config := gitlib.DefaultBatchConfig()
	p := gitlib.NewBatchProcessor(repo, config)
	require.NotNil(t, p)
}

func TestBatchProcessor_LoadBlobs(t *testing.T) {
	tr := newTestRepo(t)
	defer tr.cleanup()

	tr.createFile("a.txt", "content")
	firstHash := tr.commit("first")

	repo, err := gitlib.OpenRepository(tr.path)
	require.NoError(t, err)

	defer repo.Free()

	// Get blob hash from first commit
	commit, err := repo.LookupCommit(firstHash)
	require.NoError(t, err)
	tree, err := commit.Tree()
	require.NoError(t, err)
	entry, err := tree.EntryByPath("a.txt")
	require.NoError(t, err)

	hash := entry.Hash()

	tree.Free()
	commit.Free()

	p := gitlib.NewBatchProcessor(repo, gitlib.DefaultBatchConfig())
	results := p.LoadBlobs([]gitlib.Hash{hash})
	require.Len(t, results, 1)
	require.NoError(t, results[0].Error)
	require.Equal(t, hash, results[0].Hash)
	require.Equal(t, []byte("content"), results[0].Data)
}

func TestBatchProcessor_LoadBlobsEmpty(t *testing.T) {
	tr := newTestRepo(t)
	defer tr.cleanup()

	tr.createFile("x.txt", "x")
	tr.commit("init")

	repo, err := gitlib.OpenRepository(tr.path)
	require.NoError(t, err)

	defer repo.Free()

	p := gitlib.NewBatchProcessor(repo, gitlib.DefaultBatchConfig())
	results := p.LoadBlobs(nil)
	require.Nil(t, results)
}

func TestBatchProcessor_LoadBlobsAsCached(t *testing.T) {
	tr := newTestRepo(t)
	defer tr.cleanup()

	tr.createFile("b.txt", "cached")
	firstHash := tr.commit("first")

	repo, err := gitlib.OpenRepository(tr.path)
	require.NoError(t, err)

	defer repo.Free()

	commit, err := repo.LookupCommit(firstHash)
	require.NoError(t, err)
	tree, err := commit.Tree()
	require.NoError(t, err)
	entry, err := tree.EntryByPath("b.txt")
	require.NoError(t, err)

	hash := entry.Hash()

	tree.Free()
	commit.Free()

	p := gitlib.NewBatchProcessor(repo, gitlib.DefaultBatchConfig())
	cached := p.LoadBlobsAsCached([]gitlib.Hash{hash})
	require.Len(t, cached, 1)
	require.NotNil(t, cached[0])
	require.Equal(t, hash, cached[0].Hash())
	require.Equal(t, []byte("cached"), cached[0].Data)
}

func TestBatchProcessor_ProcessCommitBlobs(t *testing.T) {
	tr := newTestRepo(t)
	defer tr.cleanup()

	tr.createFile("f.txt", "v1")
	firstHash := tr.commit("first")
	tr.createFile("f.txt", "v2")
	secondHash := tr.commit("second")

	repo, err := gitlib.OpenRepository(tr.path)
	require.NoError(t, err)

	defer repo.Free()

	firstCommit, err := repo.LookupCommit(firstHash)
	require.NoError(t, err)
	secondCommit, err := repo.LookupCommit(secondHash)
	require.NoError(t, err)
	firstTree, err := firstCommit.Tree()
	require.NoError(t, err)
	secondTree, err := secondCommit.Tree()
	require.NoError(t, err)
	changes, err := gitlib.TreeDiff(repo, firstTree, secondTree)
	require.NoError(t, err)
	firstTree.Free()
	secondTree.Free()
	firstCommit.Free()
	secondCommit.Free()

	p := gitlib.NewBatchProcessor(repo, gitlib.DefaultBatchConfig())
	blobs := p.ProcessCommitBlobs(changes)
	require.NotEmpty(t, blobs)
}

func TestBatchProcessor_ProcessCommitDiffs(t *testing.T) {
	tr := newTestRepo(t)
	defer tr.cleanup()

	tr.createFile("f.txt", "line1\nline2\n")
	firstHash := tr.commit("first")
	tr.createFile("f.txt", "line1\nline2\nline3\n")
	secondHash := tr.commit("second")

	repo, err := gitlib.OpenRepository(tr.path)
	require.NoError(t, err)

	defer repo.Free()

	firstCommit, err := repo.LookupCommit(firstHash)
	require.NoError(t, err)
	secondCommit, err := repo.LookupCommit(secondHash)
	require.NoError(t, err)
	firstTree, err := firstCommit.Tree()
	require.NoError(t, err)
	secondTree, err := secondCommit.Tree()
	require.NoError(t, err)
	changes, err := gitlib.TreeDiff(repo, firstTree, secondTree)
	require.NoError(t, err)
	firstTree.Free()
	secondTree.Free()
	firstCommit.Free()
	secondCommit.Free()

	p := gitlib.NewBatchProcessor(repo, gitlib.DefaultBatchConfig())
	diffs := p.ProcessCommitDiffs(changes)
	require.Len(t, diffs, 1)

	for path, res := range diffs {
		require.NoError(t, res.Error, "path %s", path)
		require.NotEmpty(t, res.Ops)
	}
}

func TestBatchProcessor_ComputeDiffs(t *testing.T) {
	tr := newTestRepo(t)
	defer tr.cleanup()

	tr.createFile("f.txt", "a\nb\n")
	firstHash := tr.commit("first")
	tr.createFile("f.txt", "a\nb\nc\n")
	secondHash := tr.commit("second")

	repo, err := gitlib.OpenRepository(tr.path)
	require.NoError(t, err)

	defer repo.Free()

	firstCommit, err := repo.LookupCommit(firstHash)
	require.NoError(t, err)
	secondCommit, err := repo.LookupCommit(secondHash)
	require.NoError(t, err)
	firstTree, err := firstCommit.Tree()
	require.NoError(t, err)
	secondTree, err := secondCommit.Tree()
	require.NoError(t, err)
	changes, err := gitlib.TreeDiff(repo, firstTree, secondTree)
	require.NoError(t, err)
	firstTree.Free()
	secondTree.Free()
	firstCommit.Free()
	secondCommit.Free()

	require.Len(t, changes, 1)
	ch := changes[0]
	req := gitlib.DiffRequest{OldHash: ch.From.Hash, NewHash: ch.To.Hash, HasOld: true, HasNew: true}
	p := gitlib.NewBatchProcessor(repo, gitlib.DefaultBatchConfig())
	results := p.ComputeDiffs([]gitlib.DiffRequest{req})
	require.Len(t, results, 1)
	require.NoError(t, results[0].Error)
	require.NotEmpty(t, results[0].Ops)
}

func TestBlobStreamer_Stream(t *testing.T) {
	tr := newTestRepo(t)
	defer tr.cleanup()

	tr.createFile("a.txt", "aaa")
	firstHash := tr.commit("first")

	repo, err := gitlib.OpenRepository(tr.path)
	require.NoError(t, err)

	defer repo.Free()

	commit, err := repo.LookupCommit(firstHash)
	require.NoError(t, err)
	tree, err := commit.Tree()
	require.NoError(t, err)
	entry, err := tree.EntryByPath("a.txt")
	require.NoError(t, err)

	hash := entry.Hash()

	tree.Free()
	commit.Free()

	config := gitlib.DefaultBatchConfig()
	streamer := gitlib.NewBlobStreamer(repo, config)
	ctx := context.Background()

	hashCh := make(chan []gitlib.Hash, 1)
	hashCh <- []gitlib.Hash{hash}

	close(hashCh)

	out := streamer.Stream(ctx, hashCh)

	batches := make([]gitlib.BlobBatch, 0, 16)
	for b := range out {
		batches = append(batches, b)
	}

	require.Len(t, batches, 1)
	require.Len(t, batches[0].Blobs, 1)
	require.NotNil(t, batches[0].Blobs[0])
	require.Equal(t, hash, batches[0].Blobs[0].Hash())
	require.Equal(t, []byte("aaa"), batches[0].Blobs[0].Data)
}

func TestDiffStreamer_Stream(t *testing.T) {
	tr := newTestRepo(t)
	defer tr.cleanup()

	tr.createFile("f.txt", "v1\n")
	firstHash := tr.commit("first")
	tr.createFile("f.txt", "v1\nv2\n")
	secondHash := tr.commit("second")

	repo, err := gitlib.OpenRepository(tr.path)
	require.NoError(t, err)

	defer repo.Free()

	firstCommit, err := repo.LookupCommit(firstHash)
	require.NoError(t, err)
	secondCommit, err := repo.LookupCommit(secondHash)
	require.NoError(t, err)
	firstTree, err := firstCommit.Tree()
	require.NoError(t, err)
	secondTree, err := secondCommit.Tree()
	require.NoError(t, err)
	changes, err := gitlib.TreeDiff(repo, firstTree, secondTree)
	require.NoError(t, err)
	firstTree.Free()
	secondTree.Free()
	firstCommit.Free()
	secondCommit.Free()

	require.Len(t, changes, 1)
	ch := changes[0]
	req := gitlib.DiffRequest{OldHash: ch.From.Hash, NewHash: ch.To.Hash, HasOld: true, HasNew: true}

	streamer := gitlib.NewDiffStreamer(repo, gitlib.DefaultBatchConfig())
	ctx := context.Background()

	reqCh := make(chan []gitlib.DiffRequest, 1)
	reqCh <- []gitlib.DiffRequest{req}

	close(reqCh)

	out := streamer.Stream(ctx, reqCh)

	batches := make([]gitlib.DiffBatch, 0, 16)
	for b := range out {
		batches = append(batches, b)
	}

	require.Len(t, batches, 1)
	require.Len(t, batches[0].Diffs, 1)
	require.NoError(t, batches[0].Diffs[0].Error)
	require.NotEmpty(t, batches[0].Diffs[0].Ops)
}
