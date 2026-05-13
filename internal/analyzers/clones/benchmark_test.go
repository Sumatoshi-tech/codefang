package clones

import (
	"fmt"
	"runtime"
	"testing"

	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/alg/minhash"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

// Benchmark constants.
const (
	benchFunctionCount   = 100
	benchChildCountLarge = 20
	benchShingleSize     = 5

	// benchPairCapFileCount is the number of synthetic files for pair cap benchmarks.
	benchPairCapFileCount = 500

	// benchPairCapCapped is the pair cap used in the "after" sub-benchmark.
	benchPairCapCapped = 100

	// bytesPerKiB converts bytes to kibibytes.
	bytesPerKiB = 1024
)

// benchmarkChildTypes returns a realistic set of child types for benchmark functions.
func benchmarkChildTypes() []node.Type {
	return []node.Type{
		node.UASTBlock, node.UASTAssignment, node.UASTIdentifier,
		node.UASTCall, node.UASTIdentifier, node.UASTReturn,
		node.UASTBinaryOp, node.UASTLiteral, node.UASTVariable,
		node.UASTParameter, node.UASTIf, node.UASTBlock,
		node.UASTLoop, node.UASTAssignment, node.UASTIdentifier,
		node.UASTCall, node.UASTReturn, node.UASTBinaryOp,
		node.UASTLiteral, node.UASTVariable,
	}
}

// BenchmarkCloneDetection_100Functions benchmarks clone detection on 100 functions.
func BenchmarkCloneDetection_100Functions(b *testing.B) {
	a := NewAnalyzer()
	childTypes := benchmarkChildTypes()

	functions := make([]*node.Node, 0, benchFunctionCount)

	for i := range benchFunctionCount {
		name := string(rune('A' + i%26))
		fn := buildFunctionNode(name, childTypes)
		functions = append(functions, fn)
	}

	root := buildRootWithFunctions(functions...)

	b.ResetTimer()

	for range b.N {
		report, err := a.Analyze(root)
		_ = report
		_ = err
	}
}

// BenchmarkShingling benchmarks shingle extraction.
func BenchmarkShingling(b *testing.B) {
	s := NewShingler(benchShingleSize)
	fn := buildFunctionNode("bench", benchmarkChildTypes())

	b.ResetTimer()

	for range b.N {
		shingles := s.ExtractShingles(fn)
		_ = shingles
	}
}

// buildBenchPairCapReports creates synthetic per-file reports with near-identical signatures.
// Each file has one function with the same token set, triggering worst-case pair explosion.
func buildBenchPairCapReports(b *testing.B, fileCount int) []map[string]analyze.Report {
	b.Helper()

	tokens := []string{"a", "b", "c", "d", "e", "f", "g"}
	reports := make([]map[string]analyze.Report, 0, fileCount)

	for i := range fileCount {
		sig, err := minhash.New(numHashes)
		require.NoError(b, err)

		for _, t := range tokens {
			sig.Add([]byte(t))
		}

		sourceFile := fmt.Sprintf("pkg/file%04d.go", i)
		funcName := "Handler"

		sigEntries := []map[string]any{
			{
				"name":         funcName,
				"sig":          sig,
				"_source_file": sourceFile,
			},
		}

		report := analyze.Report{
			keyAnalyzerName:    analyzerName,
			keyTotalFunctions:  1,
			keyTotalClonePairs: 0,
			keyCloneRatio:      0.0,
			keyClonePairs:      []map[string]any{},
			keyMessage:         msgNoClones,
			keyFuncSignatures:  sigEntries,
		}

		reports = append(reports, map[string]analyze.Report{"clones": report})
	}

	return reports
}

// stableHeapBytes forces GC and returns HeapInuse.
func stableHeapBytes() uint64 {
	runtime.GC()
	runtime.GC()

	var ms runtime.MemStats

	runtime.ReadMemStats(&ms)

	return ms.HeapInuse
}

// BenchmarkClonesPairCap measures heap impact of capping accumulated clone pairs.
// "before-no-cap" uses MaxClonePairs=0 (unlimited). "after-capped" uses MaxClonePairs=100.
// 500 near-identical functions → C(500,2)=124,750 pairs worst case.
func BenchmarkClonesPairCap(b *testing.B) {
	reports := buildBenchPairCapReports(b, benchPairCapFileCount)

	b.Run("before-no-cap", func(b *testing.B) {
		b.ReportAllocs()

		for b.Loop() {
			agg := NewAggregator()
			agg.MaxClonePairs = 0

			for _, report := range reports {
				agg.Aggregate(report)
			}

			baseline := stableHeapBytes()

			result := agg.GetResult()

			heapDelta := stableHeapBytes() - baseline
			b.ReportMetric(float64(heapDelta)/bytesPerKiB, "heap-delta-KiB")

			totalPairs, ok := result[keyTotalClonePairs].(int)
			require.True(b, ok, "total_clone_pairs must be int")
			b.ReportMetric(float64(totalPairs), "total-pairs")

			pairsRaw, ok := result[keyClonePairs].([]map[string]any)
			require.True(b, ok, "clone_pairs must be []map[string]any")
			b.ReportMetric(float64(len(pairsRaw)), "stored-pairs")
		}
	})

	b.Run("after-capped", func(b *testing.B) {
		b.ReportAllocs()

		for b.Loop() {
			agg := NewAggregator()
			agg.MaxClonePairs = benchPairCapCapped

			for _, report := range reports {
				agg.Aggregate(report)
			}

			baseline := stableHeapBytes()

			result := agg.GetResult()

			heapDelta := stableHeapBytes() - baseline
			b.ReportMetric(float64(heapDelta)/bytesPerKiB, "heap-delta-KiB")

			totalPairs, ok := result[keyTotalClonePairs].(int)
			require.True(b, ok, "total_clone_pairs must be int")
			b.ReportMetric(float64(totalPairs), "total-pairs")

			pairsRaw, ok := result[keyClonePairs].([]map[string]any)
			require.True(b, ok, "clone_pairs must be []map[string]any")
			b.ReportMetric(float64(len(pairsRaw)), "stored-pairs")
		}
	})
}

// BenchmarkVisitor_100Functions benchmarks the visitor pattern on 100 functions.
func BenchmarkVisitor_100Functions(b *testing.B) {
	childTypes := benchmarkChildTypes()

	functions := make([]*node.Node, 0, benchFunctionCount)

	for i := range benchFunctionCount {
		name := string(rune('A' + i%26))
		fn := buildFunctionNode(name, childTypes)
		functions = append(functions, fn)
	}

	b.ResetTimer()

	for range b.N {
		v := NewVisitor()

		for _, fn := range functions {
			v.OnEnter(fn, 0)

			for _, child := range fn.Children {
				v.OnEnter(child, 1)
			}
		}

		report := v.GetReport()
		_ = report
	}
}
