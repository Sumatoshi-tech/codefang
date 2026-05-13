package common

import (
	"fmt"
	"runtime"
	"testing"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
)

// benchReportCount is the number of synthetic reports fed to the aggregator.
const benchReportCount = 50000

// benchFunctionsPerReport is the number of function items per report.
const benchFunctionsPerReport = 10

// benchBytesPerMiB converts bytes to mebibytes.
const benchBytesPerMiB = 1024 * 1024

// makeSyntheticReport creates a complexity-style report with n function items.
func makeSyntheticReport(fileIndex, numFunctions int) analyze.Report {
	functions := make([]map[string]any, numFunctions)

	for j := range numFunctions {
		functions[j] = map[string]any{
			"name":                 fmt.Sprintf("file%d_func%d", fileIndex, j),
			"cognitive_complexity": j + 1,
			"nesting_depth":        j % 5,
			"_source_file":         fmt.Sprintf("/repo/pkg/mod%d/file%d.go", fileIndex/100, fileIndex),
		}
	}

	return analyze.Report{
		"cognitive_complexity": 5.0,
		"nesting_depth":        2.0,
		"total_functions":      numFunctions,
		"total_complexity":     numFunctions * 3,
		"decision_points":      numFunctions * 2,
		"functions":            functions,
	}
}

// testFunctionMetrics is a typed struct for benchmark comparison.
type testFunctionMetrics struct {
	Name                 string
	CyclomaticComplexity int
	CognitiveComplexity  int
	NestingDepth         int
	LinesOfCode          int
	ComplexityAssessment string
	CognitiveAssessment  string
}

// testFunctionConverter converts typed items to maps.
func testFunctionConverter(items any, sourceFile string) []map[string]any {
	typed, ok := items.([]testFunctionMetrics)
	if !ok {
		return nil
	}

	result := make([]map[string]any, 0, len(typed))

	for _, fn := range typed {
		m := map[string]any{
			"name":                  fn.Name,
			"cyclomatic_complexity": fn.CyclomaticComplexity,
			"cognitive_complexity":  fn.CognitiveComplexity,
			"nesting_depth":         fn.NestingDepth,
			"lines_of_code":         fn.LinesOfCode,
			"complexity_assessment": fn.ComplexityAssessment,
			"cognitive_assessment":  fn.CognitiveAssessment,
		}
		if sourceFile != "" {
			m["_source_file"] = sourceFile
		}

		result = append(result, m)
	}

	return result
}

// benchTypedFileCount is the number of files for the typed vs map benchmark.
const benchTypedFileCount = 5000

// benchTypedFuncsPerFile is the number of functions per file.
const benchTypedFuncsPerFile = 10

// BenchmarkTypedVsMapAccumulation compares DetailedDataCollector accumulation
// with legacy []map[string]any vs TypedCollection.
// Step 3.2: typed allocs/op < 1/3 of map allocs/op.
func BenchmarkTypedVsMapAccumulation(b *testing.B) {
	b.Run("before-map-accumulation", func(b *testing.B) {
		b.ReportAllocs()

		var heapDelta float64

		for b.Loop() {
			dc := NewDetailedDataCollector("functions")

			runtime.GC()

			var before runtime.MemStats

			runtime.ReadMemStats(&before)

			for i := range benchTypedFileCount {
				functions := make([]map[string]any, 0, benchTypedFuncsPerFile)

				for j := range benchTypedFuncsPerFile {
					functions = append(functions, map[string]any{
						"name":                  fmt.Sprintf("file%d_func%d", i, j),
						"cyclomatic_complexity": j + 1,
						"cognitive_complexity":  j % 8,
						"nesting_depth":         j % 5,
						"lines_of_code":         j%100 + 10,
						"complexity_assessment": "ok",
						"cognitive_assessment":  "ok",
					})
				}

				report := analyze.Report{"functions": functions}
				dc.CollectFromReports(map[string]analyze.Report{"complexity": report})
			}

			var after runtime.MemStats

			runtime.ReadMemStats(&after)
			heapDelta = float64(after.HeapInuse-before.HeapInuse) / benchBytesPerMiB
		}

		b.ReportMetric(heapDelta, "heap-MiB")
	})

	b.Run("after-typed-accumulation", func(b *testing.B) {
		b.ReportAllocs()

		var heapDelta float64

		for b.Loop() {
			dc := NewDetailedDataCollector("functions")

			runtime.GC()

			var before runtime.MemStats

			runtime.ReadMemStats(&before)

			for i := range benchTypedFileCount {
				functions := make([]testFunctionMetrics, 0, benchTypedFuncsPerFile)

				for j := range benchTypedFuncsPerFile {
					functions = append(functions, testFunctionMetrics{
						Name:                 fmt.Sprintf("file%d_func%d", i, j),
						CyclomaticComplexity: j + 1,
						CognitiveComplexity:  j % 8,
						NestingDepth:         j % 5,
						LinesOfCode:          j%100 + 10,
						ComplexityAssessment: "ok",
						CognitiveAssessment:  "ok",
					})
				}

				tc := analyze.TypedCollection{
					Items:  functions,
					ToMaps: testFunctionConverter,
				}
				report := analyze.Report{"functions": tc}
				dc.CollectFromReports(map[string]analyze.Report{"complexity": report})
			}

			var after runtime.MemStats

			runtime.ReadMemStats(&after)
			heapDelta = float64(after.HeapInuse-before.HeapInuse) / benchBytesPerMiB
		}

		b.ReportMetric(heapDelta, "heap-MiB")
	})
}

