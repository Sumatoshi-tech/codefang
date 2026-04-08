//go:build integration

package analyze_test

// FRD: specs/frds/FRD-20260312-static-budget-integration-test.md.

import (
	"context"
	"runtime/debug"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/budget"
	"github.com/Sumatoshi-tech/codefang/pkg/units"
)

// budgetTestFileCount is the number of synthetic Go files to generate.
const budgetTestFileCount = 5000

// budgetTestFunctionsPerFile is the number of functions per synthetic file.
const budgetTestFunctionsPerFile = 50

// budgetTestBudgetBytes is the memory budget for the integration test (512 MiB).
const budgetTestBudgetBytes = 512 * units.MiB

// budgetTestPeakLimit is the maximum allowed peak heap (2× budget = 1 GiB).
const budgetTestPeakLimit = 1024

// budgetTestHeapSampleInterval is the heap sampling interval.
// Matches heapSampleInterval from static_bench_test.go.
const budgetTestHeapSampleInterval = heapSampleInterval

// budgetTestAnalyzerCount is the expected number of analyzers producing results.
const budgetTestAnalyzerCount = 5

// TestStaticAnalyzers_MemoryBudget verifies that static analysis on a large
// synthetic codebase stays within 2× the memory budget.
//
// This is a pass/fail gate, not a comparative benchmark.
// Run with: go test -tags integration -run TestStaticAnalyzers_MemoryBudget ./internal/analyzers/analyze/...
func TestStaticAnalyzers_MemoryBudget(t *testing.T) {
	t.Parallel()

	dir := setupHeavyBenchDir(t, budgetTestFileCount, budgetTestFunctionsPerFile)

	svc := analyze.NewStaticService(testStaticAnalyzers(), nil)
	svc.NativeMemoryReleaseFn = func() {} // Skip real malloc_trim in test.

	// Apply budget-derived parameters.
	cfg := budget.SolveStaticBudget(budgetTestBudgetBytes)
	require.NotZero(t, cfg.MaxWorkers, "budget solver should derive workers")

	svc.MaxWorkers = cfg.MaxWorkers
	svc.SpillThreshold = cfg.SpillThreshold
	svc.AggregationMode = analyze.AggregationModeSummaryOnly

	// Engage Go GC self-regulation.
	prev := debug.SetMemoryLimit(budgetTestBudgetBytes)
	defer debug.SetMemoryLimit(prev)

	// Start heap sampling.
	sampler := newHeapSampler()

	results, err := svc.AnalyzeFolder(context.Background(), dir, nil)

	peakMiB := sampler.stopAndGet()

	// Assert: no error.
	require.NoError(t, err, "AnalyzeFolder should complete without error")

	// Assert: all analyzers produced results.
	assert.Len(t, results, budgetTestAnalyzerCount,
		"all analyzers should produce results")

	for name, report := range results {
		assert.NotNil(t, report, "report for %s should not be nil", name)
	}

	// Assert: peak heap within budget.
	t.Logf("peak heap: %.0f MiB (limit: %d MiB)", peakMiB, budgetTestPeakLimit)

	assert.LessOrEqual(t, peakMiB, float64(budgetTestPeakLimit),
		"peak heap should stay within 2x budget")
}
