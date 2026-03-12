package budget

// FRD: specs/frds/FRD-20260312-static-budget-tuning.md.

import (
	"runtime"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/units"
)

func TestSolveStaticBudget_ZeroBudget(t *testing.T) {
	t.Parallel()

	cfg := SolveStaticBudget(0)

	assert.Zero(t, cfg.MaxWorkers)
	assert.Zero(t, cfg.SpillThreshold)
}

func TestSolveStaticBudget_NegativeBudget(t *testing.T) {
	t.Parallel()

	cfg := SolveStaticBudget(-1)

	assert.Zero(t, cfg.MaxWorkers)
	assert.Zero(t, cfg.SpillThreshold)
}

func TestSolveStaticBudget_BelowMinimum(t *testing.T) {
	t.Parallel()

	cfg := SolveStaticBudget(MinStaticBudget - 1)

	assert.Zero(t, cfg.MaxWorkers)
	assert.Zero(t, cfg.SpillThreshold)
}

func TestSolveStaticBudget_AtMinimum(t *testing.T) {
	t.Parallel()

	cfg := SolveStaticBudget(MinStaticBudget)

	assert.GreaterOrEqual(t, cfg.MaxWorkers, 1)
	assert.GreaterOrEqual(t, cfg.SpillThreshold, MinStaticSpillThreshold)
}

func TestSolveStaticBudget_MediumBudget(t *testing.T) {
	t.Parallel()

	const budget = 1 * units.GiB

	cfg := SolveStaticBudget(budget)

	assert.GreaterOrEqual(t, cfg.MaxWorkers, 1)
	assert.LessOrEqual(t, cfg.MaxWorkers, MaxStaticWorkers)
	assert.GreaterOrEqual(t, cfg.SpillThreshold, MinStaticSpillThreshold)
	assert.LessOrEqual(t, cfg.SpillThreshold, MaxStaticSpillThreshold)
}

func TestSolveStaticBudget_LargeBudget(t *testing.T) {
	t.Parallel()

	const budget = 4 * units.GiB

	cfg := SolveStaticBudget(budget)

	// With 4 GiB, workers should be capped at MaxStaticWorkers or NumCPU.
	maxExpected := min(runtime.NumCPU(), MaxStaticWorkers)
	assert.LessOrEqual(t, cfg.MaxWorkers, maxExpected)

	// Spill threshold should be at max.
	assert.Equal(t, MaxStaticSpillThreshold, cfg.SpillThreshold)
}

func TestSolveStaticBudget_WorkersScaleWithBudget(t *testing.T) {
	t.Parallel()

	smallCfg := SolveStaticBudget(MinStaticBudget)
	largeCfg := SolveStaticBudget(4 * units.GiB)

	assert.GreaterOrEqual(t, largeCfg.MaxWorkers, smallCfg.MaxWorkers,
		"larger budget should allow at least as many workers")
}

func TestSolveStaticBudget_SpillScalesWithBudget(t *testing.T) {
	t.Parallel()

	smallCfg := SolveStaticBudget(MinStaticBudget)
	largeCfg := SolveStaticBudget(4 * units.GiB)

	assert.GreaterOrEqual(t, largeCfg.SpillThreshold, smallCfg.SpillThreshold,
		"larger budget should allow at least as large a spill threshold")
}

func TestSolveStaticBudget_WorkersCappedByCPU(t *testing.T) {
	t.Parallel()

	// Even with unlimited budget, workers must not exceed min(NumCPU, MaxStaticWorkers).
	cfg := SolveStaticBudget(16 * units.GiB)

	maxExpected := min(runtime.NumCPU(), MaxStaticWorkers)

	require.LessOrEqual(t, cfg.MaxWorkers, maxExpected)
}