// benchEstimatedSizeReportCount is the number of reports for size estimation benchmark.
const benchEstimatedSizeReportCount = 10000

// BenchmarkAggregatorEstimatedSize verifies EstimatedStateSize grows linearly with items.
func BenchmarkAggregatorEstimatedSize(b *testing.B) {
	b.ReportAllocs()

	var lastEstimated float64

	for b.Loop() {
		agg := NewAggregator(
			"complexity",
			[]string{"cognitive_complexity", "nesting_depth"},
			[]string{"total_functions", "total_complexity", "decision_points"},
			"functions", []string{"name"},
			nil, nil,
		)

		agg.SetAggregationMode(analyze.AggregationModeFull)
		agg.SetSpillThreshold(0) // Disable spilling for accurate estimation.

		runtime.GC()

		var before runtime.MemStats

		runtime.ReadMemStats(&before)

		for i := range benchEstimatedSizeReportCount {
			report := makeSyntheticReport(i, benchFunctionsPerReport)
			agg.Aggregate(map[string]analyze.Report{"complexity": report})
		}

		var after runtime.MemStats

		runtime.ReadMemStats(&after)

		estimatedMiB := float64(agg.EstimatedStateSize()) / benchBytesPerMiB
		actualMiB := float64(after.HeapInuse-before.HeapInuse) / benchBytesPerMiB
		lastEstimated = estimatedMiB

		b.ReportMetric(estimatedMiB, "estimated-MiB")
		b.ReportMetric(actualMiB, "actual-MiB")
	}

	// Prevent compiler from eliding.
	_ = lastEstimated
}

// BenchmarkAggregatorSummaryMode measures heap delta in Full vs SummaryOnly mode.
// Step 2.1: SummaryOnly heap delta < 1 MiB; Full mode heap delta > 50 MiB.
func BenchmarkAggregatorSummaryMode(b *testing.B) {
	b.Run("before-full-mode", func(b *testing.B) {
		b.ReportAllocs()

		var heapDelta float64

		for b.Loop() {
			agg := NewAggregator(
				"complexity",
				[]string{"cognitive_complexity", "nesting_depth"},
				[]string{"total_functions", "total_complexity", "decision_points"},
				"functions", []string{"name"},
				nil, nil,
			)

			agg.SetAggregationMode(analyze.AggregationModeFull)

			runtime.GC()

			var before runtime.MemStats

			runtime.ReadMemStats(&before)

			for i := range benchReportCount {
				report := makeSyntheticReport(i, benchFunctionsPerReport)
				agg.Aggregate(map[string]analyze.Report{"complexity": report})
			}

			var after runtime.MemStats

			runtime.ReadMemStats(&after)
			heapDelta = float64(after.HeapInuse-before.HeapInuse) / benchBytesPerMiB
		}

		b.ReportMetric(heapDelta, "heap-MiB")
	})

	b.Run("after-summary-only", func(b *testing.B) {
		b.ReportAllocs()

		var heapDelta float64

		for b.Loop() {
			agg := NewAggregator(
				"complexity",
				[]string{"cognitive_complexity", "nesting_depth"},
				[]string{"total_functions", "total_complexity", "decision_points"},
				"functions", []string{"name"},
				nil, nil,
			)

			agg.SetAggregationMode(analyze.AggregationModeSummaryOnly)

			runtime.GC()

			var before runtime.MemStats

			runtime.ReadMemStats(&before)

			for i := range benchReportCount {
				report := makeSyntheticReport(i, benchFunctionsPerReport)
				agg.Aggregate(map[string]analyze.Report{"complexity": report})
			}

			var after runtime.MemStats

			runtime.ReadMemStats(&after)
			heapDelta = float64(after.HeapInuse-before.HeapInuse) / benchBytesPerMiB
		}

		b.ReportMetric(heapDelta, "heap-MiB")
	})
}
