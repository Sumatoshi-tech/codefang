package framework_test

import (
	"io"
	"runtime/debug"
	"testing"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/framework"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
	"github.com/Sumatoshi-tech/codefang/pkg/pipeline"
)

// stubLeaf is a minimal HistoryAnalyzer + Parallelizable stub for testing dispatch logic.
type stubLeaf struct {
	name           string
	sequentialOnly bool
	cpuHeavy       bool
	consumed       int
}

func (s *stubLeaf) Name() string         { return s.name }
func (s *stubLeaf) Flag() string         { return s.name }
func (s *stubLeaf) SequentialOnly() bool { return s.sequentialOnly }
func (s *stubLeaf) CPUHeavy() bool       { return s.cpuHeavy }

func (s *stubLeaf) Descriptor() analyze.Descriptor {
	return analyze.Descriptor{ID: s.name, Description: s.name, Mode: analyze.ModeHistory}
}

func (s *stubLeaf) ListConfigurationOptions() []pipeline.ConfigurationOption { return nil }
func (s *stubLeaf) Configure(_ map[string]any) error                         { return nil }

func (s *stubLeaf) Initialize(_ *gitlib.Repository) error { return nil }
func (s *stubLeaf) Consume(_ *analyze.Context) error {
	s.consumed++

	return nil
}
func (s *stubLeaf) Finalize() (analyze.Report, error) { return analyze.Report{}, nil }

func (s *stubLeaf) Fork(n int) []analyze.HistoryAnalyzer {
	forks := make([]analyze.HistoryAnalyzer, n)
	for i := range n {
		forks[i] = &stubLeaf{name: s.name, sequentialOnly: s.sequentialOnly, cpuHeavy: s.cpuHeavy}
	}

	return forks
}

func (s *stubLeaf) Merge(_ []analyze.HistoryAnalyzer)                       {}
func (s *stubLeaf) Serialize(_ analyze.Report, _ string, _ io.Writer) error { return nil }
func (s *stubLeaf) SnapshotPlumbing() analyze.PlumbingSnapshot              { return nil }
func (s *stubLeaf) ApplySnapshot(_ analyze.PlumbingSnapshot)                {}
func (s *stubLeaf) ReleaseSnapshot(_ analyze.PlumbingSnapshot)              {}

