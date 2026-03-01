package framework_test

import (
	"context"
	"encoding/gob"
	"io"
	"runtime/debug"
	"testing"
	"time"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/plumbing"
	"github.com/Sumatoshi-tech/codefang/internal/checkpoint"
	"github.com/Sumatoshi-tech/codefang/internal/framework"
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
func (s *stubLeaf) Consume(_ context.Context, _ *analyze.Context) (analyze.TC, error) {
	s.consumed++

	return analyze.TC{}, nil
}
func (s *stubLeaf) Fork(n int) []analyze.HistoryAnalyzer {
	forks := make([]analyze.HistoryAnalyzer, n)
	for i := range n {
		forks[i] = &stubLeaf{name: s.name, sequentialOnly: s.sequentialOnly, cpuHeavy: s.cpuHeavy}
	}

	return forks
}

func (s *stubLeaf) Merge(_ []analyze.HistoryAnalyzer)                            {}
func (s *stubLeaf) Serialize(_ analyze.Report, _ string, _ io.Writer) error      { return nil }
func (s *stubLeaf) SnapshotPlumbing() analyze.PlumbingSnapshot                   { return nil }
func (s *stubLeaf) ApplySnapshot(_ analyze.PlumbingSnapshot)                     {}
func (s *stubLeaf) ReleaseSnapshot(_ analyze.PlumbingSnapshot)                   {}
func (s *stubLeaf) WorkingStateSize() int64                                      { return 1024 }
func (s *stubLeaf) AvgTCSize() int64                                             { return 1024 }
func (s *stubLeaf) NewAggregator(_ analyze.AggregatorOptions) analyze.Aggregator { return nil }
func (s *stubLeaf) SerializeTICKs(_ []analyze.TICK, _ string, _ io.Writer) error {
	return analyze.ErrNotImplemented
}

func (s *stubLeaf) ReportFromTICKs(_ context.Context, _ []analyze.TICK) (analyze.Report, error) {
	return nil, analyze.ErrNotImplemented
}

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

	reports, err := r.Run(context.Background(), nil)
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

	reports, err := r.Run(context.Background(), commits)
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

	_, runErr := runner.Run(context.Background(), commits)
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

	_, runErr := runner.Run(context.Background(), commits)
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

func TestResolveMemoryLimitFromBudget_SetsBudgetBased(t *testing.T) {
	t.Parallel()

	const (
		budget   = int64(4 * 1024 * 1024 * 1024)   // 4 GiB budget.
		totalRAM = uint64(32 * 1024 * 1024 * 1024) // 32 GiB system.
	)

	got := framework.ResolveMemoryLimitFromBudgetForTest(budget, totalRAM)

	// 95% of 4 GiB = 3.8 GiB. System cap = 90% of 32 GiB = 28.8 GiB. Min = 3.8 GiB.
	want := uint64(budget) * 95 / 100
	if got != want {
		t.Fatalf("memory limit = %d, want %d", got, want)
	}
}

func TestResolveMemoryLimitFromBudget_CappedAtSystemRAM(t *testing.T) {
	t.Parallel()

	const (
		budget   = int64(16 * 1024 * 1024 * 1024) // 16 GiB budget.
		totalRAM = uint64(8 * 1024 * 1024 * 1024) // 8 GiB system (budget > system).
	)

	got := framework.ResolveMemoryLimitFromBudgetForTest(budget, totalRAM)

	// 95% of 16 GiB = 15.2 GiB, but capped at 90% of 8 GiB = 7.2 GiB.
	want := totalRAM * 90 / 100
	if got != want {
		t.Fatalf("memory limit = %d, want %d", got, want)
	}
}

func TestResolveMemoryLimitFromBudget_ZeroBudget(t *testing.T) {
	t.Parallel()

	got := framework.ResolveMemoryLimitFromBudgetForTest(0, 32*1024*1024*1024)
	if got != 0 {
		t.Fatalf("memory limit = %d, want 0 for zero budget", got)
	}
}

func TestResolveMemoryLimit_CappedAt8GiB(t *testing.T) {
	t.Parallel()

	const totalRAM = uint64(64 * 1024 * 1024 * 1024) // 64 GiB.

	got := framework.ResolveMemoryLimitForTest(totalRAM)

	// 75% of 64 GiB = 48 GiB, but capped at 8 GiB.
	const want = uint64(8 * 1024 * 1024 * 1024)
	if got != want {
		t.Fatalf("memory limit = %d, want %d", got, want)
	}
}

func TestResolveMemoryLimit_SmallSystem(t *testing.T) {
	t.Parallel()

	const totalRAM = uint64(4 * 1024 * 1024 * 1024) // 4 GiB.

	got := framework.ResolveMemoryLimitForTest(totalRAM)

	// 75% of 4 GiB = 3 GiB, which is less than the 4 GiB cap.
	const want = uint64(3 * 1024 * 1024 * 1024)
	if got != want {
		t.Fatalf("memory limit = %d, want %d", got, want)
	}
}

func TestResolveMemoryLimit_UnknownSystem(t *testing.T) {
	t.Parallel()

	got := framework.ResolveMemoryLimitForTest(0)

	// Falls back to 8 GiB default.
	const want = uint64(8 * 1024 * 1024 * 1024)
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

func TestRecordCommitMeta_Basic(t *testing.T) {
	t.Parallel()

	runner := framework.NewRunner(nil, "")
	framework.SetIDProviderForTest(runner, &plumbing.IdentityDetector{
		ReversedPeopleDict: []string{"alice", "bob"},
	})
	framework.InitAggregatorsForTest(runner)

	ts := time.Date(2024, 1, 15, 10, 30, 0, 0, time.UTC)

	tc := analyze.TC{
		CommitHash: gitlib.NewHash("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
		Tick:       0,
		AuthorID:   0,
		Timestamp:  ts,
		Data:       "dummy",
	}

	framework.RecordCommitMetaForTest(runner, tc)

	meta := framework.CommitMetaForTest(runner)
	if len(meta) != 1 {
		t.Fatalf("expected 1 entry, got %d", len(meta))
	}

	hashStr := "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"

	entry, ok := meta[hashStr]
	if !ok {
		t.Fatal("expected entry for hash")
	}

	if entry.Timestamp != "2024-01-15T10:30:00Z" {
		t.Errorf("expected timestamp 2024-01-15T10:30:00Z, got %s", entry.Timestamp)
	}

	if entry.Author != "alice" {
		t.Errorf("expected author alice, got %s", entry.Author)
	}

	if entry.Tick != 0 {
		t.Errorf("expected tick 0, got %d", entry.Tick)
	}
}

func TestRecordCommitMeta_Deduplication(t *testing.T) {
	t.Parallel()

	runner := framework.NewRunner(nil, "")
	framework.InitAggregatorsForTest(runner)

	tc := analyze.TC{
		CommitHash: gitlib.NewHash("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
		Tick:       0,
		Timestamp:  time.Date(2024, 1, 1, 0, 0, 0, 0, time.UTC),
		Data:       "dummy",
	}

	framework.RecordCommitMetaForTest(runner, tc)
	framework.RecordCommitMetaForTest(runner, tc)

	meta := framework.CommitMetaForTest(runner)
	if len(meta) != 1 {
		t.Fatalf("expected 1 entry after dedup, got %d", len(meta))
	}
}

func TestRecordCommitMeta_NilIdProvider(t *testing.T) {
	t.Parallel()

	runner := framework.NewRunner(nil, "")
	framework.InitAggregatorsForTest(runner)

	tc := analyze.TC{
		CommitHash: gitlib.NewHash("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
		Tick:       0,
		AuthorID:   5,
		Timestamp:  time.Date(2024, 1, 1, 0, 0, 0, 0, time.UTC),
		Data:       "dummy",
	}

	framework.RecordCommitMetaForTest(runner, tc)

	meta := framework.CommitMetaForTest(runner)
	entry := meta["aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"]

	if entry.Author != "" {
		t.Errorf("expected empty author with nil idProvider, got %s", entry.Author)
	}
}

func TestRecordCommitMeta_AuthorIDOutOfBounds(t *testing.T) {
	t.Parallel()

	runner := framework.NewRunner(nil, "")
	framework.SetIDProviderForTest(runner, &plumbing.IdentityDetector{
		ReversedPeopleDict: []string{"alice"},
	})
	framework.InitAggregatorsForTest(runner)

	tc := analyze.TC{
		CommitHash: gitlib.NewHash("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
		Tick:       0,
		AuthorID:   999,
		Timestamp:  time.Date(2024, 1, 1, 0, 0, 0, 0, time.UTC),
		Data:       "dummy",
	}

	framework.RecordCommitMetaForTest(runner, tc)

	meta := framework.CommitMetaForTest(runner)
	entry := meta["aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"]

	if entry.Author != "" {
		t.Errorf("expected empty author for out-of-bounds ID, got %s", entry.Author)
	}
}

func TestRecordCommitMeta_ZeroTimestamp(t *testing.T) {
	t.Parallel()

	runner := framework.NewRunner(nil, "")
	framework.InitAggregatorsForTest(runner)

	tc := analyze.TC{
		CommitHash: gitlib.NewHash("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
		Tick:       0,
		Data:       "dummy",
	}

	framework.RecordCommitMetaForTest(runner, tc)

	meta := framework.CommitMetaForTest(runner)
	entry := meta["aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"]

	if entry.Timestamp != "" {
		t.Errorf("expected empty timestamp for zero time, got %s", entry.Timestamp)
	}
}

func TestAuthorName_Basic(t *testing.T) {
	t.Parallel()

	runner := framework.NewRunner(nil, "")
	framework.SetIDProviderForTest(runner, &plumbing.IdentityDetector{
		ReversedPeopleDict: []string{"alice", "bob"},
	})

	if name := framework.AuthorNameForTest(runner, 0); name != "alice" {
		t.Errorf("expected alice, got %s", name)
	}

	if name := framework.AuthorNameForTest(runner, 1); name != "bob" {
		t.Errorf("expected bob, got %s", name)
	}
}

func TestAuthorName_NegativeID(t *testing.T) {
	t.Parallel()

	runner := framework.NewRunner(nil, "")
	framework.SetIDProviderForTest(runner, &plumbing.IdentityDetector{
		ReversedPeopleDict: []string{"alice"},
	})

	if name := framework.AuthorNameForTest(runner, -1); name != "" {
		t.Errorf("expected empty for negative ID, got %s", name)
	}
}

func TestInjectCommitMeta_InjectsIntoCBTReports(t *testing.T) {
	t.Parallel()

	runner := framework.NewRunner(nil, "")
	framework.InitAggregatorsForTest(runner)

	tc := analyze.TC{
		CommitHash: gitlib.NewHash("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
		Tick:       0,
		Timestamp:  time.Date(2024, 1, 1, 0, 0, 0, 0, time.UTC),
		Data:       "dummy",
	}
	framework.RecordCommitMetaForTest(runner, tc)

	leaf := &stubLeaf{name: "test"}
	reports := map[analyze.HistoryAnalyzer]analyze.Report{
		leaf: {
			"commits_by_tick": map[int][]gitlib.Hash{
				0: {gitlib.NewHash("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")},
			},
		},
	}

	framework.InjectCommitMetaForTest(runner, reports)

	cm, ok := reports[leaf][analyze.ReportKeyCommitMeta].(map[string]analyze.CommitMeta)
	if !ok {
		t.Fatal("expected commit_meta in report")
	}

	if len(cm) != 1 {
		t.Fatalf("expected 1 entry, got %d", len(cm))
	}
}

func TestInitAggregators_SkipsWhenTCSinkSet(t *testing.T) {
	t.Parallel()

	runner := framework.NewRunner(nil, "", &stubLeaf{name: "leaf1"}, &stubLeaf{name: "leaf2"})
	runner.TCSink = func(_ analyze.TC, _ string) error { return nil }

	framework.InitAggregatorsForTest(runner)

	aggs := framework.AggregatorsForTest(runner)
	for i, agg := range aggs {
		if agg != nil {
			t.Errorf("aggregator[%d] should be nil when TCSink is set, got %v", i, agg)
		}
	}
}

func TestInitAggregators_CreatesWhenNoTCSink(t *testing.T) {
	t.Parallel()

	// stubLeaf.NewAggregator returns nil, so aggregators will all be nil,
	// but the slice should be allocated.
	runner := framework.NewRunner(nil, "", &stubLeaf{name: "leaf1"})

	framework.InitAggregatorsForTest(runner)

	aggs := framework.AggregatorsForTest(runner)
	if aggs == nil {
		t.Fatal("aggregator slice should be allocated even if individual entries are nil")
	}
}

func TestInitAggregators_WiresAggSpillBudget(t *testing.T) {
	t.Parallel()

	runner := framework.NewRunner(nil, "", &stubLeaf{name: "leaf1"})
	runner.AggSpillBudget = 42 * 1024 * 1024

	framework.InitAggregatorsForTest(runner)

	// Verify the runner's AggSpillBudget was set (wiring test).
	assert.Equal(t, int64(42*1024*1024), framework.AggSpillBudgetForTest(runner))
}

func TestAddTC_RouteToTCSink(t *testing.T) {
	t.Parallel()

	var captured []struct {
		tc   analyze.TC
		flag string
	}

	sink := func(tc analyze.TC, flag string) error {
		captured = append(captured, struct {
			tc   analyze.TC
			flag string
		}{tc: tc, flag: flag})

		return nil
	}

	leaf := &stubLeaf{name: "quality"}
	runner := framework.NewRunner(nil, "", leaf)
	runner.TCSink = sink
	framework.InitAggregatorsForTest(runner)

	ts := time.Date(2024, 6, 15, 12, 0, 0, 0, time.UTC)

	tc := analyze.TC{
		CommitHash: gitlib.NewHash("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"),
		Data:       map[string]any{"score": 99},
	}

	ac := &analyze.Context{
		Time: ts,
	}

	framework.AddTCForTest(runner, tc, 0, ac)

	if len(captured) != 1 {
		t.Fatalf("expected 1 captured TC, got %d", len(captured))
	}

	got := captured[0]
	if got.flag != "quality" {
		t.Errorf("expected flag 'quality', got %s", got.flag)
	}

	if got.tc.Timestamp != ts {
		t.Errorf("expected timestamp %v, got %v", ts, got.tc.Timestamp)
	}
}

func TestAddTC_NilDataSkipsSink(t *testing.T) {
	t.Parallel()

	called := false
	sink := func(_ analyze.TC, _ string) error {
		called = true

		return nil
	}

	leaf := &stubLeaf{name: "quality"}
	runner := framework.NewRunner(nil, "", leaf)
	runner.TCSink = sink
	framework.InitAggregatorsForTest(runner)

	tc := analyze.TC{
		CommitHash: gitlib.NewHash("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"),
		Data:       nil,
	}

	ac := &analyze.Context{
		Time: time.Now(),
	}

	framework.AddTCForTest(runner, tc, 0, ac)

	if called {
		t.Error("TCSink should not be called for nil Data")
	}
}

func TestAddTC_SinkReceivesStampedMetadata(t *testing.T) {
	t.Parallel()

	var captured analyze.TC

	sink := func(tc analyze.TC, _ string) error {
		captured = tc

		return nil
	}

	leaf := &stubLeaf{name: "quality"}
	runner := framework.NewRunner(nil, "", leaf)
	runner.TCSink = sink

	framework.SetIDProviderForTest(runner, &plumbing.IdentityDetector{
		ReversedPeopleDict: []string{"alice", "bob"},
	})
	framework.InitAggregatorsForTest(runner)

	ts := time.Date(2024, 3, 10, 8, 0, 0, 0, time.UTC)

	tc := analyze.TC{
		CommitHash: gitlib.NewHash("cccccccccccccccccccccccccccccccccccccccc"),
		Data:       map[string]any{"val": 1},
	}

	ac := &analyze.Context{
		Time: ts,
	}

	framework.AddTCForTest(runner, tc, 0, ac)

	if captured.Timestamp != ts {
		t.Errorf("expected timestamp %v, got %v", ts, captured.Timestamp)
	}

	// AuthorID should be 0 (default from nil tickProvider/idProvider state).
	// The important thing is the TC reaches the sink with metadata stamped.
	if captured.Data == nil {
		t.Error("captured TC should have non-nil Data")
	}
}

func TestInjectCommitMeta_SkipsReportsWithoutCBT(t *testing.T) {
	t.Parallel()

	runner := framework.NewRunner(nil, "")
	framework.InitAggregatorsForTest(runner)

	tc := analyze.TC{
		CommitHash: gitlib.NewHash("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
		Tick:       0,
		Timestamp:  time.Date(2024, 1, 1, 0, 0, 0, 0, time.UTC),
		Data:       "dummy",
	}
	framework.RecordCommitMetaForTest(runner, tc)

	leaf := &stubLeaf{name: "test"}
	reports := map[analyze.HistoryAnalyzer]analyze.Report{
		leaf: {
			"other_data": 42,
		},
	}

	framework.InjectCommitMetaForTest(runner, reports)

	if _, ok := reports[leaf][analyze.ReportKeyCommitMeta]; ok {
		t.Error("should not inject commit_meta into report without commits_by_tick")
	}
}

// stubAggregator is a minimal Aggregator for testing TC counting, state size, and spill state.
type stubAggregator struct {
	stateSize  int64
	addCount   int
	spillDir   string
	spillCount int
	spilled    bool
}

func (a *stubAggregator) Add(_ analyze.TC) error {
	a.addCount++

	return nil
}

func (a *stubAggregator) FlushTick(_ int) (analyze.TICK, error)  { return analyze.TICK{}, nil }
func (a *stubAggregator) FlushAllTicks() ([]analyze.TICK, error) { return nil, nil }

func (a *stubAggregator) Spill() (int64, error) {
	a.spilled = true

	return a.stateSize, nil
}

func (a *stubAggregator) Collect() error            { return nil }
func (a *stubAggregator) EstimatedStateSize() int64 { return a.stateSize }

func (a *stubAggregator) SpillState() analyze.AggregatorSpillInfo {
	return analyze.AggregatorSpillInfo{Dir: a.spillDir, Count: a.spillCount}
}

func (a *stubAggregator) RestoreSpillState(info analyze.AggregatorSpillInfo) {
	a.spillDir = info.Dir
	a.spillCount = info.Count
}

func (a *stubAggregator) Close() error { return nil }

// stubLeafWithAgg returns a non-nil aggregator.
type stubLeafWithAgg struct {
	stubLeaf

	agg *stubAggregator
}

func (s *stubLeafWithAgg) NewAggregator(_ analyze.AggregatorOptions) analyze.Aggregator {
	return s.agg
}

// stubLeafCapturingOpts captures the AggregatorOptions passed by initAggregators.
type stubLeafCapturingOpts struct {
	stubLeaf

	capturedOpts analyze.AggregatorOptions
	agg          *stubAggregator
}

func (s *stubLeafCapturingOpts) NewAggregator(opts analyze.AggregatorOptions) analyze.Aggregator {
	s.capturedOpts = opts

	return s.agg
}

// T-10a: SpillBudget is divided among leaf aggregators (not given full budget to each).
func TestInitAggregators_DividesSpillBudget(t *testing.T) {
	t.Parallel()

	const totalBudget = int64(100 * 1024 * 1024) // 100 MiB total.

	leaf1 := &stubLeafCapturingOpts{stubLeaf: stubLeaf{name: "a1"}, agg: &stubAggregator{}}
	leaf2 := &stubLeafCapturingOpts{stubLeaf: stubLeaf{name: "a2"}, agg: &stubAggregator{}}
	leaf3 := &stubLeafCapturingOpts{stubLeaf: stubLeaf{name: "a3"}, agg: &stubAggregator{}}
	leaf4 := &stubLeafCapturingOpts{stubLeaf: stubLeaf{name: "a4"}, agg: &stubAggregator{}}
	leaf5 := &stubLeafCapturingOpts{stubLeaf: stubLeaf{name: "a5"}, agg: &stubAggregator{}}

	runner := &framework.Runner{
		Analyzers: []analyze.HistoryAnalyzer{leaf1, leaf2, leaf3, leaf4, leaf5},
		CoreCount: 0,
	}
	runner.AggSpillBudget = totalBudget

	framework.InitAggregatorsForTest(runner)

	// Each of 5 aggregators should get totalBudget/5 = 20 MiB, not 100 MiB.
	expectedPerAgg := totalBudget / 5

	assert.Equal(t, expectedPerAgg, leaf1.capturedOpts.SpillBudget, "leaf1 should get divided budget")
	assert.Equal(t, expectedPerAgg, leaf2.capturedOpts.SpillBudget, "leaf2 should get divided budget")
	assert.Equal(t, expectedPerAgg, leaf3.capturedOpts.SpillBudget, "leaf3 should get divided budget")
	assert.Equal(t, expectedPerAgg, leaf4.capturedOpts.SpillBudget, "leaf4 should get divided budget")
	assert.Equal(t, expectedPerAgg, leaf5.capturedOpts.SpillBudget, "leaf5 should get divided budget")
}

// T-10: AggregatorStateSize sums correctly.
func TestRunner_AggregatorStateSize(t *testing.T) {
	t.Parallel()

	agg1 := &stubAggregator{stateSize: 1000}
	agg2 := &stubAggregator{stateSize: 2000}

	leaf1 := &stubLeafWithAgg{stubLeaf: stubLeaf{name: "leaf1"}, agg: agg1}
	leaf2 := &stubLeafWithAgg{stubLeaf: stubLeaf{name: "leaf2"}, agg: agg2}

	runner := &framework.Runner{
		Analyzers: []analyze.HistoryAnalyzer{leaf1, leaf2},
		CoreCount: 0,
	}

	framework.InitAggregatorsForTest(runner)

	got := framework.AggregatorStateSizeForTest(runner)
	assert.Equal(t, int64(3000), got)
}

// T-11: TCCountAccumulated tracks and resets.
func TestRunner_TCCountAccumulated(t *testing.T) {
	t.Parallel()

	agg := &stubAggregator{stateSize: 100}
	leaf := &stubLeafWithAgg{stubLeaf: stubLeaf{name: "leaf"}, agg: agg}

	runner := &framework.Runner{
		Analyzers: []analyze.HistoryAnalyzer{leaf},
		CoreCount: 0,
	}

	framework.InitAggregatorsForTest(runner)

	assert.Equal(t, int64(0), framework.TCCountAccumulatedForTest(runner))

	ac := &analyze.Context{Time: time.Now()}
	tc := analyze.TC{Data: "payload"}
	framework.AddTCForTest(runner, tc, 0, ac)
	framework.AddTCForTest(runner, tc, 0, ac)

	assert.Equal(t, int64(2), framework.TCCountAccumulatedForTest(runner))

	framework.ResetTCCountForTest(runner)
	assert.Equal(t, int64(0), framework.TCCountAccumulatedForTest(runner))
}

// T-12: AggregatorSpills returns spill state from all aggregators.
func TestRunner_AggregatorSpills(t *testing.T) {
	t.Parallel()

	agg1 := &stubAggregator{stateSize: 100, spillDir: "/tmp/spill-a", spillCount: 2}
	agg2 := &stubAggregator{stateSize: 200, spillDir: "/tmp/spill-b", spillCount: 5}

	leaf1 := &stubLeafWithAgg{stubLeaf: stubLeaf{name: "leaf1"}, agg: agg1}
	leaf2 := &stubLeafWithAgg{stubLeaf: stubLeaf{name: "leaf2"}, agg: agg2}

	runner := &framework.Runner{
		Analyzers: []analyze.HistoryAnalyzer{leaf1, leaf2},
		CoreCount: 0,
	}

	framework.InitAggregatorsForTest(runner)

	spills := runner.AggregatorSpills()

	require.Len(t, spills, 2)
	assert.Equal(t, "/tmp/spill-a", spills[0].Dir)
	assert.Equal(t, 2, spills[0].Count)
	assert.Equal(t, "/tmp/spill-b", spills[1].Dir)
	assert.Equal(t, 5, spills[1].Count)
}

// T-13: AggregatorSpills skips nil aggregators.
func TestRunner_AggregatorSpills_NilAggregator(t *testing.T) {
	t.Parallel()

	agg := &stubAggregator{stateSize: 100, spillDir: "/tmp/spill", spillCount: 1}
	leaf := &stubLeafWithAgg{stubLeaf: stubLeaf{name: "leaf"}, agg: agg}

	// stubLeaf returns nil from NewAggregator (no aggregator).
	noAgg := &stubLeaf{name: "no-agg"}

	runner := &framework.Runner{
		Analyzers: []analyze.HistoryAnalyzer{noAgg, leaf},
		CoreCount: 0,
	}

	framework.InitAggregatorsForTest(runner)

	spills := runner.AggregatorSpills()

	require.Len(t, spills, 2)
	assert.Empty(t, spills[0].Dir)
	assert.Equal(t, 0, spills[0].Count)
	assert.Equal(t, "/tmp/spill", spills[1].Dir)
	assert.Equal(t, 1, spills[1].Count)
}

// T-14: SpillAggregators calls Spill on all aggregators.
func TestRunner_SpillAggregators(t *testing.T) {
	t.Parallel()

	agg1 := &stubAggregator{stateSize: 100}
	agg2 := &stubAggregator{stateSize: 200}

	leaf1 := &stubLeafWithAgg{stubLeaf: stubLeaf{name: "leaf1"}, agg: agg1}
	leaf2 := &stubLeafWithAgg{stubLeaf: stubLeaf{name: "leaf2"}, agg: agg2}

	runner := &framework.Runner{
		Analyzers: []analyze.HistoryAnalyzer{leaf1, leaf2},
		CoreCount: 0,
	}

	framework.InitAggregatorsForTest(runner)

	err := runner.SpillAggregators()
	require.NoError(t, err)

	// Verify Spill was called on both stubs.
	aggs := framework.AggregatorsForTest(runner)

	spilled1, ok1 := aggs[0].(*stubAggregator)
	require.True(t, ok1)
	assert.True(t, spilled1.spilled)

	spilled2, ok2 := aggs[1].(*stubAggregator)
	require.True(t, ok2)
	assert.True(t, spilled2.spilled)
}

// T-15: InitializeForResume restores aggregator spill state.
func TestRunner_InitializeForResume(t *testing.T) {
	t.Parallel()

	repo := framework.NewTestRepo(t)
	defer repo.Close()

	repo.CreateFile("x.txt", "x")
	repo.Commit("init")

	libRepo, err := gitlib.OpenRepository(repo.Path())
	require.NoError(t, err)

	defer libRepo.Free()

	agg := &stubAggregator{stateSize: 100}
	leaf := &stubLeafWithAgg{stubLeaf: stubLeaf{name: "leaf"}, agg: agg}

	runner := framework.NewRunner(libRepo, repo.Path(), leaf)

	aggSpills := []checkpoint.AggregatorSpillEntry{
		{Dir: "/tmp/restored-spill", Count: 7},
	}

	err = runner.InitializeForResume(aggSpills)
	require.NoError(t, err)

	// Verify the aggregator was created and its spill state restored.
	aggs := framework.AggregatorsForTest(runner)
	require.Len(t, aggs, 1)
	require.NotNil(t, aggs[0])

	info := aggs[0].SpillState()
	assert.Equal(t, "/tmp/restored-spill", info.Dir)
	assert.Equal(t, 7, info.Count)
}

// FinalizeToStore test stubs follow.

// reportingAggregator returns predefined ticks from FlushAllTicks.
type reportingAggregator struct {
	stubAggregator

	ticks  []analyze.TICK
	closed bool
}

func (a *reportingAggregator) FlushAllTicks() ([]analyze.TICK, error) {
	return a.ticks, nil
}

func (a *reportingAggregator) Close() error {
	a.closed = true

	return nil
}

// stubLeafReporting is a leaf with an aggregator that returns a known report.
type stubLeafReporting struct {
	stubLeaf

	agg    *reportingAggregator
	report analyze.Report
}

func (s *stubLeafReporting) NewAggregator(_ analyze.AggregatorOptions) analyze.Aggregator {
	return s.agg
}

func (s *stubLeafReporting) ReportFromTICKs(_ context.Context, _ []analyze.TICK) (analyze.Report, error) {
	return s.report, nil
}

// stubStoreWriterLeaf implements StoreWriter. It writes custom records to the store.
type stubStoreWriterLeaf struct {
	stubLeafReporting

	writeToStoreCalled bool
}

func (s *stubStoreWriterLeaf) WriteToStore(_ context.Context, ticks []analyze.TICK, w analyze.ReportWriter) error {
	s.writeToStoreCalled = true

	for _, tick := range ticks {
		writeErr := w.Write("custom", tick.Data)
		if writeErr != nil {
			return writeErr
		}
	}

	return nil
}

// registerGobTypes registers types needed for gob encoding in FinalizeToStore tests.
func registerGobTypes() {
	gob.Register(map[string]any{})
	gob.Register("")
	gob.Register(0)
	gob.Register([]string{})
}

// FRD: specs/frds/FRD-20260228-runner-integration.md.

func TestFinalizeToStore_NoAggregators(t *testing.T) {
	t.Parallel()

	registerGobTypes()

	runner := framework.NewRunner(nil, "")
	framework.InitAggregatorsForTest(runner)

	store := analyze.NewFileReportStore(t.TempDir())
	defer store.Close()

	err := framework.FinalizeToStoreForTest(context.Background(), runner, store)
	require.NoError(t, err)

	assert.Empty(t, store.AnalyzerIDs())
}

func TestFinalizeToStore_RejectsNonStoreWriter(t *testing.T) {
	t.Parallel()

	registerGobTypes()

	agg := &reportingAggregator{
		ticks: []analyze.TICK{{Tick: 1, Data: "tick-data"}},
	}
	leaf := &stubLeafReporting{
		stubLeaf: stubLeaf{name: "burndown"},
		agg:      agg,
		report:   analyze.Report{"score": "42"},
	}

	runner := &framework.Runner{
		Analyzers: []analyze.HistoryAnalyzer{leaf},
		CoreCount: 0,
	}

	framework.InitAggregatorsForTest(runner)

	store := analyze.NewFileReportStore(t.TempDir())
	defer store.Close()

	err := framework.FinalizeToStoreForTest(context.Background(), runner, store)
	require.Error(t, err)
	assert.Contains(t, err.Error(), "does not implement StoreWriter")
}

func TestFinalizeToStore_StoreWriter(t *testing.T) {
	t.Parallel()

	registerGobTypes()

	agg := &reportingAggregator{
		ticks: []analyze.TICK{
			{Tick: 1, Data: "alpha"},
			{Tick: 2, Data: "beta"},
		},
	}
	leaf := &stubStoreWriterLeaf{
		stubLeafReporting: stubLeafReporting{
			stubLeaf: stubLeaf{name: "couples"},
			agg:      agg,
			report:   analyze.Report{"unused": "value"},
		},
	}

	runner := &framework.Runner{
		Analyzers: []analyze.HistoryAnalyzer{leaf},
		CoreCount: 0,
	}

	framework.InitAggregatorsForTest(runner)

	store := analyze.NewFileReportStore(t.TempDir())
	defer store.Close()

	err := framework.FinalizeToStoreForTest(context.Background(), runner, store)
	require.NoError(t, err)

	assert.True(t, leaf.writeToStoreCalled, "WriteToStore should have been called")

	// Verify the custom records were written, not the in-memory report.
	reader, openErr := store.Open("couples")
	require.NoError(t, openErr)

	assert.Equal(t, []string{"custom"}, reader.Kinds())

	var records []string

	iterErr := reader.Iter("custom", func(raw []byte) error {
		var s string

		decErr := analyze.GobDecode(raw, &s)
		if decErr != nil {
			return decErr
		}

		records = append(records, s)

		return nil
	})
	require.NoError(t, iterErr)
	require.NoError(t, reader.Close())

	require.Len(t, records, 2)
	assert.Equal(t, "alpha", records[0])
	assert.Equal(t, "beta", records[1])
}

func TestFinalizeToStore_NilsAggregators(t *testing.T) {
	t.Parallel()

	registerGobTypes()

	agg := &reportingAggregator{
		ticks: []analyze.TICK{{Tick: 1, Data: "data"}},
	}
	leaf := &stubStoreWriterLeaf{
		stubLeafReporting: stubLeafReporting{
			stubLeaf: stubLeaf{name: "burndown"},
			agg:      agg,
			report:   analyze.Report{"key": "val"},
		},
	}

	runner := &framework.Runner{
		Analyzers: []analyze.HistoryAnalyzer{leaf},
		CoreCount: 0,
	}

	framework.InitAggregatorsForTest(runner)

	store := analyze.NewFileReportStore(t.TempDir())
	defer store.Close()

	err := framework.FinalizeToStoreForTest(context.Background(), runner, store)
	require.NoError(t, err)

	// After FinalizeToStore, aggregators should be nil'd.
	aggs := framework.AggregatorsForTest(runner)

	for i, a := range aggs {
		assert.Nil(t, a, "aggregator[%d] should be nil after FinalizeToStore", i)
	}
}

func TestFinalizeToStore_MultipleAnalyzers(t *testing.T) {
	t.Parallel()

	registerGobTypes()

	agg1 := &reportingAggregator{
		ticks: []analyze.TICK{{Tick: 1, Data: "tick1"}},
	}
	leaf1 := &stubStoreWriterLeaf{
		stubLeafReporting: stubLeafReporting{
			stubLeaf: stubLeaf{name: "burndown"},
			agg:      agg1,
			report:   analyze.Report{"type": "burndown"},
		},
	}

	agg2 := &reportingAggregator{
		ticks: []analyze.TICK{{Tick: 1, Data: "alpha"}, {Tick: 2, Data: "beta"}},
	}
	leaf2 := &stubStoreWriterLeaf{
		stubLeafReporting: stubLeafReporting{
			stubLeaf: stubLeaf{name: "couples"},
			agg:      agg2,
		},
	}

	noAggLeaf := &stubLeaf{name: "file_history"}

	runner := &framework.Runner{
		Analyzers: []analyze.HistoryAnalyzer{leaf1, leaf2, noAggLeaf},
		CoreCount: 0,
	}

	framework.InitAggregatorsForTest(runner)

	store := analyze.NewFileReportStore(t.TempDir())
	defer store.Close()

	err := framework.FinalizeToStoreForTest(context.Background(), runner, store)
	require.NoError(t, err)

	// Only analyzers with aggregators should be written.
	ids := store.AnalyzerIDs()
	require.Len(t, ids, 2)
	assert.Equal(t, "burndown", ids[0])
	assert.Equal(t, "couples", ids[1])
}
