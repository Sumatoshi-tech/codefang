package pipeline_test

import (
	"testing"

	"github.com/stretchr/testify/assert"

	"github.com/Sumatoshi-tech/codefang/pkg/pipeline"
)

func TestThresholdBatcher_FlushAtThreshold(t *testing.T) {
	t.Parallel()

	const threshold = 3

	b := pipeline.NewThresholdBatcher[int](threshold)

	assert.False(t, b.Add(1))
	assert.False(t, b.Add(2))
	assert.True(t, b.Add(3)) // Threshold reached.

	batch, ok := b.Flush()
	assert.True(t, ok)
	assert.Equal(t, []int{1, 2, 3}, batch)
}

func TestThresholdBatcher_PartialFlush(t *testing.T) {
	t.Parallel()

	const threshold = 10

	b := pipeline.NewThresholdBatcher[string](threshold)

	b.Add("a")
	b.Add("b")

	// Flush before threshold.
	batch, ok := b.Flush()
	assert.True(t, ok)
	assert.Equal(t, []string{"a", "b"}, batch)
}

func TestThresholdBatcher_EmptyFlush(t *testing.T) {
	t.Parallel()

	const threshold = 5

	b := pipeline.NewThresholdBatcher[int](threshold)

	batch, ok := b.Flush()
	assert.False(t, ok)
	assert.Nil(t, batch)
}

func TestThresholdBatcher_DoubleFlush(t *testing.T) {
	t.Parallel()

	const threshold = 2

	b := pipeline.NewThresholdBatcher[int](threshold)

	b.Add(1)
	b.Add(2)

	batch1, ok1 := b.Flush()
	assert.True(t, ok1)
	assert.Equal(t, []int{1, 2}, batch1)

	// Second flush should be empty.
	batch2, ok2 := b.Flush()
	assert.False(t, ok2)
	assert.Nil(t, batch2)
}

func TestThresholdBatcher_ZeroThreshold_ClampsToOne(t *testing.T) {
	t.Parallel()

	b := pipeline.NewThresholdBatcher[int](0)

	// With threshold clamped to 1, first Add signals ready.
	assert.True(t, b.Add(42))

	batch, ok := b.Flush()
	assert.True(t, ok)
	assert.Equal(t, []int{42}, batch)
}

func TestPassthroughBatcher_AlwaysReady(t *testing.T) {
	t.Parallel()

	b := &pipeline.PassthroughBatcher[int]{}

	assert.True(t, b.Add(1))

	batch, ok := b.Flush()
	assert.True(t, ok)
	assert.Equal(t, []int{1}, batch)
}

func TestPassthroughBatcher_EmptyFlush(t *testing.T) {
	t.Parallel()

	b := &pipeline.PassthroughBatcher[int]{}

	batch, ok := b.Flush()
	assert.False(t, ok)
	assert.Nil(t, batch)
}

func TestPassthroughBatcher_DoubleFlush(t *testing.T) {
	t.Parallel()

	b := &pipeline.PassthroughBatcher[string]{}

	b.Add("hello")

	batch1, ok1 := b.Flush()
	assert.True(t, ok1)
	assert.Equal(t, []string{"hello"}, batch1)

	batch2, ok2 := b.Flush()
	assert.False(t, ok2)
	assert.Nil(t, batch2)
}

func TestPassthroughBatcher_OverwritesPrevious(t *testing.T) {
	t.Parallel()

	b := &pipeline.PassthroughBatcher[int]{}

	b.Add(1)
	b.Add(2) // Overwrites without flush.

	batch, ok := b.Flush()
	assert.True(t, ok)
	assert.Equal(t, []int{2}, batch)
}
