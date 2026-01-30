package gitlib_test

import (
	"errors"
	"io"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

// TestCommitIterCloseAfterExhaustion tests that calling Close() after the iterator
// has been exhausted (which frees the walk internally) does not cause a double-free.
// This is a regression test for a segfault caused by double-freeing the RevWalk.
func TestCommitIterCloseAfterExhaustion(t *testing.T) {
	tr := newTestRepo(t)
	defer tr.cleanup()

	tr.createFile("1.txt", "1")
	tr.commit("first")

	tr.createFile("2.txt", "2")
	tr.commit("second")

	repo, err := gitlib.OpenRepository(tr.path)
	require.NoError(t, err)

	defer repo.Free()

	iter, err := repo.Log(&gitlib.LogOptions{})
	require.NoError(t, err)

	// Iterate through all commits until EOF.
	// This should free the walk internally when EOF is reached.
	for {
		commit, nextErr := iter.Next()
		if errors.Is(nextErr, io.EOF) {
			break
		}

		require.NoError(t, nextErr)
		commit.Free()
	}

	// Close() after exhaustion should be safe (not cause double-free/segfault).
	iter.Close()

	// Calling Close() again should also be safe.
	iter.Close()
}

// TestCommitIterCloseBeforeExhaustion tests that calling Close() before
// exhausting the iterator correctly frees resources.
func TestCommitIterCloseBeforeExhaustion(t *testing.T) {
	tr := newTestRepo(t)
	defer tr.cleanup()

	tr.createFile("1.txt", "1")
	tr.commit("first")

	tr.createFile("2.txt", "2")
	tr.commit("second")

	tr.createFile("3.txt", "3")
	tr.commit("third")

	repo, err := gitlib.OpenRepository(tr.path)
	require.NoError(t, err)

	defer repo.Free()

	iter, err := repo.Log(&gitlib.LogOptions{})
	require.NoError(t, err)

	// Read only one commit.
	commit, err := iter.Next()
	require.NoError(t, err)
	commit.Free()

	// Close before exhaustion - should free the walk.
	iter.Close()

	// Calling Close() again should be safe.
	iter.Close()
}

// TestCommitIterNextAfterClose tests that calling Next() after Close() returns EOF.
func TestCommitIterNextAfterClose(t *testing.T) {
	tr := newTestRepo(t)
	defer tr.cleanup()

	tr.createFile("test.txt", "content")
	tr.commit("init")

	repo, err := gitlib.OpenRepository(tr.path)
	require.NoError(t, err)

	defer repo.Free()

	iter, err := repo.Log(&gitlib.LogOptions{})
	require.NoError(t, err)

	// Close immediately.
	iter.Close()

	// Next() after Close() should return EOF.
	commit, err := iter.Next()
	assert.Nil(t, commit)
	assert.ErrorIs(t, err, io.EOF)
}
