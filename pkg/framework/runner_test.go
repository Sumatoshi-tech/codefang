package framework_test

import (
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
