package gitlib_test

import (
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

func TestErrBinaryExists(t *testing.T) {
	// Verify the error sentinel is accessible.
	require.Error(t, gitlib.ErrBinary)
	assert.Equal(t, "binary", gitlib.ErrBinary.Error())
}

// Note: CachedBlob tests that require a real repository
// are in gitlib_test.go.
