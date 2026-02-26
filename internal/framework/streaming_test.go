package framework

import (
	"context"
	"io"
	"log/slog"
	"testing"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/plumbing"
	"github.com/Sumatoshi-tech/codefang/internal/streaming"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

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
	pf := prefetchPipeline(context.Background(), repo.Path(), config, commits, nil)

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

	resultCh := startPrefetch(context.Background(), repo.Path(), config, commits, nil)

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
	pf := prefetchPipeline(context.Background(), repo.Path(), config, commits, nil)

	if pf.err != nil {
		t.Fatalf("prefetchPipeline error: %v", pf.err)
	}

	// Create a runner with a plumbing analyzer and process from prefetched data.
	runner := NewRunnerWithConfig(libRepo, repo.Path(), config, &plumbing.TreeDiffAnalyzer{})

	initErr := runner.Initialize()
	if initErr != nil {
		t.Fatalf("Initialize: %v", initErr)
	}

	_, processErr := runner.ProcessChunkFromData(context.Background(), pf.data, 0, 0)
	if processErr != nil {
		t.Fatalf("ProcessChunkFromData: %v", processErr)
	}

	reports, finalizeErr := runner.FinalizeWithAggregators(context.Background())
	if finalizeErr != nil {
		t.Fatalf("FinalizeWithAggregators: %v", finalizeErr)
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

	seqReports, seqErr := seqRunner.Run(context.Background(), commits)
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

	ap := streaming.NewAdaptivePlanner(len(commits), 0, 0, 0)

	_, dbErr := processChunksDoubleBuffered(
		context.Background(), slog.New(slog.NewTextHandler(io.Discard, nil)),
		dbRunner, commits, chunks, nil, nil, nil, repo.Path(), nil, 0,
		ap, 0,
	)
	if dbErr != nil {
		t.Fatalf("processChunksDoubleBuffered: %v", dbErr)
	}

	dbReports, dbFinalizeErr := dbRunner.FinalizeWithAggregators(context.Background())
	if dbFinalizeErr != nil {
		t.Fatalf("FinalizeWithAggregators: %v", dbFinalizeErr)
	}

	// Both should produce one report entry.
	if len(seqReports) != len(dbReports) {
		t.Fatalf("report count: seq=%d, db=%d", len(seqReports), len(dbReports))
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

func TestStreamingConfig_LoggerFallback(t *testing.T) {
	t.Parallel()

	t.Run("nil returns discard logger", func(t *testing.T) {
		t.Parallel()

		cfg := StreamingConfig{}
		logger := cfg.logger()

		if logger == nil {
			t.Fatal("logger() should never return nil")
		}

		// Discard logger should not panic on write.
		logger.Info("test message")
	})

	t.Run("set logger is returned", func(t *testing.T) {
		t.Parallel()

		want := slog.New(slog.NewTextHandler(io.Discard, nil))
		cfg := StreamingConfig{Logger: want}

		got := cfg.logger()
		if got != want {
			t.Fatal("logger() should return the configured logger")
		}
	})
}
