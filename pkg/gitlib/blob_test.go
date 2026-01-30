package gitlib_test

import (
	"io"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

func TestBlobReaderViaBlob(t *testing.T) {
	repo := setupTestRepo(t)
	defer repo.Free()

	commit := getHeadCommit(t, repo)
	defer commit.Free()

	file, err := commit.File("test.txt")
	require.NoError(t, err)

	blob, err := repo.LookupBlob(file.Hash)
	require.NoError(t, err)

	defer blob.Free()

	reader := blob.Reader()
	data, err := io.ReadAll(reader)
	require.NoError(t, err)

	assert.Equal(t, "test content", string(data))
}

func TestBlobContents(t *testing.T) {
	repo := setupTestRepo(t)
	defer repo.Free()

	commit := getHeadCommit(t, repo)
	defer commit.Free()

	file, err := commit.File("test.txt")
	require.NoError(t, err)

	blob, err := repo.LookupBlob(file.Hash)
	require.NoError(t, err)

	defer blob.Free()

	assert.Equal(t, []byte("test content"), blob.Contents())
	assert.Equal(t, int64(12), blob.Size())
	assert.NotNil(t, blob.Native())
}

func TestBlobHash(t *testing.T) {
	repo := setupTestRepo(t)
	defer repo.Free()

	commit := getHeadCommit(t, repo)
	defer commit.Free()

	file, err := commit.File("test.txt")
	require.NoError(t, err)

	blob, err := repo.LookupBlob(file.Hash)
	require.NoError(t, err)

	defer blob.Free()

	assert.Equal(t, file.Hash, blob.Hash())
	assert.False(t, blob.Hash().IsZero())
}

func TestBlobFree(t *testing.T) {
	repo := setupTestRepo(t)
	defer repo.Free()

	commit := getHeadCommit(t, repo)
	defer commit.Free()

	file, err := commit.File("test.txt")
	require.NoError(t, err)

	blob, err := repo.LookupBlob(file.Hash)
	require.NoError(t, err)

	// Free multiple times should be safe.
	blob.Free()
	blob.Free()
}

// Helper functions for test setup.
func setupTestRepo(t *testing.T) *gitlib.Repository {
	t.Helper()

	repo, err := gitlib.OpenRepository(testRepoPath(t))
	require.NoError(t, err)

	return repo
}

func getHeadCommit(t *testing.T, repo *gitlib.Repository) *gitlib.Commit {
	t.Helper()

	head, err := repo.Head()
	require.NoError(t, err)

	commit, err := repo.LookupCommit(head)
	require.NoError(t, err)

	return commit
}

func testRepoPath(t *testing.T) string {
	t.Helper()

	// This will be set by the integration test that creates a real repo.
	// For now, we use a test fixture or skip.
	t.Skip("Requires integration test setup")

	return ""
}
