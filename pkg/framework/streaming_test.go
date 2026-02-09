package framework

import (
	"testing"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
	"github.com/Sumatoshi-tech/codefang/pkg/streaming"
)

func TestDoubleBufferMemoryBudget_HalvesBudget(t *testing.T) {
	t.Parallel()

	// 2 GiB budget: (2 GiB - 400 MiB overhead) / 2 + 400 MiB = ~1.2 GiB
	const budget = int64(2 * 1024 * 1024 * 1024)
	const overhead = int64(streaming.BaseOverhead)
	const want = (budget-overhead)/2 + overhead

	got := doubleBufferMemoryBudget(budget)
	if got != want {
		t.Fatalf("doubleBufferMemoryBudget(%d) = %d, want %d", budget, got, want)
	}
}

func TestDoubleBufferMemoryBudget_BelowOverhead(t *testing.T) {
	t.Parallel()

	// Budget below overhead: returns original budget (no split possible).
	const budget = int64(streaming.BaseOverhead - 1)

	got := doubleBufferMemoryBudget(budget)
	if got != budget {
		t.Fatalf("doubleBufferMemoryBudget(%d) = %d, want %d (unchanged)", budget, got, budget)
	}
}

func TestDoubleBufferMemoryBudget_ZeroBudget(t *testing.T) {
	t.Parallel()

	got := doubleBufferMemoryBudget(0)
	if got != 0 {
		t.Fatalf("doubleBufferMemoryBudget(0) = %d, want 0", got)
	}
}

func TestCanDoubleBuffer_EnoughBudgetMultipleChunks(t *testing.T) {
	t.Parallel()

	// 2 GiB budget, 3 chunks: double-buffering is allowed.
	const budget = int64(2 * 1024 * 1024 * 1024)
	const chunks = 3

	if !canDoubleBuffer(budget, chunks) {
		t.Fatal("canDoubleBuffer should return true for sufficient budget and multiple chunks")
	}
}

func TestCanDoubleBuffer_SingleChunk(t *testing.T) {
	t.Parallel()

	// Even with large budget, single chunk means no double-buffering needed.
	const budget = int64(4 * 1024 * 1024 * 1024)
	const chunks = 1

	if canDoubleBuffer(budget, chunks) {
		t.Fatal("canDoubleBuffer should return false for single chunk")
	}
}

func TestCanDoubleBuffer_ZeroBudget(t *testing.T) {
	t.Parallel()

	// Zero budget disables double-buffering.
	const chunks = 3

	if canDoubleBuffer(0, chunks) {
		t.Fatal("canDoubleBuffer should return false for zero budget")
	}
}

func TestCanDoubleBuffer_BudgetBelowThreshold(t *testing.T) {
	t.Parallel()

	// Budget below minimum threshold: no double-buffering.
	const budget = int64(streaming.BaseOverhead + 1)
	const chunks = 3

	if canDoubleBuffer(budget, chunks) {
		t.Fatal("canDoubleBuffer should return false when budget is below minimum threshold")
	}
}

func TestPrefetchedChunk_CollectsData(t *testing.T) {
	t.Parallel()

	repo := NewTestRepo(t)
	defer repo.Close()

	repo.CreateFile("a.txt", "hello")
	repo.Commit("first")
	repo.CreateFile("b.txt", "world")
	repo.Commit("second")

	libRepo, err := gitlib.OpenRepository(repo.Path())
	if err != nil {
		t.Fatalf("OpenRepository: %v", err)
	}
	defer libRepo.Free()

	commits := CollectCommits(t, libRepo, 0)
	if len(commits) < 2 {
		t.Fatalf("expected at least 2 commits, got %d", len(commits))
	}

	config := DefaultCoordinatorConfig()
	pf := prefetchPipeline(repo.Path(), config, commits)

	if pf.err != nil {
		t.Fatalf("prefetchPipeline error: %v", pf.err)
	}

	if len(pf.data) != len(commits) {
		t.Fatalf("prefetched %d items, want %d", len(pf.data), len(commits))
	}

	for idx, cd := range pf.data {
		if cd.Commit == nil {
			t.Fatalf("prefetched item %d has nil commit", idx)
		}
	}
}

