package pipeline

import (
	"context"
	"fmt"
	"runtime"
	"sync"
)

// WorkerPool runs Work on each item with at most MaxParallel goroutines.
// Returns the first non-nil error encountered, or nil.
// Remaining goroutines observe context cancellation on first error.
type WorkerPool[T any] struct {
	// MaxParallel is the maximum number of concurrent goroutines.
	// Zero defaults to runtime.NumCPU().
	MaxParallel int
	// Work processes a single item. Must not be nil.
	Work func(ctx context.Context, item T) error
}

// Run processes all items with bounded concurrency.
// If any Work call returns a non-nil error, the derived context is canceled
// and Run returns that error after all goroutines finish.
func (p WorkerPool[T]) Run(ctx context.Context, items []T) error {
	if len(items) == 0 {
		return nil
	}

	if ctx.Err() != nil {
		return fmt.Errorf("worker pool: %w", ctx.Err())
	}

	workers := p.resolveWorkers(len(items))
	workCh := make(chan T, workers)
	ctx, cancel := context.WithCancel(ctx)

	defer cancel()

	var (
		wg       sync.WaitGroup
		errOnce  sync.Once
		firstErr error
	)

	wg.Add(workers)

	for range workers {
		go func() {
			defer wg.Done()

			for item := range workCh {
				if ctx.Err() != nil {
					continue
				}

				err := p.Work(ctx, item)
				if err != nil {
					errOnce.Do(func() {
						firstErr = err

						cancel()
					})
				}
			}
		}()
	}

	for _, item := range items {
		workCh <- item
	}

	close(workCh)
	wg.Wait()

	return firstErr
}

// resolveWorkers returns the effective worker count, clamped to item count.
func (p WorkerPool[T]) resolveWorkers(itemCount int) int {
	workers := p.MaxParallel
	if workers <= 0 {
		workers = runtime.NumCPU()
	}

	if workers > itemCount {
		workers = itemCount
	}

	return workers
}
