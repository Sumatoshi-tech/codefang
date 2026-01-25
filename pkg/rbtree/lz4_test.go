package rbtree

import (
	"testing"

	"github.com/stretchr/testify/assert"
)

func TestCompressDecompressUInt32Slice(t *testing.T) {
	data := make([]uint32, 1000)
	for i := range data {
		data[i] = 7
	}
	packed := CompressUInt32Slice(data)

	// Check that compression actually reduced the size (or at least didn't fail)
	assert.NotNil(t, packed)
	assert.True(t, len(packed) > 0, "Compression should produce some output")

	// Clear the data and decompress
	for i := range data {
		data[i] = 0
	}
	DecompressUInt32Slice(packed, data)

	// Verify that all values were restored correctly
	for i := range data {
		assert.Equal(t, uint32(7), data[i], "Value at index %d should be 7", i)
	}
}
