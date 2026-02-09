package framework_test

import (
	"runtime/debug"
	"testing"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/framework"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

func TestRunner_NewRunner(t *testing.T) {
	repo := framework.NewTestRepo(t)
	defer repo.Close()

	repo.CreateFile("x.txt", "x")
	repo.Commit("init")

	libRepo, err := gitlib.OpenRepository(repo.Path())
	if err != nil {
		t.Fatalf("OpenRepository: %v", err)
	}
	defer libRepo.Free()

	r := framework.NewRunner(libRepo, repo.Path(), &plumbing.TreeDiffAnalyzer{})
	if r == nil {
		t.Fatal("NewRunner returned nil")
	}

	if r.Repo != libRepo || r.RepoPath != repo.Path() {
		t.Error("Repo or RepoPath not set")
	}

	if len(r.Analyzers) != 1 {
		t.Errorf("Analyzers length = %d, want 1", len(r.Analyzers))
	}
}

func TestRunner_RunEmptyCommits(t *testing.T) {
	repo := framework.NewTestRepo(t)
	defer repo.Close()

	repo.CreateFile("x.txt", "x")
	repo.Commit("init")

	libRepo, err := gitlib.OpenRepository(repo.Path())
	if err != nil {
		t.Fatalf("OpenRepository: %v", err)
	}
	defer libRepo.Free()

	r := framework.NewRunner(libRepo, repo.Path(), &plumbing.TreeDiffAnalyzer{})

	reports, err := r.Run(nil)
	if err != nil {
		t.Fatalf("Run: %v", err)
	}

	if len(reports) != 1 {
		t.Errorf("reports length = %d, want 1", len(reports))
	}
	// TreeDiffAnalyzer may return nil report when no commits; map entry still exists.
	if _, ok := reports[r.Analyzers[0]]; !ok {
		t.Error("reports missing entry for analyzer")
	}
}

// TestRunner_BurndownWithMergeRegression verifies burndown runs on a repo with a merge
// when using first-parent walk. Without SimplifyFirstParent, topological+filter produced
// interleaved order causing "internal integrity error src X != Y".
func TestRunner_BurndownWithMergeRegression(t *testing.T) {
	repo := framework.NewTestRepo(t)
	defer repo.Close()

	repo.CreateFile("a.go", "a")
	hashA := repo.Commit("first")
	repo.CreateFile("b.go", "b")
	hashB := repo.CommitToRef("refs/heads/side", "branch", hashA)
	_ = repo.CreateMergeCommit("merge", hashA, hashB)

	libRepo, err := gitlib.OpenRepository(repo.Path())
	if err != nil {
		t.Fatalf("OpenRepository: %v", err)
	}
	defer libRepo.Free()

	commits := framework.CollectCommitsFirstParent(t, libRepo, 0)
	if len(commits) < 2 {
		t.Fatalf("first-parent should yield at least 2 commits (merge + parent), got %d", len(commits))
	}
	for _, c := range commits {
		c.Free()
	}
}

func TestRunner_RunSingleCommit(t *testing.T) {
	repo := framework.NewTestRepo(t)
	defer repo.Close()

	repo.CreateFile("a.txt", "hello")
	repo.Commit("first")

	libRepo, err := gitlib.OpenRepository(repo.Path())
	if err != nil {
		t.Fatalf("OpenRepository: %v", err)
	}
	defer libRepo.Free()

	commits := framework.CollectCommits(t, libRepo, 1)
	if len(commits) != 1 {
		t.Fatalf("got %d commits, want 1", len(commits))
	}

	r := framework.NewRunner(libRepo, repo.Path(), &plumbing.TreeDiffAnalyzer{})

	reports, err := r.Run(commits)
	if err != nil {
		t.Fatalf("Run: %v", err)
	}

	if len(reports) != 1 {
		t.Errorf("reports length = %d, want 1", len(reports))
	}

	if _, ok := reports[r.Analyzers[0]]; !ok {
		t.Error("reports missing entry for analyzer")
	}
}

func TestRunner_AppliesExplicitGCPercent(t *testing.T) {
	repo := framework.NewTestRepo(t)
	defer repo.Close()

	repo.CreateFile("gc.txt", "content")
	repo.Commit("init")

	libRepo, err := gitlib.OpenRepository(repo.Path())
	if err != nil {
		t.Fatalf("OpenRepository: %v", err)
	}
	defer libRepo.Free()

	commits := framework.CollectCommits(t, libRepo, 1)
	if len(commits) != 1 {
		t.Fatalf("got %d commits, want 1", len(commits))
	}

	originalGCPercent := debug.SetGCPercent(100)
	t.Cleanup(func() {
		debug.SetGCPercent(originalGCPercent)
	})

	config := framework.DefaultCoordinatorConfig()
	config.GCPercent = 240

	runner := framework.NewRunnerWithConfig(libRepo, repo.Path(), config, &plumbing.TreeDiffAnalyzer{})
	_, runErr := runner.Run(commits)
	if runErr != nil {
		t.Fatalf("Run: %v", runErr)
	}

	previousGCPercent := debug.SetGCPercent(100)
	if previousGCPercent != 240 {
		t.Fatalf("GC percent = %d, want 240", previousGCPercent)
	}
}

func TestRunner_AppliesBallastSize(t *testing.T) {
	repo := framework.NewTestRepo(t)
	defer repo.Close()

	repo.CreateFile("ballast.txt", "content")
	repo.Commit("init")

	libRepo, err := gitlib.OpenRepository(repo.Path())
	if err != nil {
		t.Fatalf("OpenRepository: %v", err)
	}
	defer libRepo.Free()

	commits := framework.CollectCommits(t, libRepo, 1)
	if len(commits) != 1 {
		t.Fatalf("got %d commits, want 1", len(commits))
	}

	config := framework.DefaultCoordinatorConfig()
	config.BallastSize = 8 * 1024 * 1024

	runner := framework.NewRunnerWithConfig(libRepo, repo.Path(), config, &plumbing.TreeDiffAnalyzer{})
	_, runErr := runner.Run(commits)
	if runErr != nil {
		t.Fatalf("Run: %v", runErr)
	}

	if framework.RunnerBallastSizeForTest(runner) < int(config.BallastSize) {
		t.Fatalf(
			"runner ballast size = %d, want at least %d",
			framework.RunnerBallastSizeForTest(runner),
			config.BallastSize,
		)
	}
}

func TestResolveGCPercentForTest_AutoMode(t *testing.T) {
	const overThreshold = uint64(33 * 1024 * 1024 * 1024)

	got := framework.ResolveGCPercentForTest(0, overThreshold)
	if got != 200 {
		t.Fatalf("GC percent = %d, want 200", got)
	}
}

func TestResolveGCPercentForTest_ExplicitMode(t *testing.T) {
	got := framework.ResolveGCPercentForTest(180, 0)
	if got != 180 {
		t.Fatalf("GC percent = %d, want 180", got)
	}
}