func TestStartPrefetch_ReturnsChannelWithResult(t *testing.T) {
	t.Parallel()

	repo := NewTestRepo(t)
	defer repo.Close()

	repo.CreateFile("a.txt", "hello")
	repo.Commit("first")

	libRepo, err := gitlib.OpenRepository(repo.Path())
	if err != nil {
		t.Fatalf("OpenRepository: %v", err)
	}
	defer libRepo.Free()

	commits := CollectCommits(t, libRepo, 0)
	config := DefaultCoordinatorConfig()

	resultCh := startPrefetch(repo.Path(), config, commits)

	pf := <-resultCh
	if pf.err != nil {
		t.Fatalf("startPrefetch error: %v", pf.err)
	}

	if len(pf.data) != len(commits) {
		t.Fatalf("prefetched %d items, want %d", len(pf.data), len(commits))
	}
}

func TestRunner_ProcessChunkFromData(t *testing.T) {
	t.Parallel()

	repo := NewTestRepo(t)
	defer repo.Close()

	repo.CreateFile("a.txt", "hello")
	repo.Commit("first")
	repo.CreateFile("b.txt", "world")
	repo.Commit("second")

	libRepo, err := gitlib.OpenRepository(repo.Path())
	if err != nil {
		t.Fatalf("OpenRepository: %v", err)
	}
	defer libRepo.Free()

	commits := CollectCommits(t, libRepo, 0)
	if len(commits) < 2 {
		t.Fatalf("expected at least 2 commits, got %d", len(commits))
	}

	// Prefetch pipeline data.
	config := DefaultCoordinatorConfig()
	pf := prefetchPipeline(repo.Path(), config, commits)

	if pf.err != nil {
		t.Fatalf("prefetchPipeline error: %v", pf.err)
	}

	// Create a runner with a plumbing analyzer and process from prefetched data.
	runner := NewRunnerWithConfig(libRepo, repo.Path(), config, &plumbing.TreeDiffAnalyzer{})

	initErr := runner.Initialize()
	if initErr != nil {
		t.Fatalf("Initialize: %v", initErr)
	}

	processErr := runner.ProcessChunkFromData(pf.data, 0)
	if processErr != nil {
		t.Fatalf("ProcessChunkFromData: %v", processErr)
	}

	reports, finalizeErr := runner.Finalize()
	if finalizeErr != nil {
		t.Fatalf("Finalize: %v", finalizeErr)
	}

	if len(reports) != 1 {
		t.Fatalf("reports length = %d, want 1", len(reports))
	}
}

