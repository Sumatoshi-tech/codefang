package commands

import (
	"context"
	"os/exec"
	"runtime"
	"testing"
	"time"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/framework"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

// TestUniversalAnalyzers_MemoryLeak checks all default analyzers for memory leaks
// during a simulated long-running stream of commits.
func TestUniversalAnalyzers_MemoryLeak(t *testing.T) {
	t.Parallel()

	if testing.Short() {
		t.Skip("skipping universal memory test in short mode")
	}

	tmpDir := t.TempDir()

	// 1. Create a synthetic repo.
	ctx := context.Background()

	cmd := exec.CommandContext(ctx, "bash", "-c", `
		git init
		git config user.name "Test"
		git config user.email "test@test.com"
		echo "package main\nfunc main() {}" > file1.go
		git add .
		git commit -m "init"

		for i in {1..500}; do
			for j in {1..50}; do
				echo "package main\nfunc f${i}_${j}() {}" > "file_${j}.go"
			done
			git add .
			git commit -m "commit ${i}"
		done
	`)
	cmd.Dir = tmpDir
	require.NoError(t, cmd.Run())

	libRepo, err := gitlib.OpenRepository(tmpDir)
	require.NoError(t, err)

	t.Cleanup(libRepo.Free)

	leaves := defaultHistoryLeaves()
	for _, leaf := range leaves {
		t.Run(leaf.Name(), func(t *testing.T) {
			t.Parallel()

			// Create a fresh iterator for each run.
			iter, iterErr := libRepo.Log(&gitlib.LogOptions{})
			require.NoError(t, iterErr)

			// Count commits.
			commitCount := 0
			iterErr = iter.ForEach(func(_ *gitlib.Commit) error {
				commitCount++

				return nil
			})
			require.NoError(t, iterErr)

			// Re-create iterator for actual run.
			iter, iterErr = libRepo.Log(&gitlib.LogOptions{Reverse: true})
			require.NoError(t, iterErr)

			// Trigger GC and measure baseline.
			runtime.GC()

			var baseline runtime.MemStats
			runtime.ReadMemStats(&baseline)

			// Some analyzers use huge memory baseline (e.g. Tree-sitter parsers),
			// so we look at the DELTA.

			// We need a pipeline core + the specific leaf.
			pl := buildPipeline(nil)
			allAnalyzers := make([]analyze.HistoryAnalyzer, 0, len(pl.Core)+1)
			allAnalyzers = append(allAnalyzers, pl.Core...)
			allAnalyzers = append(allAnalyzers, leaf)

			cfg := framework.CoordinatorConfig{
				Workers:         2,
				BufferSize:      10,
				CommitBatchSize: 50,
			}

			runner := framework.NewRunnerWithConfig(libRepo, tmpDir, cfg, allAnalyzers...)
			runner.CoreCount = len(pl.Core)

			streamCfg := framework.StreamingConfig{
				MemBudget: 1024 * 1024 * 1024, // 1 GiB.
				TmpDir:    t.TempDir(),
			}

			// Run streaming.
			_, runErr := framework.RunStreamingFromIterator(t.Context(), runner, iter, commitCount, allAnalyzers, streamCfg)
			require.NoError(t, runErr)

			// After streaming and finalizing, trigger GC to reclaim.
			runtime.GC()

			// Small sleep to allow background goroutines (like finalizers) to clear out.
			time.Sleep(100 * time.Millisecond)
			runtime.GC()

			var after runtime.MemStats
			runtime.ReadMemStats(&after)

			deltaMiB := int64(after.HeapInuse-baseline.HeapInuse) / (1024 * 1024)

			t.Logf("Analyzer: %s | Memory Delta: %d MiB (Before: %d MiB, After: %d MiB)",
				leaf.Name(), deltaMiB, baseline.HeapInuse/(1024*1024), after.HeapInuse/(1024*1024))

			// Assert that the analyzer didn't leak a massive amount of memory.
			// We allow a small tolerance (e.g. 50MB) for global caches or init structures.
			assert.Less(t, deltaMiB, int64(50), "Analyzer %s leaked too much memory: %d MiB", leaf.Name(), deltaMiB)
		})
	}
}
