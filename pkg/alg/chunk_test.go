package alg_test

import (
	"testing"

	"github.com/stretchr/testify/assert"

	"github.com/Sumatoshi-tech/codefang/pkg/alg"
)

func TestChunk_ZeroTotal(t *testing.T) {
	t.Parallel()

	result := alg.Chunk(0, 5)
	assert.Nil(t, result)
}

func TestChunk_NegativeTotal(t *testing.T) {
	t.Parallel()

	result := alg.Chunk(-1, 5)
	assert.Nil(t, result)
}

func TestChunk_ZeroSize(t *testing.T) {
	t.Parallel()

	result := alg.Chunk(10, 0)
	assert.Nil(t, result)
}

func TestChunk_NegativeSize(t *testing.T) {
	t.Parallel()

	result := alg.Chunk(10, -1)
	assert.Nil(t, result)
}

func TestChunk_SizeGreaterThanTotal(t *testing.T) {
	t.Parallel()

	result := alg.Chunk(5, 10)
	expected := []alg.Range{{Start: 0, End: 5}}
	assert.Equal(t, expected, result)
}

func TestChunk_ExactDivision(t *testing.T) {
	t.Parallel()

	result := alg.Chunk(10, 5)
	expected := []alg.Range{
		{Start: 0, End: 5},
		{Start: 5, End: 10},
	}
	assert.Equal(t, expected, result)
}

func TestChunk_Remainder(t *testing.T) {
	t.Parallel()

	result := alg.Chunk(7, 3)
	expected := []alg.Range{
		{Start: 0, End: 3},
		{Start: 3, End: 6},
		{Start: 6, End: 7},
	}
	assert.Equal(t, expected, result)
}

func TestChunk_SingleElement(t *testing.T) {
	t.Parallel()

	result := alg.Chunk(1, 1)
	expected := []alg.Range{{Start: 0, End: 1}}
	assert.Equal(t, expected, result)
}

func TestChunk_SizeEqualsTotal(t *testing.T) {
	t.Parallel()

	result := alg.Chunk(5, 5)
	expected := []alg.Range{{Start: 0, End: 5}}
	assert.Equal(t, expected, result)
}

func TestChunk_Contiguous(t *testing.T) {
	t.Parallel()

	const total = 100

	const size = 7

	chunks := alg.Chunk(total, size)

	// First chunk starts at 0.
	assert.Equal(t, 0, chunks[0].Start)

	// Last chunk ends at total.
	assert.Equal(t, total, chunks[len(chunks)-1].End)

	// Adjacent chunks are contiguous.
	for i := 1; i < len(chunks); i++ {
		assert.Equal(t, chunks[i-1].End, chunks[i].Start)
	}
}
