package pipeline

import (
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// FRD: specs/frds/FRD-20260310-signal-on-drain.md.

const forwardTestItems = 3

func TestSignalOnDrain_ForwardsAllItems(t *testing.T) {
	t.Parallel()

	src := make(chan int, forwardTestItems)

	src <- 1

	src <- 2

	src <- 3

	close(src)

	forwarded, drained := SignalOnDrain(src)

	got := make([]int, 0, forwardTestItems)

	for v := range forwarded {
		got = append(got, v)
	}

	assert.Equal(t, []int{1, 2, 3}, got)

	// drained should be closed after forwarded is exhausted.
	_, open := <-drained
	assert.False(t, open, "drained channel should be closed")
}

func TestSignalOnDrain_EmptySource(t *testing.T) {
	t.Parallel()

	src := make(chan string)

	close(src)

	forwarded, drained := SignalOnDrain(src)

	// forwarded should be immediately closed.
	v, open := <-forwarded
	assert.False(t, open, "forwarded should be closed for empty source")
	assert.Empty(t, v)

	// drained should also be closed.
	_, open = <-drained
	assert.False(t, open, "drained should be closed for empty source")
}

func TestSignalOnDrain_DrainedClosesAfterForwarded(t *testing.T) {
	t.Parallel()

	src := make(chan int, 1)

	src <- 42

	close(src)

	forwarded, drained := SignalOnDrain(src)

	// Read the single item.
	val, ok := <-forwarded
	require.True(t, ok)
	assert.Equal(t, 42, val)

	// forwarded should now be closed.
	_, ok = <-forwarded
	assert.False(t, ok)

	// drained should be closed.
	_, ok = <-drained
	assert.False(t, ok)
}

func TestSignalOnDrain_NilSource(t *testing.T) {
	t.Parallel()

	// nil channel blocks forever on receive; SignalOnDrain with nil src
	// should still return valid channels. However, since range over nil
	// channel blocks forever, we test that the function does not panic.
	// Note: nil source is a degenerate case. The goroutine will block
	// forever, so we just verify the function returns without panic.
	forwarded, drained := SignalOnDrain[int](nil)

	assert.NotNil(t, forwarded)
	assert.NotNil(t, drained)
}
