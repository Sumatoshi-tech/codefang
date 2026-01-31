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

func TestCachedBlob_CountLines_Caching(t *testing.T) {
	// Test that CountLines caches the result and returns same value on repeated calls.
	blob := gitlib.NewCachedBlobForTest([]byte("line1\nline2\nline3\n"))

	// First call computes the result.
	count1, err1 := blob.CountLines()
	require.NoError(t, err1)
	assert.Equal(t, 3, count1)

	// Second call should return cached result (same value).
	count2, err2 := blob.CountLines()
	require.NoError(t, err2)
	assert.Equal(t, 3, count2)

	// Verify both calls return identical results.
	assert.Equal(t, count1, count2)
}

func TestCachedBlob_CountLines_BinaryCaching(t *testing.T) {
	// Test that binary detection is also cached.
	blob := gitlib.NewCachedBlobForTest([]byte("binary\x00data"))

	// First call detects binary.
	count1, err1 := blob.CountLines()
	require.ErrorIs(t, err1, gitlib.ErrBinary)
	assert.Equal(t, 0, count1)

	// Second call should return cached binary error.
	count2, err2 := blob.CountLines()
	require.ErrorIs(t, err2, gitlib.ErrBinary)
	assert.Equal(t, 0, count2)
}

func TestCachedBlob_CountLines_EmptyBlob(t *testing.T) {
	blob := gitlib.NewCachedBlobForTest([]byte{})

	count, err := blob.CountLines()
	require.NoError(t, err)
	assert.Equal(t, 0, count)
}

func TestCachedBlob_CountLines_NoTrailingNewline(t *testing.T) {
	blob := gitlib.NewCachedBlobForTest([]byte("line1\nline2"))

	count, err := blob.CountLines()
	require.NoError(t, err)
	assert.Equal(t, 2, count)
}

func TestCachedBlob_CountLines_ConcurrentAccess(t *testing.T) {
	// Test that concurrent CountLines calls are safe and return consistent results.
	blob := gitlib.NewCachedBlobForTest([]byte("line1\nline2\nline3\nline4\nline5\n"))

	const goroutines = 100

	results := make(chan int, goroutines)
	errors := make(chan error, goroutines)

	for range goroutines {
		go func() {
			count, err := blob.CountLines()
			if err != nil {
				errors <- err

				return
			}

			results <- count
		}()
	}

	for range goroutines {
		select {
		case err := <-errors:
			t.Fatalf("unexpected error: %v", err)
		case count := <-results:
			assert.Equal(t, 5, count)
		}
	}
}

// Note: CachedBlob tests that require a real repository
// are in gitlib_test.go.
