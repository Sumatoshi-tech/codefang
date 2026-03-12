package pipeline_test

// FRD: specs/frds/FRD-20260302-composable-pipeline-patterns.md.

import (
	"context"
	"testing"
	"time"

	"github.com/stretchr/testify/assert"

	"github.com/Sumatoshi-tech/codefang/pkg/pipeline"
)

const testTimeout = 2 * time.Second

func TestRunPC_BasicFlow(t *testing.T) {
	t.Parallel()

	pc := pipeline.RunPC[[]int, int, int]{
		Buffer: 2,
		Produce: func(_ context.Context, in []int, jobs chan<- int) {
			for _, v := range in {
				jobs <- v
			}
		},
		Consume: func(_ context.Context, jobs <-chan int, out chan<- int) {
			for v := range jobs {
				out <- v * 2
			}
		},
	}

	ctx, cancel := context.WithTimeout(context.Background(), testTimeout)
	defer cancel()

	out := pc.Run(ctx, []int{1, 2, 3, 4, 5})

	const expectedLen = 5

	results := make([]int, 0, expectedLen)

	for v := range out {
		results = append(results, v)
	}

	expected := []int{2, 4, 6, 8, 10}
	assert.Equal(t, expected, results)
}

func TestRunPC_EmptyProducer(t *testing.T) {
	t.Parallel()

	pc := pipeline.RunPC[[]int, int, int]{
		Buffer:  1,
		Produce: func(_ context.Context, _ []int, _ chan<- int) {},
		Consume: func(_ context.Context, jobs <-chan int, _ chan<- int) {
			for v := range jobs {
				_ = v
			}
		},
	}

	ctx, cancel := context.WithTimeout(context.Background(), testTimeout)
	defer cancel()

	out := pc.Run(ctx, nil)

	var count int

	for range out {
		count++
	}

	assert.Equal(t, 0, count)
}

func TestRunPC_ContextCancellation(t *testing.T) {
	t.Parallel()

	pc := pipeline.RunPC[int, int, int]{
		Buffer: 1,
		Produce: func(ctx context.Context, _ int, jobs chan<- int) {
			for i := range 100 {
				select {
				case jobs <- i:
				case <-ctx.Done():
					return
				}
			}
		},
		Consume: func(ctx context.Context, jobs <-chan int, out chan<- int) {
			for v := range jobs {
				select {
				case out <- v:
				case <-ctx.Done():
					return
				}
			}
		},
	}

	ctx, cancel := context.WithCancel(context.Background())

	out := pc.Run(ctx, 0)

	// Read one item then cancel.
	<-out
	cancel()

	// Drain remaining items; channel must close eventually.
	drained := make(chan struct{})

	go func() {
		for v := range out {
			_ = v
		}

		close(drained)
	}()

	select {
	case <-drained:
	case <-time.After(testTimeout):
		t.Fatal("output channel did not close after cancellation")
	}
}

func TestRunPC_PreservesOrder(t *testing.T) {
	t.Parallel()

	const itemCount = 100

	pc := pipeline.RunPC[int, int, int]{
		Buffer: 10,
		Produce: func(_ context.Context, n int, jobs chan<- int) {
			for i := range n {
				jobs <- i
			}
		},
		Consume: func(_ context.Context, jobs <-chan int, out chan<- int) {
			for v := range jobs {
				out <- v
			}
		},
	}

	ctx, cancel := context.WithTimeout(context.Background(), testTimeout)
	defer cancel()

	out := pc.Run(ctx, itemCount)

	results := make([]int, 0, itemCount)

	for v := range out {
		results = append(results, v)
	}

	assert.Len(t, results, itemCount)

	for i, v := range results {
		assert.Equal(t, i, v)
	}
}

func TestRunPC_ZeroBuffer_DefaultsToOne(t *testing.T) {
	t.Parallel()

	pc := pipeline.RunPC[int, int, int]{
		Buffer: 0,
		Produce: func(_ context.Context, _ int, jobs chan<- int) {
			jobs <- 42
		},
		Consume: func(_ context.Context, jobs <-chan int, out chan<- int) {
			for v := range jobs {
				out <- v
			}
		},
	}

	ctx, cancel := context.WithTimeout(context.Background(), testTimeout)
	defer cancel()

	out := pc.Run(ctx, 0)

	result := <-out
	assert.Equal(t, 42, result)

	// Channel must close.
	_, open := <-out
	assert.False(t, open)
}

func TestRunPC_OutputChannelCloses(t *testing.T) {
	t.Parallel()

	pc := pipeline.RunPC[int, string, string]{
		Buffer: 1,
		Produce: func(_ context.Context, _ int, jobs chan<- string) {
			jobs <- "hello"
		},
		Consume: func(_ context.Context, jobs <-chan string, out chan<- string) {
			for v := range jobs {
				out <- v
			}
		},
	}

	ctx, cancel := context.WithTimeout(context.Background(), testTimeout)
	defer cancel()

	out := pc.Run(ctx, 0)

	val := <-out
	assert.Equal(t, "hello", val)

	_, open := <-out
	assert.False(t, open, "output channel must be closed after consumer returns")
}
