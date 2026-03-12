package framework_test

import (
	"context"
	"testing"

	"github.com/Sumatoshi-tech/codefang/internal/framework"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

func TestCoordinator_ProcessEmptyCommits(t *testing.T) {
	t.Parallel()

	repo := framework.NewTestRepo(t)
	defer repo.Close()

	repo.CreateFile("f.txt", "content")
	repo.Commit("init")

	libRepo, err := gitlib.OpenRepository(repo.Path())
	if err != nil {
		t.Fatalf("OpenRepository: %v", err)
	}
	defer libRepo.Free()

	config := framework.CoordinatorConfig{
		CommitBatchSize: 1,
		Workers:         1,
		BufferSize:      2,
		BatchConfig:     gitlib.DefaultBatchConfig(),
	}
	coord := framework.NewCoordinator(libRepo, config)
	ctx := context.Background()
	out := coord.Process(ctx, nil)

	n := 0
	for range out {
		n++
	}

	if n != 0 {
		t.Errorf("got %d results, want 0", n)
	}
}

func TestCoordinator_ProcessSingleCommit(t *testing.T) {
	t.Parallel()

	repo := framework.NewTestRepo(t)
	defer repo.Close()

	repo.CreateFile("only.txt", "hello")
	repo.Commit("initial")

	libRepo, err := gitlib.OpenRepository(repo.Path())
	if err != nil {
		t.Fatalf("OpenRepository: %v", err)
	}
	defer libRepo.Free()

	commits := framework.CollectCommits(t, libRepo, 1)
	if len(commits) != 1 {
		t.Fatalf("got %d commits, want 1", len(commits))
	}

	config := framework.CoordinatorConfig{
		CommitBatchSize: 1,
		Workers:         1,
		BufferSize:      2,
		BatchConfig:     gitlib.DefaultBatchConfig(),
	}
	coord := framework.NewCoordinator(libRepo, config)
	ctx := context.Background()
	out := coord.Process(ctx, commits)

	results := make([]framework.CommitData, 0, 16)
	for d := range out {
		results = append(results, d)
	}

	if len(results) != 1 {
		t.Fatalf("got %d results, want 1", len(results))
	}

	if results[0].Error != nil {
		t.Fatalf("result error: %v", results[0].Error)
	}

	if results[0].Commit == nil || results[0].Index != 0 {
		t.Errorf("Commit=%v Index=%d", results[0].Commit, results[0].Index)
	}

	if results[0].Changes == nil || results[0].BlobCache == nil || results[0].FileDiffs == nil {
		t.Error("Changes, BlobCache, or FileDiffs nil")
	}
}

func TestCoordinator_ProcessTwoCommitsWithModification(t *testing.T) {
	t.Parallel()

	repo := framework.NewTestRepo(t)
	defer repo.Close()

	repo.CreateFile("f.txt", "v1")
	repo.Commit("first")
	repo.CreateFile("f.txt", "v2")
	repo.Commit("second")

	libRepo, err := gitlib.OpenRepository(repo.Path())
	if err != nil {
		t.Fatalf("OpenRepository: %v", err)
	}
	defer libRepo.Free()

	commits := framework.CollectCommits(t, libRepo, 2)
	if len(commits) < 2 {
		t.Fatalf("got %d commits, want at least 2", len(commits))
	}

	commits = commits[:2]

	config := framework.CoordinatorConfig{
		CommitBatchSize: 1,
		Workers:         1,
		BufferSize:      2,
		BatchConfig:     gitlib.DefaultBatchConfig(),
	}
	coord := framework.NewCoordinator(libRepo, config)
	ctx := context.Background()
	out := coord.Process(ctx, commits)

	results := make([]framework.CommitData, 0, 16)
	for d := range out {
		results = append(results, d)
	}

	if len(results) != 2 {
		t.Fatalf("got %d results, want 2", len(results))
	}

	if results[0].Error != nil || results[1].Error != nil {
		t.Errorf("errors: %v, %v", results[0].Error, results[1].Error)
	}

	if len(results[0].Changes) == 0 || len(results[1].Changes) == 0 {
		t.Error("expected non-empty Changes for both commits")
	}

	if len(results[1].FileDiffs) == 0 {
		t.Error("expected FileDiffs on second commit (modified file)")
	}
}

func TestCoordinator_NewCoordinatorNormalizesConfig(t *testing.T) {
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

	config := framework.CoordinatorConfig{
		CommitBatchSize: 0,
		Workers:         0,
		BufferSize:      0,
		BatchConfig:     gitlib.DefaultBatchConfig(),
	}

	coord := framework.NewCoordinator(libRepo, config)
	if coord == nil {
		t.Fatal("NewCoordinator returned nil")
	}

	cfg := coord.Config()
	if cfg.CommitBatchSize != 1 {
		t.Errorf("CommitBatchSize = %d, want 1", cfg.CommitBatchSize)
	}

	if cfg.Workers != 1 {
		t.Errorf("Workers = %d, want 1", cfg.Workers)
	}

	if cfg.BufferSize != 10 {
		t.Errorf("BufferSize = %d, want 10", cfg.BufferSize)
	}
}

func TestCoordinator_ProcessThreeCommits(t *testing.T) {
	t.Parallel()

	repo := framework.NewTestRepo(t)
	defer repo.Close()

	repo.CreateFile("f1.txt", "a")
	repo.Commit("first")
	repo.CreateFile("f2.txt", "b")
	repo.Commit("second")
	repo.CreateFile("f3.txt", "c")
	repo.Commit("third")

	libRepo, err := gitlib.OpenRepository(repo.Path())
	if err != nil {
		t.Fatalf("OpenRepository: %v", err)
	}
	defer libRepo.Free()

	commits := framework.CollectCommits(t, libRepo, 3)
	if len(commits) < 3 {
		t.Fatalf("got %d commits, want at least 3", len(commits))
	}

	commits = commits[:3]

	config := framework.CoordinatorConfig{
		CommitBatchSize: 1,
		Workers:         1,
		BufferSize:      4,
		BatchConfig:     gitlib.DefaultBatchConfig(),
	}
	coord := framework.NewCoordinator(libRepo, config)
	ctx := context.Background()
	out := coord.Process(ctx, commits)

	results := make([]framework.CommitData, 0, 16)
	for d := range out {
		results = append(results, d)
	}

	if len(results) != 3 {
		t.Fatalf("got %d results, want 3", len(results))
	}

	for i, d := range results {
		if d.Error != nil {
			t.Errorf("result %d: %v", i, d.Error)
		}

		if d.Index != i {
			t.Errorf("result %d Index = %d", i, d.Index)
		}
	}
}

func TestCoordinator_ProcessContextCancel(t *testing.T) {
	t.Parallel()

	repo := framework.NewTestRepo(t)
	defer repo.Close()

	repo.CreateFile("a.txt", "a")
	repo.Commit("first")
	repo.CreateFile("b.txt", "b")
	repo.Commit("second")

	libRepo, err := gitlib.OpenRepository(repo.Path())
	if err != nil {
		t.Fatalf("OpenRepository: %v", err)
	}
	defer libRepo.Free()

	commits := framework.CollectCommits(t, libRepo, 2)
	if len(commits) < 2 {
		t.Fatalf("got %d commits, want at least 2", len(commits))
	}

	commits = commits[:2]

	config := framework.CoordinatorConfig{
		CommitBatchSize: 1,
		Workers:         1,
		BufferSize:      2,
		BatchConfig:     gitlib.DefaultBatchConfig(),
	}
	coord := framework.NewCoordinator(libRepo, config)
	ctx, cancel := context.WithCancel(context.Background())
	out := coord.Process(ctx, commits)

	cancel()

	n := 0
	for range out {
		n++
	}

	if n > 2 {
		t.Errorf("after cancel got %d results", n)
	}
}

func TestCoordinator_ProcessSingle(t *testing.T) {
	t.Parallel()

	repo := framework.NewTestRepo(t)
	defer repo.Close()

	repo.CreateFile("x.txt", "x")
	repo.Commit("one")

	libRepo, err := gitlib.OpenRepository(repo.Path())
	if err != nil {
		t.Fatalf("OpenRepository: %v", err)
	}
	defer libRepo.Free()

	commits := framework.CollectCommits(t, libRepo, 1)
	if len(commits) != 1 {
		t.Fatalf("got %d commits, want 1", len(commits))
	}

	config := framework.CoordinatorConfig{
		CommitBatchSize: 1,
		Workers:         1,
		BufferSize:      2,
		BatchConfig:     gitlib.DefaultBatchConfig(),
	}
	coord := framework.NewCoordinator(libRepo, config)
	ctx := context.Background()

	data := coord.ProcessSingle(ctx, commits[0], 0)
	if data.Error != nil {
		t.Fatalf("ProcessSingle: %v", data.Error)
	}

	if data.Index != 0 || data.Commit == nil {
		t.Errorf("Index=%d Commit=%v", data.Index, data.Commit)
	}
}
