package pipeline_test

import (
	"context"
	"errors"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/pipeline"
)

func TestDispatchFunc_Success(t *testing.T) {
	t.Parallel()

	var dispatched string

	dispatch := pipeline.DispatchFunc[string](func(_ context.Context, req string) error {
		dispatched = req

		return nil
	})

	err := dispatch(context.Background(), "hello")

	require.NoError(t, err)
	assert.Equal(t, "hello", dispatched)
}

var errDispatch = errors.New("dispatch failed")

func TestDispatchFunc_Error(t *testing.T) {
	t.Parallel()

	dispatch := pipeline.DispatchFunc[int](func(_ context.Context, _ int) error {
		return errDispatch
	})

	err := dispatch(context.Background(), 42)
	require.ErrorIs(t, err, errDispatch)
}

func TestDispatchFunc_ContextCancelled(t *testing.T) {
	t.Parallel()

	ctx, cancel := context.WithCancel(context.Background())
	cancel()

	dispatch := pipeline.DispatchFunc[int](func(ctx context.Context, _ int) error {
		return ctx.Err()
	})

	err := dispatch(ctx, 0)
	require.ErrorIs(t, err, context.Canceled)
}
