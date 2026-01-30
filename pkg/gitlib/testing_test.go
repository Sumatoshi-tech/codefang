package gitlib_test

import (
	"testing"
	"time"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

func TestNewTestCommit(t *testing.T) {
	hash := gitlib.NewHash("abcdef1234567890abcdef1234567890abcdef12")
	author := gitlib.Signature{
		Name:  "Test Author",
		Email: "test@example.com",
		When:  time.Now(),
	}
	parent1 := gitlib.NewHash("1111111111111111111111111111111111111111")
	parent2 := gitlib.NewHash("2222222222222222222222222222222222222222")

	commit := gitlib.NewTestCommit(hash, author, "test message", parent1, parent2)

	assert.Equal(t, hash, commit.Hash())
	assert.Equal(t, author, commit.Author())
	assert.Equal(t, author, commit.Committer()) // Committer defaults to author.
	assert.Equal(t, "test message", commit.Message())
	assert.Equal(t, 2, commit.NumParents())
}

func TestTestCommitParent(t *testing.T) {
	commit := gitlib.NewTestCommit(gitlib.Hash{}, gitlib.Signature{}, "msg")

	parent, err := commit.Parent(0)

	assert.Nil(t, parent)
	assert.ErrorIs(t, err, gitlib.ErrMockNotImplemented)
}

func TestTestCommitTree(t *testing.T) {
	commit := gitlib.NewTestCommit(gitlib.Hash{}, gitlib.Signature{}, "msg")

	tree, err := commit.Tree()

	assert.Nil(t, tree)
	assert.ErrorIs(t, err, gitlib.ErrMockNotImplemented)
}

func TestTestCommitFiles(t *testing.T) {
	commit := gitlib.NewTestCommit(gitlib.Hash{}, gitlib.Signature{}, "msg")

	files, err := commit.Files()

	assert.Nil(t, files)
	assert.ErrorIs(t, err, gitlib.ErrMockNotImplemented)
}

func TestTestCommitFile(t *testing.T) {
	commit := gitlib.NewTestCommit(gitlib.Hash{}, gitlib.Signature{}, "msg")

	file, err := commit.File("some/path")

	assert.Nil(t, file)
	assert.ErrorIs(t, err, gitlib.ErrMockNotImplemented)
}

func TestTestCommitFree(_ *testing.T) {
	commit := gitlib.NewTestCommit(gitlib.Hash{}, gitlib.Signature{}, "msg")

	// Should not panic.
	commit.Free()
}

func TestTestSignature(t *testing.T) {
	sig := gitlib.TestSignature("John Doe", "john@example.com")

	assert.Equal(t, "John Doe", sig.Name)
	assert.Equal(t, "john@example.com", sig.Email)
	assert.False(t, sig.When.IsZero())
}

func TestErrMockNotImplementedExists(t *testing.T) {
	require.Error(t, gitlib.ErrMockNotImplemented)
	assert.Equal(t, "mock: operation not implemented", gitlib.ErrMockNotImplemented.Error())
}
