package uast_test

// FRD: specs/frds/FRD-20260311-eager-tree-release.md.

import (
	"context"
	"fmt"
	"runtime"
	"strings"
	"testing"

	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/uast"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

// benchFunctionCount is the number of functions in the synthetic Go file for tree release benchmarks.
const benchFunctionCount = 50

// benchFunctionLines is the approximate line count per function body.
const benchFunctionLines = 20

// benchTreeFileCount is the number of files parsed to simulate a realistic workload.
const benchTreeFileCount = 80

// bytesPerMiB converts bytes to mebibytes.
const bytesPerMiB = 1024 * 1024

// buildLargeGoSource generates a synthetic Go file with many functions to produce large UAST trees.
func buildLargeGoSource(functionCount, bodyLines int) []byte {
	var b strings.Builder

	fmt.Fprintf(&b, "package bench\n\n")

	for i := range functionCount {
		fmt.Fprintf(&b, "func F%d(a, b int) int {\n", i)

		for j := range bodyLines {
			fmt.Fprintf(&b, "\tx%d := a + b + %d\n", j, j)
		}

		fmt.Fprintf(&b, "\treturn x0\n")
		fmt.Fprintf(&b, "}\n\n")
	}

	return []byte(b.String())
}

// stableHeapInuse forces two GC cycles and returns HeapInuse in bytes.
func stableHeapInuse() uint64 {
	runtime.GC()
	runtime.GC()

	var ms runtime.MemStats

	runtime.ReadMemStats(&ms)

	return ms.HeapInuse
}

// BenchmarkParserTreeRelease measures heap impact of eager node.ReleaseTree.
//
// "before-no-release" parses 80 files, holds ALL trees alive, then measures heap.
// "after-with-release" parses 80 files, calls ReleaseTree after each, then measures heap.
// The delta shows how much heap is saved by not accumulating dead trees.
func BenchmarkParserTreeRelease(b *testing.B) {
	parser, err := uast.NewParser()
	require.NoError(b, err)

	source := buildLargeGoSource(benchFunctionCount, benchFunctionLines)

	b.Run("before-no-release", func(b *testing.B) {
		for b.Loop() {
			// Establish baseline before parsing.
			baseline := stableHeapInuse()

			// Parse all files — keep every tree alive (no release).
			trees := make([]*node.Node, 0, benchTreeFileCount)

			for range benchTreeFileCount {
				root, parseErr := parser.Parse(context.Background(), "bench.go", source)
				require.NoError(b, parseErr)

				trees = append(trees, root)
			}

			// Measure with all trees still referenced.
			var ms runtime.MemStats

			runtime.ReadMemStats(&ms)

			heapDelta := ms.HeapInuse - baseline
			b.ReportMetric(float64(heapDelta)/bytesPerMiB, "heap-delta-MiB")

			// Keep trees alive past measurement.
			runtime.KeepAlive(trees)
		}
	})

	b.Run("after-with-release", func(b *testing.B) {
		for b.Loop() {
			// Establish baseline before parsing.
			baseline := stableHeapInuse()

			// Parse all files — release each tree immediately after parse.
			for range benchTreeFileCount {
				root, parseErr := parser.Parse(context.Background(), "bench.go", source)
				require.NoError(b, parseErr)

				node.ReleaseTree(root)
			}

			// Measure after all trees released back to pool.
			var ms runtime.MemStats

			runtime.ReadMemStats(&ms)

			heapDelta := ms.HeapInuse - baseline
			b.ReportMetric(float64(heapDelta)/bytesPerMiB, "heap-delta-MiB")
		}
	})
}
