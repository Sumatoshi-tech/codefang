package pipeline_test

// FRD: specs/frds/FRD-20260302-composable-pipeline-patterns.md.

import (
	"context"
	"errors"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/pipeline"
)

func TestRunPhases_Empty(t *testing.T) {
	t.Parallel()

	const initial = 42

	result, err := pipeline.RunPhases[int](context.Background(), initial)

	require.NoError(t, err)
	assert.Equal(t, initial, result)
}

func TestRunPhases_SinglePhase(t *testing.T) {
	t.Parallel()

	const initial = 10

	double := pipeline.PhaseFunc[int](func(_ context.Context, s int) (int, error) {
		return s * 2, nil
	})

	result, err := pipeline.RunPhases(context.Background(), initial, double)

	require.NoError(t, err)
	assert.Equal(t, 20, result)
}

func TestRunPhases_MultiplePhases(t *testing.T) {
	t.Parallel()

	const initial = 1

	add := pipeline.PhaseFunc[int](func(_ context.Context, s int) (int, error) {
		return s + 10, nil
	})

	multiply := pipeline.PhaseFunc[int](func(_ context.Context, s int) (int, error) {
		return s * 3, nil
	})

	subtract := pipeline.PhaseFunc[int](func(_ context.Context, s int) (int, error) {
		return s - 5, nil
	})

	// Expected: add(1)=11, multiply(11)=33, subtract(33)=28.
	result, err := pipeline.RunPhases(context.Background(), initial, add, multiply, subtract)

	require.NoError(t, err)
	assert.Equal(t, 28, result)
}

var errPhase = errors.New("phase failed")

func TestRunPhases_ErrorStopsChain(t *testing.T) {
	t.Parallel()

	const initial = 5

	add := pipeline.PhaseFunc[int](func(_ context.Context, s int) (int, error) {
		return s + 1, nil
	})

	fail := pipeline.PhaseFunc[int](func(_ context.Context, s int) (int, error) {
		return s, errPhase
	})

	unreachable := pipeline.PhaseFunc[int](func(_ context.Context, _ int) (int, error) {
		t.Fatal("this phase must not be reached")

		return 0, nil
	})

	result, err := pipeline.RunPhases(context.Background(), initial, add, fail, unreachable)

	require.ErrorIs(t, err, errPhase)
	// Partial state: add executed (5→6), fail returned 6 with error.
	assert.Equal(t, 6, result)
}

func TestRunPhases_ContextPropagated(t *testing.T) {
	t.Parallel()

	ctx, cancel := context.WithCancel(context.Background())
	cancel() // Already canceled.

	phase := pipeline.PhaseFunc[int](func(ctx context.Context, s int) (int, error) {
		return s, ctx.Err()
	})

	_, err := pipeline.RunPhases(ctx, 0, phase)
	require.ErrorIs(t, err, context.Canceled)
}

func TestPhaseFunc_SatisfiesInterface(t *testing.T) {
	t.Parallel()

	var p pipeline.Phase[string] = pipeline.PhaseFunc[string](func(_ context.Context, s string) (string, error) {
		return s + "!", nil
	})

	result, err := p.Run(context.Background(), "hello")

	require.NoError(t, err)
	assert.Equal(t, "hello!", result)
}
