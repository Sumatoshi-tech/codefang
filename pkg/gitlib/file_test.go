package gitlib_test

import (
	"io"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

func TestErrParentNotFoundExists(t *testing.T) {
	// Verify the error sentinel is accessible.
	require.Error(t, gitlib.ErrParentNotFound)
	assert.Equal(t, "parent commit not found", gitlib.ErrParentNotFound.Error())
}

func TestErrParentNotFoundIsError(t *testing.T) {
	err := gitlib.ErrParentNotFound
	assert.ErrorIs(t, err, gitlib.ErrParentNotFound)
}

func TestIOEOFIsRecognized(t *testing.T) {
	// Verify io.EOF is the expected end-of-iteration signal.
	assert.Equal(t, "EOF", io.EOF.Error())
}

func TestHashConstants(t *testing.T) {
	assert.Equal(t, 20, gitlib.HashSize)
	assert.Equal(t, 40, gitlib.HashHexSize)
}

// Note: File and FileIter tests that require a real repository
// are in gitlib_test.go.
