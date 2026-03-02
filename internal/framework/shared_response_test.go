package framework_test

// FRD: specs/frds/FRD-20260302-shared-response.md.

import (
	"context"
	"errors"
	"sync"
	"sync/atomic"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/internal/framework"
)

var errComputeFailed = errors.New("compute failed")

func TestSharedResponse_Get_ReturnsComputedValue(t *testing.T) {
	t.Parallel()

	const want = 42

	sr := framework.NewSharedResponse(func(_ context.Context) (int, error) {
		return want, nil
	})

	got, err := sr.Get(context.Background())
	require.NoError(t, err)
	assert.Equal(t, want, got)
}

func TestSharedResponse_Get_ReturnsError(t *testing.T) {
	t.Parallel()

	sr := framework.NewSharedResponse(func(_ context.Context) (string, error) {
		return "", errComputeFailed
	})

	got, err := sr.Get(context.Background())
	require.ErrorIs(t, err, errComputeFailed)
	assert.Empty(t, got)
}

func TestSharedResponse_Get_EvaluatesOnce(t *testing.T) {
	t.Parallel()

	var calls atomic.Int64

	sr := framework.NewSharedResponse(func(_ context.Context) (int, error) {
		calls.Add(1)

		return 1, nil
	})

	const concurrency = 100

	var wg sync.WaitGroup

	wg.Add(concurrency)

	for range concurrency {
		go func() {
			defer wg.Done()

			v, err := sr.Get(context.Background())
			assert.NoError(t, err)
			assert.Equal(t, 1, v)
		}()
	}

	wg.Wait()

	assert.Equal(t, int64(1), calls.Load())
}

func TestSharedResponse_Get_CancelledContext(t *testing.T) {
	t.Parallel()

	ctx, cancel := context.WithCancel(context.Background())
	cancel()

	sr := framework.NewSharedResponse(func(ctx context.Context) (int, error) {
		return 0, ctx.Err()
	})

	_, err := sr.Get(ctx)
	require.ErrorIs(t, err, context.Canceled)
}

func TestSharedResponse_Get_CachesResultAcrossCalls(t *testing.T) {
	t.Parallel()

	const want = 7

	sr := framework.NewSharedResponse(func(_ context.Context) (int, error) {
		return want, nil
	})

	got1, err1 := sr.Get(context.Background())
	got2, err2 := sr.Get(context.Background())

	require.NoError(t, err1)
	require.NoError(t, err2)
	assert.Equal(t, want, got1)
	assert.Equal(t, want, got2)
}

func TestSharedResponse_Get_CachesErrorAcrossCalls(t *testing.T) {
	t.Parallel()

	sr := framework.NewSharedResponse(func(_ context.Context) (int, error) {
		return 0, errComputeFailed
	})

	_, err1 := sr.Get(context.Background())
	_, err2 := sr.Get(context.Background())

	require.ErrorIs(t, err1, errComputeFailed)
	require.ErrorIs(t, err2, errComputeFailed)
}
