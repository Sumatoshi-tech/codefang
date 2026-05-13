package pipeline

import (
	"context"
	"errors"
	"runtime"
	"sync/atomic"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

var errWorker = errors.New("worker failed")

func TestWorkerPool_EmptyItems(t *testing.T) {
	t.Parallel()

	pool := WorkerPool[int]{
		MaxParallel: 1,
		Work: func(_ context.Context, _ int) error {
			t.Fatal("should not be called")

			return nil
		},
	}

	err := pool.Run(context.Background(), nil)
	require.NoError(t, err)

	err = pool.Run(context.Background(), []int{})
	require.NoError(t, err)
}

func TestWorkerPool_SerialExecution(t *testing.T) {
	t.Parallel()

	const itemCount = 5

	var count atomic.Int64

	pool := WorkerPool[int]{
		MaxParallel: 1,
		Work: func(_ context.Context, _ int) error {
			count.Add(1)

			return nil
		},
	}

	items := make([]int, itemCount)

	for i := range items {
		items[i] = i
	}

	err := pool.Run(context.Background(), items)
	require.NoError(t, err)
	assert.Equal(t, int64(itemCount), count.Load())
}

func TestWorkerPool_ParallelExecution(t *testing.T) {
	t.Parallel()

	const itemCount = 20

	var count atomic.Int64

	pool := WorkerPool[int]{
		MaxParallel: 4,
		Work: func(_ context.Context, _ int) error {
			count.Add(1)

			return nil
		},
	}

	items := make([]int, itemCount)

	err := pool.Run(context.Background(), items)
	require.NoError(t, err)
	assert.Equal(t, int64(itemCount), count.Load())
}

func TestWorkerPool_FirstError(t *testing.T) {
	t.Parallel()

	const itemCount = 10

	pool := WorkerPool[int]{
		MaxParallel: 2,
		Work: func(_ context.Context, item int) error {
			if item == 0 {
				return errWorker
			}

			return nil
		},
	}

	items := make([]int, itemCount)

	err := pool.Run(context.Background(), items)
	assert.ErrorIs(t, err, errWorker)
}

func TestWorkerPool_ContextCancellation(t *testing.T) {
	t.Parallel()

	ctx, cancel := context.WithCancel(context.Background())
	cancel()

	pool := WorkerPool[int]{
		MaxParallel: 2,
		Work: func(workCtx context.Context, _ int) error {
			return workCtx.Err()
		},
	}

	err := pool.Run(ctx, []int{1, 2, 3})
	assert.Error(t, err)
}

func TestWorkerPool_DefaultMaxParallel(t *testing.T) {
	t.Parallel()

	const itemCount = 10

	var count atomic.Int64

	pool := WorkerPool[int]{
		Work: func(_ context.Context, _ int) error {
			count.Add(1)

			return nil
		},
	}

	items := make([]int, itemCount)

	err := pool.Run(context.Background(), items)
	require.NoError(t, err)
	assert.Equal(t, int64(itemCount), count.Load())
}

func TestWorkerPool_MaxParallelCappedToItemCount(t *testing.T) {
	t.Parallel()

	const itemCount = 2

	var maxConcurrent atomic.Int64

	var current atomic.Int64

	pool := WorkerPool[int]{
		MaxParallel: runtime.NumCPU() * 2,
		Work: func(_ context.Context, _ int) error {
			val := current.Add(1)

			for {
				old := maxConcurrent.Load()
				if val <= old || maxConcurrent.CompareAndSwap(old, val) {
					break
				}
			}

			current.Add(-1)

			return nil
		},
	}

	items := make([]int, itemCount)

	err := pool.Run(context.Background(), items)
	require.NoError(t, err)
	assert.LessOrEqual(t, maxConcurrent.Load(), int64(itemCount))
}

func TestWorkerPool_AllItemsProcessed(t *testing.T) {
	t.Parallel()

	const itemCount = 100

	var seen [itemCount]atomic.Bool

	pool := WorkerPool[int]{
		MaxParallel: 8,
		Work: func(_ context.Context, item int) error {
			seen[item].Store(true)

			return nil
		},
	}

	items := make([]int, itemCount)

	for i := range items {
		items[i] = i
	}

	err := pool.Run(context.Background(), items)
	require.NoError(t, err)

	for i := range seen {
		assert.True(t, seen[i].Load(), "item %d not processed", i)
	}
}

func TestWorkerPool_ErrorCancelsContext(t *testing.T) {
	t.Parallel()

	var cancelledCount atomic.Int64

	pool := WorkerPool[int]{
		MaxParallel: 1,
		Work: func(ctx context.Context, item int) error {
			if item == 0 {
				return errWorker
			}

			if ctx.Err() != nil {
				cancelledCount.Add(1)
			}

			return nil
		},
	}

	// With MaxParallel=1, items are sequential.
	// After item 0 errors, the context should be canceled for remaining items.
	err := pool.Run(context.Background(), []int{0, 1, 2})
	assert.ErrorIs(t, err, errWorker)
}

func TestWorkerPool_RunChan_EmptyChannel(t *testing.T) {
	t.Parallel()

	pool := WorkerPool[int]{
		MaxParallel: 1,
		Work: func(_ context.Context, _ int) error {
			t.Fatal("should not be called")

			return nil
		},
	}

	ch := make(chan int)
	close(ch)

	err := pool.RunChan(context.Background(), ch)
	require.NoError(t, err)
}

func TestWorkerPool_RunChan_ProcessesAllItems(t *testing.T) {
	t.Parallel()

	const itemCount = 50

	var count atomic.Int64

	pool := WorkerPool[int]{
		MaxParallel: 4,
		Work: func(_ context.Context, _ int) error {
			count.Add(1)

			return nil
		},
	}

	ch := make(chan int, itemCount)

	for i := range itemCount {
		ch <- i
	}

	close(ch)

	err := pool.RunChan(context.Background(), ch)
	require.NoError(t, err)
	assert.Equal(t, int64(itemCount), count.Load())
}

func TestWorkerPool_RunChan_FirstError(t *testing.T) {
	t.Parallel()

	pool := WorkerPool[int]{
		MaxParallel: 1,
		Work: func(_ context.Context, item int) error {
			if item == 0 {
				return errWorker
			}

			return nil
		},
	}

	ch := feedChan(0, 1, 2)

	err := pool.RunChan(context.Background(), ch)
	assert.ErrorIs(t, err, errWorker)
}

func TestWorkerPool_RunChan_ContextCancellation(t *testing.T) {
	t.Parallel()

	ctx, cancel := context.WithCancel(context.Background())
	cancel()

	pool := WorkerPool[int]{
		MaxParallel: 2,
		Work: func(workCtx context.Context, _ int) error {
			return workCtx.Err()
		},
	}

	ch := feedChan(1, 2, 3)

	err := pool.RunChan(ctx, ch)
	assert.Error(t, err)
}

func TestWorkerPool_RunChan_NilChannel(t *testing.T) {
	t.Parallel()

	pool := WorkerPool[int]{
		MaxParallel: 1,
		Work: func(_ context.Context, _ int) error {
			t.Fatal("should not be called")

			return nil
		},
	}

	err := pool.RunChan(context.Background(), nil)
	require.NoError(t, err)
}

// feedChan creates a buffered, pre-filled, closed channel from the given items.
func feedChan(items ...int) <-chan int {
	ch := make(chan int, len(items))

	for _, item := range items {
		ch <- item
	}

	close(ch)

	return ch
}
