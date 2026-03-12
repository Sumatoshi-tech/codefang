package common

// FRD: specs/frds/FRD-20260311-spillable-data-collector.md.

import (
	"fmt"
	"runtime"
	"testing"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
)

// spillBenchItemCount is the number of synthetic items fed to the collector.
const spillBenchItemCount = 100000

// spillBenchThreshold is the spill threshold for the after-spillable benchmark.
const spillBenchThreshold = 5000

// spillBenchFieldCount is the number of fields per synthetic item.
const spillBenchFieldCount = 7

// makeSyntheticItems creates a report with n function items, each ~500 bytes.
func makeSyntheticItems(startIndex, count int) analyze.Report {
	functions := make([]map[string]any, count)

	for j := range count {
		idx := startIndex + j

		functions[j] = map[string]any{
			"name":                 fmt.Sprintf("pkg%d/file%d.go:func_%d", idx/1000, idx/10, idx),
			"cognitive_complexity": idx%20 + 1,
			"nesting_depth":        idx % spillBenchFieldCount,
			"_source_file":         fmt.Sprintf("/repo/pkg/mod%d/submod%d/file%d.go", idx/10000, idx/1000, idx/10),
			"lines":                idx%200 + 10,
			"start_line":           idx * 50,
			"decision_points":      idx%10 + 1,
		}
	}

	return analyze.Report{
		"cognitive_complexity": 5.0,
		"nesting_depth":        2.0,
		"total_functions":      count,
		"functions":            functions,
	}
}

// BenchmarkSpillableCollector measures peak heap with plain DataCollector vs
// SpillableDataCollector with spill threshold.
// Step 2.2: spillable peak heap < 20 MiB; plain peak heap > 80 MiB.
func BenchmarkSpillableCollector(b *testing.B) {
	b.Run("before-no-spill", func(b *testing.B) {
		b.ReportAllocs()

		var heapDelta float64

		for b.Loop() {
			sdc := NewSpillableDataCollector("functions", "name", testSpillZero)
			sdc.SetAggregationMode(analyze.AggregationModeFull)

			runtime.GC()

			var before runtime.MemStats

			runtime.ReadMemStats(&before)

			for i := 0; i < spillBenchItemCount; i += spillBenchThreshold {
				count := min(spillBenchThreshold, spillBenchItemCount-i)

				report := makeSyntheticItems(i, count)
				sdc.CollectFromReport(report)
			}

			var after runtime.MemStats

			runtime.ReadMemStats(&after)
			heapDelta = float64(after.HeapInuse-before.HeapInuse) / benchBytesPerMiB
		}

		b.ReportMetric(heapDelta, "heap-MiB")
	})

	b.Run("after-spillable", func(b *testing.B) {
		b.ReportAllocs()

		var heapDelta float64

		for b.Loop() {
			sdc := NewSpillableDataCollector("functions", "name", spillBenchThreshold)
			sdc.SetAggregationMode(analyze.AggregationModeFull)

			runtime.GC()

			var before runtime.MemStats

			runtime.ReadMemStats(&before)

			for i := 0; i < spillBenchItemCount; i += spillBenchThreshold {
				count := min(spillBenchThreshold, spillBenchItemCount-i)

				report := makeSyntheticItems(i, count)
				sdc.CollectFromReport(report)
			}

			var after runtime.MemStats

			runtime.ReadMemStats(&after)
			heapDelta = float64(after.HeapInuse-before.HeapInuse) / benchBytesPerMiB

			sdc.Cleanup()
		}

		b.ReportMetric(heapDelta, "heap-MiB")
	})
}