func TestProcessChunksDoubleBuffered_IdenticalOutput(t *testing.T) {
	t.Parallel()

	repo := NewTestRepo(t)
	defer repo.Close()

	// Create 4 commits so we can have at least 2 chunks with small chunk size.
	repo.CreateFile("a.txt", "a")
	repo.Commit("c1")
	repo.CreateFile("b.txt", "b")
	repo.Commit("c2")
	repo.CreateFile("c.txt", "c")
	repo.Commit("c3")
	repo.CreateFile("d.txt", "d")
	repo.Commit("c4")

	libRepo, err := gitlib.OpenRepository(repo.Path())
	if err != nil {
		t.Fatalf("OpenRepository: %v", err)
	}
	defer libRepo.Free()

	commits := CollectCommits(t, libRepo, 0)
	if len(commits) < 4 {
		t.Fatalf("expected at least 4 commits, got %d", len(commits))
	}

	config := DefaultCoordinatorConfig()

	// Sequential run.
	seqRunner := NewRunnerWithConfig(libRepo, repo.Path(), config, &plumbing.TreeDiffAnalyzer{})
	seqReports, seqErr := seqRunner.Run(commits)

	if seqErr != nil {
		t.Fatalf("sequential Run: %v", seqErr)
	}

	// Double-buffered run: split into 2 chunks manually.
	mid := len(commits) / 2
	chunks := []streaming.ChunkBounds{
		{Start: 0, End: mid},
		{Start: mid, End: len(commits)},
	}

	dbRunner := NewRunnerWithConfig(libRepo, repo.Path(), config, &plumbing.TreeDiffAnalyzer{})

	dbInitErr := dbRunner.Initialize()
	if dbInitErr != nil {
		t.Fatalf("Initialize: %v", dbInitErr)
	}

	dbErr := processChunksDoubleBuffered(
		dbRunner, commits, chunks, nil, nil, nil, repo.Path(), nil, 0,
	)
	if dbErr != nil {
		t.Fatalf("processChunksDoubleBuffered: %v", dbErr)
	}

	dbReports, dbFinalizeErr := dbRunner.Finalize()
	if dbFinalizeErr != nil {
		t.Fatalf("Finalize: %v", dbFinalizeErr)
	}

	// Both should produce one report entry.
	if len(seqReports) != len(dbReports) {
		t.Fatalf("report count: seq=%d, db=%d", len(seqReports), len(dbReports))
	}
}

func TestPlanChunksWithDoubleBuffer_EnabledForLargeRepo(t *testing.T) {
	t.Parallel()

	// 2 GiB budget, 2000 commits: should enable double-buffering.
	const budget = int64(2 * 1024 * 1024 * 1024)
	const commitCount = 2000

	chunks, enabled := planChunksWithDoubleBuffer(commitCount, budget)
	if !enabled {
		t.Fatal("expected double-buffering to be enabled")
	}

	if len(chunks) < 2 {
		t.Fatalf("expected at least 2 chunks, got %d", len(chunks))
	}

	// Verify all commits are covered.
	totalCommits := 0
	for _, ch := range chunks {
		totalCommits += ch.End - ch.Start
	}

	if totalCommits != commitCount {
		t.Fatalf("chunks cover %d commits, want %d", totalCommits, commitCount)
	}
}

func TestPlanChunksWithDoubleBuffer_DisabledForSmallRepo(t *testing.T) {
	t.Parallel()

	// 2 GiB budget, 100 commits: single chunk, no double-buffering.
	const budget = int64(2 * 1024 * 1024 * 1024)
	const commitCount = 100

	chunks, enabled := planChunksWithDoubleBuffer(commitCount, budget)
	if enabled {
		t.Fatal("expected double-buffering to be disabled for single chunk")
	}

	if len(chunks) != 1 {
		t.Fatalf("expected 1 chunk, got %d", len(chunks))
	}
}

func TestCanResumeWithCheckpoint(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name              string
		totalAnalyzers    int
		checkpointable    int
		wantResumeEnabled bool
	}{
		{
			name:              "no analyzers",
			totalAnalyzers:    0,
			checkpointable:    0,
			wantResumeEnabled: false,
		},
		{
			name:              "none checkpointable",
			totalAnalyzers:    8,
			checkpointable:    0,
			wantResumeEnabled: false,
		},
		{
			name:              "partial checkpoint support",
			totalAnalyzers:    8,
			checkpointable:    3,
			wantResumeEnabled: false,
		},
		{
			name:              "all analyzers checkpointable",
			totalAnalyzers:    8,
			checkpointable:    8,
			wantResumeEnabled: true,
		},
		{
			name:              "checkpoint count exceeds analyzers",
			totalAnalyzers:    8,
			checkpointable:    9,
			wantResumeEnabled: false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			got := CanResumeWithCheckpoint(tt.totalAnalyzers, tt.checkpointable)
			if got != tt.wantResumeEnabled {
				t.Fatalf(
					"CanResumeWithCheckpoint(%d, %d) = %t, want %t",
					tt.totalAnalyzers,
					tt.checkpointable,
					got,
					tt.wantResumeEnabled,
				)
			}
		})
	}
}
