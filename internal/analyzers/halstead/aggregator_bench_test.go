package halstead

import (
	"fmt"
	"testing"

	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
)

// Benchmark constants.
const (
	benchFileCount          = 1000
	benchFuncsPerFile       = 4
	benchExpectedTotalFuncs = benchFileCount * benchFuncsPerFile
)

// benchFuncNames are the common duplicate function names.
var benchFuncNames = [benchFuncsPerFile]string{"init", "main", "New", "Close"}

// BenchmarkHalsteadDedup verifies that composite identifier keys prevent
// cross-file overwrites of duplicate function names.
// Before (single "name" key): 1000 files × 4 funcs → only 4 collected (overwritten).
// After (composite key): 1000 files × 4 funcs → all 4000 collected.
func BenchmarkHalsteadDedup(b *testing.B) {
	reports := buildDedupReports(b, benchFileCount)

	b.ResetTimer()
	b.ReportAllocs()

	for b.Loop() {
		agg := NewAggregator()

		for _, report := range reports {
			agg.Aggregate(report)
		}

		result := agg.GetResult()

		functions, ok := result["functions"].([]map[string]any)
		require.True(b, ok, "functions must be []map[string]any")
		b.ReportMetric(float64(len(functions)), "items-collected")
	}
}

// buildDedupReports creates synthetic per-file reports with duplicate function names.
func buildDedupReports(b *testing.B, fileCount int) []map[string]analyze.Report {
	b.Helper()

	reports := make([]map[string]analyze.Report, 0, fileCount)

	for i := range fileCount {
		sourceFile := fmt.Sprintf("pkg/mod%d/file%d.go", i/100, i)
		functions := make([]map[string]any, 0, benchFuncsPerFile)

		for _, name := range benchFuncNames {
			functions = append(functions, map[string]any{
				"name":         name,
				"_source_file": sourceFile,
				"volume":       100.0,
				"difficulty":   5.0,
				"effort":       500.0,
			})
		}

		report := analyze.Report{
			"total_functions": benchFuncsPerFile,
			"volume":          100.0,
			"difficulty":      5.0,
			"effort":          500.0,
			"functions":       functions,
		}

		reports = append(reports, map[string]analyze.Report{"halstead": report})
	}

	return reports
}
