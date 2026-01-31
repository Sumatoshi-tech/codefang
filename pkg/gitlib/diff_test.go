package gitlib_test

import (
	"testing"

	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

func TestDiffBlobsFromCache(t *testing.T) {
	// Covers DiffBlobsFromCache and countLines
	oldData := []byte("line1\nline2\n")
	newData := []byte("line1\nline2\nline3\n")
	result := gitlib.DiffBlobsFromCache(oldData, newData)
	require.NotNil(t, result)
	require.Equal(t, 2, result.OldLines)
	require.Equal(t, 3, result.NewLines)
	require.Len(t, result.Diffs, 2)
	require.Equal(t, gitlib.LineDiffDelete, result.Diffs[0].Type)
	require.Equal(t, 2, result.Diffs[0].LineCount)
	require.Equal(t, gitlib.LineDiffInsert, result.Diffs[1].Type)
	require.Equal(t, 3, result.Diffs[1].LineCount)
}

func TestDiffBlobsFromCache_EmptyOld(t *testing.T) {
	result := gitlib.DiffBlobsFromCache(nil, []byte("a\nb\n"))
	require.NotNil(t, result)
	require.Equal(t, 0, result.OldLines)
	require.Equal(t, 2, result.NewLines)
}

func TestDiffBlobsFromCache_EmptyNew(t *testing.T) {
	result := gitlib.DiffBlobsFromCache([]byte("x\n"), nil)
	require.NotNil(t, result)
	require.Equal(t, 1, result.OldLines)
	require.Equal(t, 0, result.NewLines)
}

func TestDiffBlobs(t *testing.T) {
	tr := newTestRepo(t)
	defer tr.cleanup()

	tr.createFile("f.txt", "v1\nv2\n")
	firstHash := tr.commit("first")
	tr.createFile("f.txt", "v1\nv2\nv3\n")
	secondHash := tr.commit("second")

	repo, err := gitlib.OpenRepository(tr.path)
	require.NoError(t, err)

	defer repo.Free()

	// Get blob hashes from both commits
	commit1, err := repo.LookupCommit(firstHash)
	require.NoError(t, err)
	tree1, err := commit1.Tree()
	require.NoError(t, err)
	entry1, err := tree1.EntryByPath("f.txt")
	require.NoError(t, err)

	hash1 := entry1.Hash()

	tree1.Free()
	commit1.Free()

	commit2, err := repo.LookupCommit(secondHash)
	require.NoError(t, err)
	tree2, err := commit2.Tree()
	require.NoError(t, err)
	entry2, err := tree2.EntryByPath("f.txt")
	require.NoError(t, err)

	hash2 := entry2.Hash()

	tree2.Free()
	commit2.Free()

	oldBlob, err := repo.LookupBlob(hash1)
	require.NoError(t, err)

	defer oldBlob.Free()

	newBlob, err := repo.LookupBlob(hash2)
	require.NoError(t, err)

	defer newBlob.Free()

	result, err := gitlib.DiffBlobs(oldBlob, newBlob, "f.txt", "f.txt")
	require.NoError(t, err)
	require.NotNil(t, result)
	require.Equal(t, 2, result.OldLines)
	require.Equal(t, 3, result.NewLines)
	require.NotEmpty(t, result.Diffs)
}
