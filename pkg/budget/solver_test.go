package budget

import (
	"runtime"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestSolveForBudget_MediumBudget(t *testing.T) {
	t.Parallel()

	const budgetOneGiB = 1 * GiB

	cfg, err := SolveForBudget(budgetOneGiB)

	require.NoError(t, err)
	assert.Positive(t, cfg.Workers, "should have at least 1 worker")
	assert.Positive(t, cfg.BufferSize, "should have positive buffer size")
	assert.Positive(t, cfg.BlobCacheSize, "should have positive blob cache")
	assert.Positive(t, cfg.DiffCacheSize, "should have positive diff cache")
	assert.Positive(t, cfg.BlobArenaSize, "should have positive arena size")
}

func TestSolveForBudget_SmallBudget(t *testing.T) {
	t.Parallel()

	const budget256MiB = 256 * MiB

	cfg, err := SolveForBudget(budget256MiB)

	require.NoError(t, err)
	assert.GreaterOrEqual(t, cfg.Workers, MinWorkers, "should have minimum workers")
	assert.GreaterOrEqual(t, cfg.BufferSize, MinBufferSize, "should have minimum buffer")
}

func TestSolveForBudget_LargeBudget(t *testing.T) {
	t.Parallel()

	const budget4GiB = 4 * GiB

	cfg, err := SolveForBudget(budget4GiB)

	require.NoError(t, err)
	// Larger budget should allow more resources.
	assert.Positive(t, cfg.Workers)
	assert.Greater(t, cfg.BlobCacheSize, int64(100*MiB), "large budget should have significant cache")
}

func TestSolveForBudget_TooSmall(t *testing.T) {
	t.Parallel()

	const tinyBudget = 64 * MiB // Below MinimumBudget

	_, err := SolveForBudget(tinyBudget)

	require.Error(t, err)
	assert.ErrorIs(t, err, ErrBudgetTooSmall)
}

func TestSolveForBudget_ExactlyMinimum(t *testing.T) {
	t.Parallel()

	cfg, err := SolveForBudget(MinimumBudget)

	require.NoError(t, err)
	assert.Positive(t, cfg.Workers, "should work at minimum budget")
}

func TestSolveForBudget_NeverExceedsBudget(t *testing.T) {
	t.Parallel()

	budgets := []int64{
		MinimumBudget,
		256 * MiB,
		512 * MiB,
		1 * GiB,
		2 * GiB,
		4 * GiB,
	}

	for _, budget := range budgets {
		cfg, err := SolveForBudget(budget)
		require.NoError(t, err, "budget %d should succeed", budget)

		estimate := EstimateMemoryUsage(cfg)
		assert.LessOrEqual(t, estimate, budget,
			"estimate %d should not exceed budget %d", estimate, budget)
	}
}

func TestSolveForBudget_MaintainsSlack(t *testing.T) {
	t.Parallel()

	// Fuzz-style test: verify solver maintains >5% slack across many budgets.
	// This ensures we never get too close to the limit.
	const slackPercent = 5

	// Test a range of budgets from minimum to 8 GiB in increments.
	for budget := int64(MinimumBudget); budget <= 8*GiB; budget += 64 * MiB {
		cfg, err := SolveForBudget(budget)
		require.NoError(t, err, "budget %d should succeed", budget)

		estimate := EstimateMemoryUsage(cfg)
		maxAllowed := budget * (percentDivisor - slackPercent) / percentDivisor

		assert.LessOrEqual(t, estimate, maxAllowed,
			"estimate %d should be <= %d (budget %d with %d%% slack)",
			estimate, maxAllowed, budget, slackPercent)
	}
}

func TestSolveForBudget_Deterministic(t *testing.T) {
	t.Parallel()

	const budget = 1 * GiB

	cfg1, err1 := SolveForBudget(budget)
	cfg2, err2 := SolveForBudget(budget)

	require.NoError(t, err1)
	require.NoError(t, err2)

	assert.Equal(t, cfg1.Workers, cfg2.Workers)
	assert.Equal(t, cfg1.BufferSize, cfg2.BufferSize)
	assert.Equal(t, cfg1.BlobCacheSize, cfg2.BlobCacheSize)
	assert.Equal(t, cfg1.DiffCacheSize, cfg2.DiffCacheSize)
	assert.Equal(t, cfg1.BlobArenaSize, cfg2.BlobArenaSize)
}

func TestSolveForBudget_LargerBudgetMoreResources(t *testing.T) {
	t.Parallel()

	smallCfg, err := SolveForBudget(256 * MiB)
	require.NoError(t, err)

	largeCfg, err := SolveForBudget(2 * GiB)
	require.NoError(t, err)

	// Larger budget should provide more resources.
	assert.GreaterOrEqual(t, largeCfg.BlobCacheSize, smallCfg.BlobCacheSize,
		"larger budget should have larger or equal blob cache")
	assert.GreaterOrEqual(t, largeCfg.DiffCacheSize, smallCfg.DiffCacheSize,
		"larger budget should have larger or equal diff cache")
}

func TestSolveForBudget_WorkersCappedAtCPUCount(t *testing.T) {
	t.Parallel()

	// Very large budget that would allow more workers than CPUs.
	const hugeBudget = 64 * GiB

	cfg, err := SolveForBudget(hugeBudget)

	require.NoError(t, err)
	// Workers should be capped at CPU count.
	assert.LessOrEqual(t, cfg.Workers, runtime.NumCPU(),
		"workers should not exceed CPU count")
}

func TestSolveForBudget_MinimumValuesEnforced(t *testing.T) {
	t.Parallel()

	// Use minimum budget to trigger minimum value enforcement.
	cfg, err := SolveForBudget(MinimumBudget)

	require.NoError(t, err)
	assert.GreaterOrEqual(t, cfg.Workers, MinWorkers, "should enforce min workers")
	assert.GreaterOrEqual(t, cfg.BufferSize, MinBufferSize, "should enforce min buffer")
	assert.GreaterOrEqual(t, cfg.DiffCacheSize, MinDiffCacheSize, "should enforce min diff cache")
	assert.GreaterOrEqual(t, cfg.BlobCacheSize, int64(MinBlobCacheSize), "should enforce min blob cache")
}

func TestDeriveKnobs_ZeroAllocations(t *testing.T) {
	t.Parallel()

	// Test deriveKnobs with zero allocations to trigger all minimum value branches.
	cfg := deriveKnobs(0, 0, 0)

	assert.Equal(t, MinWorkers, cfg.Workers, "should use min workers")
	assert.Equal(t, MinBufferSize, cfg.BufferSize, "should use min buffer")
	assert.Equal(t, MinDiffCacheSize, cfg.DiffCacheSize, "should use min diff cache")
	assert.Equal(t, int64(MinBlobCacheSize), cfg.BlobCacheSize, "should use min blob cache")
}

func TestDeriveKnobs_TinyAllocations(t *testing.T) {
	t.Parallel()

	// Small allocations that trigger minimum enforcement.
	cfg := deriveKnobs(1*KiB, 1*KiB, 1*KiB)

	assert.GreaterOrEqual(t, cfg.Workers, MinWorkers)
	assert.GreaterOrEqual(t, cfg.BufferSize, MinBufferSize)
	assert.GreaterOrEqual(t, cfg.DiffCacheSize, MinDiffCacheSize)
	assert.GreaterOrEqual(t, cfg.BlobCacheSize, int64(MinBlobCacheSize))
}

func TestDeriveKnobs_HugeWorkerAllocation(t *testing.T) {
	t.Parallel()

	// Huge worker allocation that would exceed CPU count.
	cfg := deriveKnobs(100*MiB, 100*GiB, 10*MiB)

	assert.LessOrEqual(t, cfg.Workers, runtime.NumCPU(), "workers capped at CPU count")
}
