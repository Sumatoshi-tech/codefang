package budget

import (
	"runtime"

	"github.com/Sumatoshi-tech/codefang/pkg/units"
)

// Static analysis cost model constants (empirically measured).
const (
	// StaticBaseOverhead is the fixed Go runtime + loaded analyzers overhead.
	// Lower than history's BaseOverhead because no libgit2 repo is opened.
	StaticBaseOverhead = 150 * units.MiB

	// StaticWorkerFootprint is the per-worker memory for parser + tree-sitter
	// native tree + Go-side node.Node tree + file content buffer.
	StaticWorkerFootprint = 50 * units.MiB

	// StaticAvgItemBytes is the average gob-encoded size of a report item
	// (map[string]any with ~8 keys). Used to estimate spill threshold.
	StaticAvgItemBytes = 512

	// StaticAnalyzerCount is the number of static analyzers that use
	// SpillableDataCollector (complexity, halstead, comments, cohesion, clones, imports).
	StaticAnalyzerCount = 6

	// MinStaticBudget is the smallest budget that produces a non-zero config.
	// Must cover base overhead plus at least one worker.
	MinStaticBudget = StaticBaseOverhead + StaticWorkerFootprint + 10*units.MiB

	// MaxStaticWorkers caps workers even with large budgets.
	MaxStaticWorkers = 16

	// MinStaticSpillThreshold is the floor for spill threshold.
	MinStaticSpillThreshold = 1000

	// MaxStaticSpillThreshold is the ceiling for spill threshold.
	MaxStaticSpillThreshold = 100000
)

// StaticBudgetConfig holds budget-derived parameters for the static analysis phase.
// Zero values mean "use defaults" — no override applied.
type StaticBudgetConfig struct {
	MaxWorkers     int
	SpillThreshold int
}

// SolveStaticBudget derives static analysis parameters from a memory budget.
// Returns zero-value config when budget is zero, negative, or below minimum.
func SolveStaticBudget(budgetBytes int64) StaticBudgetConfig {
	if budgetBytes < MinStaticBudget {
		return StaticBudgetConfig{}
	}

	usable := budgetBytes * (percentDivisor - SlackPercent) / percentDivisor
	available := usable - StaticBaseOverhead

	if available <= 0 {
		return StaticBudgetConfig{}
	}

	workers := solveStaticWorkers(available)
	workerAlloc := int64(workers) * StaticWorkerFootprint
	remaining := available - workerAlloc
	spillThreshold := solveStaticSpillThreshold(remaining)

	return StaticBudgetConfig{
		MaxWorkers:     workers,
		SpillThreshold: spillThreshold,
	}
}

// solveStaticWorkers computes the number of workers from available memory.
func solveStaticWorkers(available int64) int {
	cpuCap := min(runtime.NumCPU(), MaxStaticWorkers)
	budgetWorkers := int(available / StaticWorkerFootprint)

	return max(1, min(budgetWorkers, cpuCap))
}

// solveStaticSpillThreshold computes the spill threshold from remaining memory
// after worker allocation.
func solveStaticSpillThreshold(remaining int64) int {
	if remaining <= 0 {
		return MinStaticSpillThreshold
	}

	perAnalyzer := remaining / StaticAnalyzerCount
	threshold := int(perAnalyzer / StaticAvgItemBytes)

	return max(MinStaticSpillThreshold, min(threshold, MaxStaticSpillThreshold))
}