func TestRunner_NewRunner(t *testing.T) {
	t.Parallel()

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
	t.Parallel()

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
	t.Parallel()

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
	t.Parallel()

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
	t.Parallel()

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
	t.Parallel()

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

func TestResolveMemoryLimit_CappedAt14GiB(t *testing.T) {
	t.Parallel()

	const totalRAM = uint64(64 * 1024 * 1024 * 1024) // 64 GiB.

	got := framework.ResolveMemoryLimitForTest(totalRAM)

	// 75% of 64 GiB = 48 GiB, but capped at 14 GiB.
	const want = uint64(14 * 1024 * 1024 * 1024)
	if got != want {
		t.Fatalf("memory limit = %d, want %d", got, want)
	}
}

func TestResolveMemoryLimit_SmallSystem(t *testing.T) {
	t.Parallel()

	const totalRAM = uint64(8 * 1024 * 1024 * 1024) // 8 GiB.

	got := framework.ResolveMemoryLimitForTest(totalRAM)

	// 75% of 8 GiB = 6 GiB, which is less than the 14 GiB cap.
	const want = uint64(6 * 1024 * 1024 * 1024)
	if got != want {
		t.Fatalf("memory limit = %d, want %d", got, want)
	}
}

func TestResolveMemoryLimit_UnknownSystem(t *testing.T) {
	t.Parallel()

	got := framework.ResolveMemoryLimitForTest(0)

	// Falls back to 14 GiB default.
	const want = uint64(14 * 1024 * 1024 * 1024)
	if got != want {
		t.Fatalf("memory limit = %d, want %d", got, want)
	}
}

func TestSplitLeaves_ThreeGroups(t *testing.T) {
	t.Parallel()

	serial := &stubLeaf{name: "serial", sequentialOnly: true, cpuHeavy: false}
	lightweight := &stubLeaf{name: "lightweight", sequentialOnly: false, cpuHeavy: false}
	heavy := &stubLeaf{name: "heavy", sequentialOnly: false, cpuHeavy: true}

	repo := framework.NewTestRepo(t)
	defer repo.Close()

	repo.CreateFile("x.txt", "x")
	repo.Commit("init")

	libRepo, err := gitlib.OpenRepository(repo.Path())
	if err != nil {
		t.Fatalf("OpenRepository: %v", err)
	}
	defer libRepo.Free()

	// Core placeholder at index 0; leaves start at CoreCount=1.
	core := &plumbing.TreeDiffAnalyzer{}
	runner := framework.NewRunner(libRepo, repo.Path(), core, serial, lightweight, heavy)
	runner.CoreCount = 1

	cpuHeavy, lw, ser := framework.SplitLeavesForTest(runner)

	if len(cpuHeavy) != 1 || cpuHeavy[0].Name() != "heavy" {
		t.Errorf("cpuHeavy = %v, want [heavy]", analyzerNames(cpuHeavy))
	}

	if len(lw) != 1 || lw[0].Name() != "lightweight" {
		t.Errorf("lightweight = %v, want [lightweight]", analyzerNames(lw))
	}

	if len(ser) != 1 || ser[0].Name() != "serial" {
		t.Errorf("serial = %v, want [serial]", analyzerNames(ser))
	}
}

func TestSplitLeaves_NoCPUHeavy(t *testing.T) {
	t.Parallel()

	serial := &stubLeaf{name: "serial", sequentialOnly: true, cpuHeavy: false}
	lightweight := &stubLeaf{name: "lightweight", sequentialOnly: false, cpuHeavy: false}

	repo := framework.NewTestRepo(t)
	defer repo.Close()

	repo.CreateFile("x.txt", "x")
	repo.Commit("init")

	libRepo, err := gitlib.OpenRepository(repo.Path())
	if err != nil {
		t.Fatalf("OpenRepository: %v", err)
	}
	defer libRepo.Free()

	core := &plumbing.TreeDiffAnalyzer{}
	runner := framework.NewRunner(libRepo, repo.Path(), core, serial, lightweight)
	runner.CoreCount = 1

	cpuHeavy, lw, ser := framework.SplitLeavesForTest(runner)

	if len(cpuHeavy) != 0 {
		t.Errorf("cpuHeavy = %v, want empty", analyzerNames(cpuHeavy))
	}

	if len(lw) != 1 || lw[0].Name() != "lightweight" {
		t.Errorf("lightweight = %v, want [lightweight]", analyzerNames(lw))
	}

	if len(ser) != 1 || ser[0].Name() != "serial" {
		t.Errorf("serial = %v, want [serial]", analyzerNames(ser))
	}
}

func TestSplitLeaves_AllCPUHeavy(t *testing.T) {
	t.Parallel()

	heavy1 := &stubLeaf{name: "heavy1", sequentialOnly: false, cpuHeavy: true}
	heavy2 := &stubLeaf{name: "heavy2", sequentialOnly: false, cpuHeavy: true}

	repo := framework.NewTestRepo(t)
	defer repo.Close()

	repo.CreateFile("x.txt", "x")
	repo.Commit("init")

	libRepo, err := gitlib.OpenRepository(repo.Path())
	if err != nil {
		t.Fatalf("OpenRepository: %v", err)
	}
	defer libRepo.Free()

	core := &plumbing.TreeDiffAnalyzer{}
	runner := framework.NewRunner(libRepo, repo.Path(), core, heavy1, heavy2)
	runner.CoreCount = 1

	cpuHeavy, lw, ser := framework.SplitLeavesForTest(runner)

	if len(cpuHeavy) != 2 {
		t.Errorf("cpuHeavy = %v, want [heavy1, heavy2]", analyzerNames(cpuHeavy))
	}

	if len(lw) != 0 {
		t.Errorf("lightweight = %v, want empty", analyzerNames(lw))
	}

	if len(ser) != 0 {
		t.Errorf("serial = %v, want empty", analyzerNames(ser))
	}
}

func analyzerNames(analyzers []analyze.HistoryAnalyzer) []string {
	names := make([]string, len(analyzers))
	for i, a := range analyzers {
		names[i] = a.Name()
	}

	return names
}
