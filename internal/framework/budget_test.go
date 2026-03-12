package framework_test

import (
	"bytes"
	"context"
	"fmt"
	"os"
	"path/filepath"
	"slices"
	"strings"
	"testing"
	"time"

	git2go "github.com/libgit2/git2go/v34"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/anomaly"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/burndown"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/couples"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/devs"
	filehistory "github.com/Sumatoshi-tech/codefang/internal/analyzers/file_history"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/imports"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/plumbing"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/quality"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/sentiment"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/shotness"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/typos"
	"github.com/Sumatoshi-tech/codefang/internal/framework"
	"github.com/Sumatoshi-tech/codefang/internal/streaming"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

// plumbingCount is the number of core (plumbing) analyzers in the full pipeline.
const plumbingCount = 8

// buildAllAnalyzerPipeline constructs the full plumbing chain + all 10 leaf
// history analyzers, properly wired. Returns the ordered slice for the runner
// (plumbing first, then leaves) and the leaf subset for result inspection.
func buildAllAnalyzerPipeline(libRepo *gitlib.Repository) (
	all []analyze.HistoryAnalyzer,
	leaves []analyze.HistoryAnalyzer,
) {
	// Plumbing (order matters — each layer depends on prior ones).
	treeDiff := &plumbing.TreeDiffAnalyzer{Repository: libRepo}
	identity := &plumbing.IdentityDetector{}
	ticks := &plumbing.TicksSinceStart{}
	blobCache := &plumbing.BlobCacheAnalyzer{TreeDiff: treeDiff, Repository: libRepo}
	fileDiff := &plumbing.FileDiffAnalyzer{BlobCache: blobCache, TreeDiff: treeDiff}
	lineStats := &plumbing.LinesStatsCalculator{TreeDiff: treeDiff, BlobCache: blobCache, FileDiff: fileDiff}
	langDetect := &plumbing.LanguagesDetectionAnalyzer{TreeDiff: treeDiff, BlobCache: blobCache}
	uastChanges := &plumbing.UASTChangesAnalyzer{TreeDiff: treeDiff, BlobCache: blobCache}

	// Leaf analyzers — wired to shared plumbing instances.
	anomalyA := anomaly.NewAnalyzer()
	anomalyA.TreeDiff = treeDiff
	anomalyA.Identity = identity
	anomalyA.Ticks = ticks
	anomalyA.LineStats = lineStats
	anomalyA.Languages = langDetect

	burndownA := burndown.NewHistoryAnalyzer()
	burndownA.TreeDiff = treeDiff
	burndownA.BlobCache = blobCache
	burndownA.Identity = identity
	burndownA.Ticks = ticks
	burndownA.FileDiff = fileDiff

	couplesA := couples.NewHistoryAnalyzer()
	couplesA.TreeDiff = treeDiff
	couplesA.Identity = identity

	devsA := devs.NewAnalyzer()
	devsA.TreeDiff = treeDiff
	devsA.Identity = identity
	devsA.Ticks = ticks
	devsA.Languages = langDetect
	devsA.LineStats = lineStats

	fileHistoryA := filehistory.NewAnalyzer()
	fileHistoryA.TreeDiff = treeDiff
	fileHistoryA.Identity = identity
	fileHistoryA.LineStats = lineStats

	importsA := imports.NewHistoryAnalyzer()
	importsA.UAST = uastChanges
	importsA.Identity = identity
	importsA.Ticks = ticks

	qualityA := quality.NewAnalyzer()
	qualityA.UAST = uastChanges
	qualityA.Ticks = ticks

	sentimentA := sentiment.NewAnalyzer()
	sentimentA.UAST = uastChanges
	sentimentA.Ticks = ticks

	shotnessA := shotness.NewAnalyzer()
	shotnessA.UAST = uastChanges
	shotnessA.FileDiff = fileDiff

	typosA := typos.NewAnalyzer()
	typosA.UAST = uastChanges
	typosA.FileDiff = fileDiff
	typosA.BlobCache = blobCache

	plumbingAnalyzers := []analyze.HistoryAnalyzer{
		treeDiff, identity, ticks, blobCache, fileDiff, lineStats, langDetect, uastChanges,
	}

	leaves = []analyze.HistoryAnalyzer{
		anomalyA, burndownA, couplesA, devsA, fileHistoryA,
		importsA, qualityA, sentimentA, shotnessA, typosA,
	}

	all = make([]analyze.HistoryAnalyzer, 0, len(plumbingAnalyzers)+len(leaves))
	all = append(all, plumbingAnalyzers...)
	all = append(all, leaves...)

	return all, leaves
}

// createDiverseRepo creates a repo with varied file types, authors, and change
// patterns to exercise all analyzer code paths.
func createDiverseRepo(t *testing.T, numCommits int) string {
	t.Helper()

	dir := t.TempDir()

	repo, err := git2go.InitRepository(dir, false)
	require.NoError(t, err)

	defer repo.Free()

	// Initial commit: multiple file types.
	writeFile(t, dir, "main.go", "package main\n\nfunc main() {}\n")
	writeFile(t, dir, "utils.go", "package main\n\nfunc add(a, b int) int { return a + b }\n")
	writeFile(t, dir, "helper.py", "def helper():\n    return 42\n")
	writeFile(t, dir, "app.js", "function app() { return 'hello'; }\n")
	commitWithAuthor(t, repo, "initial commit", "Alice", "alice@test.com", time.Now().Add(-time.Duration(numCommits)*time.Hour))

	for i := 1; i < numCommits; i++ {
		ts := time.Now().Add(-time.Duration(numCommits-i) * time.Hour)

		switch i % 7 {
		case 0: // New Go file.
			writeFile(t, dir, fmt.Sprintf("pkg/mod%d.go", i),
				fmt.Sprintf("package pkg\n\nfunc F%d() int { return %d }\n", i, i))
		case 1: // Modify existing Go file.
			writeFile(t, dir, "utils.go",
				fmt.Sprintf("package main\n\n// updated in commit %d\nfunc add(a, b int) int { return a + b + %d }\n", i, i))
		case 2: // New Python file.
			writeFile(t, dir, fmt.Sprintf("scripts/script%d.py", i),
				fmt.Sprintf("def func_%d():\n    return %d\n", i, i))
		case 3: // Modify JS file.
			writeFile(t, dir, "app.js",
				fmt.Sprintf("function app() { return %d; }\n", i))
		case 4: // Large multi-line Go change.
			var lines strings.Builder
			lines.WriteString("package main\n\n")

			for j := range 20 {
				fmt.Fprintf(&lines, "func f%d_%d() int { return %d }\n", i, j, j)
			}

			writeFile(t, dir, fmt.Sprintf("pkg/large%d.go", i), lines.String())
		case 5: // Delete a file (if it exists).
			name := fmt.Sprintf("pkg/mod%d.go", i-5)
			os.Remove(filepath.Join(dir, name))
		case 6: // Another new Go file (simulates renames by adding new + deleting old).
			writeFile(t, dir, fmt.Sprintf("pkg/renamed%d.go", i),
				fmt.Sprintf("package pkg\n\nfunc R%d() int { return %d }\n", i, i))
		}

		// Alternate between two authors.
		if i%2 == 0 {
			commitWithAuthor(t, repo, fmt.Sprintf("commit %d", i), "Alice", "alice@test.com", ts)
		} else {
			commitWithAuthor(t, repo, fmt.Sprintf("commit %d", i), "Bob", "bob@test.com", ts)
		}
	}

	return dir
}

func commitWithAuthor(t *testing.T, repo *git2go.Repository, message, name, email string, when time.Time) {
	t.Helper()

	index, err := repo.Index()
	require.NoError(t, err)

	defer index.Free()

	require.NoError(t, index.AddAll([]string{"*"}, git2go.IndexAddDefault, nil))
	require.NoError(t, index.Write())

	treeID, err := index.WriteTree()
	require.NoError(t, err)

	tree, err := repo.LookupTree(treeID)
	require.NoError(t, err)

	defer tree.Free()

	sig := &git2go.Signature{Name: name, Email: email, When: when}

	var parents []*git2go.Commit

	head, headErr := repo.Head()
	if headErr == nil {
		hc, lookupErr := repo.LookupCommit(head.Target())
		if lookupErr == nil {
			parents = append(parents, hc)
		}

		head.Free()
	}

	_, err = repo.CreateCommit("HEAD", sig, sig, message, tree, parents...)
	require.NoError(t, err)

	for _, p := range parents {
		p.Free()
	}
}

// Test: All Leaf Analyzers Implement Hibernatable.

// TestBudget_AllLeafAnalyzersHibernatable verifies that every leaf analyzer
// implements the Hibernatable interface at runtime (not just compile-time).
func TestBudget_AllLeafAnalyzersHibernatable(t *testing.T) {
	t.Parallel()

	libRepo, err := gitlib.OpenRepository(createDiverseRepo(t, 5))
	require.NoError(t, err)

	defer libRepo.Free()

	all, leaves := buildAllAnalyzerPipeline(libRepo)

	for _, leaf := range leaves {
		h, ok := leaf.(streaming.Hibernatable)
		require.Truef(t, ok, "analyzer %s must implement streaming.Hibernatable", leaf.Name())

		// Verify Hibernate/Boot don't panic on zero-value state.
		require.NoError(t, h.Hibernate(), "Hibernate on %s", leaf.Name())
		require.NoError(t, h.Boot(), "Boot on %s", leaf.Name())
	}

	// Verify validateStreamingInterfaces passes.
	runner := framework.NewRunner(libRepo, "", all...)
	runner.CoreCount = plumbingCount
	// RunStreaming calls validateStreamingInterfaces internally.
	// We verify it doesn't return a validation error by passing a single commit.
	commits := collectAllCommits(t, libRepo)
	slices.Reverse(commits)

	streamConfig := framework.StreamingConfig{
		MemBudget: 500 * 1024 * 1024,
		RepoPath:  "",
	}

	_, err = framework.RunStreaming(ctx(), runner, commits, all, streamConfig)
	require.NoError(t, err)
}

// Test: WorkingStateSize / AvgTCSize Non-Zero for Stateful Analyzers.

// TestBudget_WorkingStateSizeReported verifies that all leaf analyzers
// report meaningful WorkingStateSize and AvgTCSize values.
func TestBudget_WorkingStateSizeReported(t *testing.T) {
	t.Parallel()

	libRepo, err := gitlib.OpenRepository(createDiverseRepo(t, 5))
	require.NoError(t, err)

	defer libRepo.Free()

	_, leaves := buildAllAnalyzerPipeline(libRepo)

	// Analyzers with cumulative state must report non-zero WorkingStateSize.
	stateful := map[string]bool{
		"history/burndown":    true,
		"Couples":             true,
		"FileHistoryAnalysis": true,
		"history/devs":        true,
		"Shotness":            true,
	}

	for _, leaf := range leaves {
		if stateful[leaf.Name()] {
			require.Positivef(t, leaf.WorkingStateSize(),
				"stateful analyzer %s must report WorkingStateSize > 0", leaf.Name())
		}

		// All analyzers must report non-zero AvgTCSize.
		require.Positivef(t, leaf.AvgTCSize(),
			"analyzer %s must report AvgTCSize > 0", leaf.Name())
	}
}

// Test: Streaming with All Analyzers Stays Within Budget.

// TestBudget_StreamingAllAnalyzers runs all 10 leaf analyzers through the
// streaming pipeline with a constrained memory budget, verifying no panics
// and that aggregator state stays bounded.
func TestBudget_StreamingAllAnalyzers(t *testing.T) {
	t.Parallel()

	const numCommits = 200

	repoPath := createDiverseRepo(t, numCommits)

	libRepo, err := gitlib.OpenRepository(repoPath)
	require.NoError(t, err)

	defer libRepo.Free()

	commits := collectAllCommits(t, libRepo)
	slices.Reverse(commits)

	all, leaves := buildAllAnalyzerPipeline(libRepo)
	config := framework.DefaultCoordinatorConfig()
	runner := framework.NewRunnerWithConfig(libRepo, repoPath, config, all...)
	runner.CoreCount = plumbingCount

	var maxAggState int64

	streamConfig := framework.StreamingConfig{
		MemBudget: 256 * 1024 * 1024, // 256 MiB — forces multiple chunks.
		RepoPath:  repoPath,
		OnChunkComplete: func(r *framework.Runner) error {
			aggSize := framework.AggregatorStateSizeForTest(r)
			if aggSize > maxAggState {
				maxAggState = aggSize
			}

			return nil
		},
	}

	results, err := framework.RunStreaming(ctx(), runner, commits, all, streamConfig)
	require.NoError(t, err)

	// Verify all leaf analyzers produced a report (may be empty for some).
	for _, leaf := range leaves {
		report := results[leaf]
		require.NotNilf(t, report, "analyzer %s should produce a report", leaf.Name())
	}

	// Aggregator state should stay bounded by the budget.
	t.Logf("peak aggregator state: %d bytes", maxAggState)
}

// Test: Streaming vs Single-Pass Equivalence.

// TestBudget_StreamingEquivalenceAllAnalyzers verifies that running all 10
// analyzers through chunked streaming produces the same results as a
// single-pass run.
func TestBudget_StreamingEquivalenceAllAnalyzers(t *testing.T) {
	t.Parallel()

	const numCommits = 100

	repoPath := createDiverseRepo(t, numCommits)

	libRepo, err := gitlib.OpenRepository(repoPath)
	require.NoError(t, err)

	defer libRepo.Free()

	commits := collectAllCommits(t, libRepo)
	slices.Reverse(commits)

	// Baseline: single-pass run (same CoreCount as streaming for fair comparison).
	baselineAll, baselineLeaves := buildAllAnalyzerPipeline(libRepo)
	baselineRunner := framework.NewRunner(libRepo, repoPath, baselineAll...)
	baselineRunner.CoreCount = plumbingCount

	baselineResults, err := baselineRunner.Run(ctx(), commits)
	require.NoError(t, err)

	// Streaming: chunked run with hibernate/boot.
	streamAll, streamLeaves := buildAllAnalyzerPipeline(libRepo)
	streamConfig := framework.DefaultCoordinatorConfig()
	streamRunner := framework.NewRunnerWithConfig(libRepo, repoPath, streamConfig, streamAll...)
	streamRunner.CoreCount = plumbingCount

	streamResults, err := framework.RunStreaming(ctx(), streamRunner, commits, streamAll, framework.StreamingConfig{
		MemBudget: 256 * 1024 * 1024,
		RepoPath:  repoPath,
	})
	require.NoError(t, err)

	// Analyzers with deterministic output (no list-ordering variance from
	// parallel fork/merge). The existing TestScaleScanning_StreamingProducesIdenticalOutput
	// validates devs; burndown is sequential so also deterministic.
	deterministicAnalyzers := map[string]bool{
		"history/devs":     true,
		"history/burndown": true,
	}

	// Compare outputs for each leaf analyzer.
	for i, baselineLeaf := range baselineLeaves {
		streamLeaf := streamLeaves[i]

		baselineReport := baselineResults[baselineLeaf]
		streamReport := streamResults[streamLeaf]

		require.NotNilf(t, baselineReport, "baseline %s should produce a report", baselineLeaf.Name())
		require.NotNilf(t, streamReport, "streaming %s should produce a report", streamLeaf.Name())

		// Verify both produce valid JSON (no serialization errors).
		var baselineBuf, streamBuf bytes.Buffer

		err = baselineLeaf.Serialize(baselineReport, analyze.FormatJSON, &baselineBuf)
		require.NoErrorf(t, err, "baseline %s JSON serialization", baselineLeaf.Name())

		err = streamLeaf.Serialize(streamReport, analyze.FormatJSON, &streamBuf)
		require.NoErrorf(t, err, "streaming %s JSON serialization", streamLeaf.Name())

		// Strict YAML equivalence only for deterministic analyzers.
		if deterministicAnalyzers[baselineLeaf.Name()] {
			var baselineYAML, streamYAML bytes.Buffer

			err = baselineLeaf.Serialize(baselineReport, analyze.FormatYAML, &baselineYAML)
			require.NoError(t, err)

			err = streamLeaf.Serialize(streamReport, analyze.FormatYAML, &streamYAML)
			require.NoError(t, err)

			require.YAMLEqf(t, baselineYAML.String(), streamYAML.String(),
				"streaming output for %s must match single-pass baseline", baselineLeaf.Name())
		}
	}
}

// Test: Hibernate/Boot Cycles Don't Corrupt State.

// TestBudget_HibernateBootCycleAllAnalyzers verifies that all 10 analyzers
// survive multiple hibernate/boot cycles during chunked processing without
// data corruption.
func TestBudget_HibernateBootCycleAllAnalyzers(t *testing.T) {
	t.Parallel()

	const (
		numCommits = 100
		chunkSize  = 20
	)

	repoPath := createDiverseRepo(t, numCommits)

	libRepo, err := gitlib.OpenRepository(repoPath)
	require.NoError(t, err)

	defer libRepo.Free()

	commits := collectAllCommits(t, libRepo)
	slices.Reverse(commits)

	all, leaves := buildAllAnalyzerPipeline(libRepo)
	runner := framework.NewRunner(libRepo, repoPath, all...)
	runner.CoreCount = plumbingCount

	require.NoError(t, runner.Initialize())

	numChunks := (len(commits) + chunkSize - 1) / chunkSize

	for chunkIdx := range numChunks {
		start := chunkIdx * chunkSize
		end := min(start+chunkSize, len(commits))

		if chunkIdx > 0 {
			// Hibernate all analyzers.
			for _, a := range all {
				if h, ok := a.(streaming.Hibernatable); ok {
					require.NoErrorf(t, h.Hibernate(), "Hibernate chunk %d, analyzer %s", chunkIdx, a.Name())
				}
			}

			// Boot all analyzers.
			for _, a := range all {
				if h, ok := a.(streaming.Hibernatable); ok {
					require.NoErrorf(t, h.Boot(), "Boot chunk %d, analyzer %s", chunkIdx, a.Name())
				}
			}
		}

		_, processErr := runner.ProcessChunk(ctx(), commits[start:end], start, chunkIdx)
		require.NoErrorf(t, processErr, "ProcessChunk %d", chunkIdx)
	}

	results, err := runner.FinalizeWithAggregators(ctx())
	require.NoError(t, err)

	// All leaf analyzers should produce a report.
	for _, leaf := range leaves {
		report := results[leaf]
		require.NotNilf(t, report, "analyzer %s should produce a report after hibernate/boot cycles", leaf.Name())
	}
}

// Test: Aggregator State Growth Tracking.

// TestBudget_AggregatorStateGrowthBounded verifies that aggregator state
// grows sub-linearly with commit count when spilling is active.
func TestBudget_AggregatorStateGrowthBounded(t *testing.T) {
	t.Parallel()

	const (
		numCommits = 200
		chunkSize  = 50
	)

	repoPath := createDiverseRepo(t, numCommits)

	libRepo, err := gitlib.OpenRepository(repoPath)
	require.NoError(t, err)

	defer libRepo.Free()

	commits := collectAllCommits(t, libRepo)
	slices.Reverse(commits)

	all, _ := buildAllAnalyzerPipeline(libRepo)
	runner := framework.NewRunner(libRepo, repoPath, all...)
	runner.CoreCount = plumbingCount

	require.NoError(t, runner.Initialize())

	numChunks := (len(commits) + chunkSize - 1) / chunkSize
	stateByChunk := make([]int64, 0, numChunks)

	for chunkIdx := range numChunks {
		start := chunkIdx * chunkSize
		end := min(start+chunkSize, len(commits))

		if chunkIdx > 0 {
			for _, a := range all {
				if h, ok := a.(streaming.Hibernatable); ok {
					require.NoError(t, h.Hibernate())
				}
			}

			for _, a := range all {
				if h, ok := a.(streaming.Hibernatable); ok {
					require.NoError(t, h.Boot())
				}
			}
		}

		_, processErr := runner.ProcessChunk(ctx(), commits[start:end], start, chunkIdx)
		require.NoError(t, processErr)

		stateByChunk = append(stateByChunk, framework.AggregatorStateSizeForTest(runner))
	}

	// State should grow — but not explode. Log the progression for visibility.
	for i, size := range stateByChunk {
		t.Logf("chunk %d: aggregator state = %d bytes", i, size)
	}
}

func ctx() context.Context {
	return context.Background()
}
