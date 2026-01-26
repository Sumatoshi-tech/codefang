package rbtree_test

import (
	"testing"

	"github.com/stretchr/testify/assert"

	"github.com/Sumatoshi-tech/codefang/pkg/rbtree"
)

func TestCompressDecompressUInt32Slice(t *testing.T) {
	t.Parallel()

	data := make([]uint32, 1000)
	for idx := range data {
		data[idx] = 7
	}

	packed := rbtree.CompressUInt32Slice(data)

	// Check that compression actually reduced the size (or at least didn't fail).
	assert.NotNil(t, packed)
	assert.NotEmpty(t, packed, "Compression should produce some output")

	// Clear the data and decompress.
	for idx := range data {
		data[idx] = 0
	}

	rbtree.DecompressUInt32Slice(packed, data)

	// Verify that all values were restored correctly.
	for idx := range data {
		assert.Equal(t, uint32(7), data[idx], "Value at index %d should be 7", idx)
	}
}
