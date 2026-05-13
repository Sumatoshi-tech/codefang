package alg

import (
	"errors"
	"io"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// sliceIter is a test stub implementing Iterator[T] over a slice.
type sliceIter[T any] struct {
	items []T
	pos   int
	err   error // Injected error at exhaustion (instead of EOF).
}

func newSliceIter[T any](items ...T) *sliceIter[T] {
	return &sliceIter[T]{items: items}
}

func newErrorIter[T any](items []T, err error) *sliceIter[T] {
	return &sliceIter[T]{items: items, err: err}
}

func (it *sliceIter[T]) Next() (T, error) {
	if it.pos >= len(it.items) {
		var zero T

		if it.err != nil {
			return zero, it.err
		}

		return zero, io.EOF
	}

	item := it.items[it.pos]
	it.pos++

	return item, nil
}

func (it *sliceIter[T]) Close() {
	it.pos = len(it.items)
}

func TestCollectN_EmptyIterator(t *testing.T) {
	t.Parallel()

	iter := newSliceIter[int]()

	got, err := CollectN[int](iter, 0)
	require.NoError(t, err)
	assert.Empty(t, got)
}

func TestCollectN_CollectAll(t *testing.T) {
	t.Parallel()

	iter := newSliceIter(1, 2, 3, 4, 5)

	got, err := CollectN[int](iter, 0)
	require.NoError(t, err)
	assert.Equal(t, []int{1, 2, 3, 4, 5}, got)
}

func TestCollectN_WithLimit(t *testing.T) {
	t.Parallel()

	iter := newSliceIter(10, 20, 30, 40, 50)

	got, err := CollectN[int](iter, 3)
	require.NoError(t, err)
	assert.Equal(t, []int{10, 20, 30}, got)
}

func TestCollectN_LimitExceedsItems(t *testing.T) {
	t.Parallel()

	iter := newSliceIter("a", "b")

	got, err := CollectN[string](iter, 10)
	require.NoError(t, err)
	assert.Equal(t, []string{"a", "b"}, got)
}

var errIterFailed = errors.New("iterator failed")

func TestCollectN_ErrorPropagation(t *testing.T) {
	t.Parallel()

	iter := newErrorIter([]int{1, 2}, errIterFailed)

	got, err := CollectN[int](iter, 0)
	require.ErrorIs(t, err, errIterFailed)
	assert.Nil(t, got)
}

func TestCollectN_ErrorAfterPartialRead(t *testing.T) {
	t.Parallel()

	iter := newErrorIter([]int{1}, errIterFailed)

	got, err := CollectN[int](iter, 0)
	require.ErrorIs(t, err, errIterFailed)
	assert.Nil(t, got)
}

func TestCollectN_ExhaustedIterator(t *testing.T) {
	t.Parallel()

	iter := newSliceIter[int]()

	// First call — empty.
	got1, err := CollectN[int](iter, 0)
	require.NoError(t, err)
	assert.Empty(t, got1)

	// Second call — still empty (already exhausted).
	got2, err := CollectN[int](iter, 0)
	require.NoError(t, err)
	assert.Empty(t, got2)
}

func TestCollectN_LimitOne(t *testing.T) {
	t.Parallel()

	iter := newSliceIter(42, 99)

	got, err := CollectN[int](iter, 1)
	require.NoError(t, err)
	assert.Equal(t, []int{42}, got)
}
