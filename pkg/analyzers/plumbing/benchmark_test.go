package plumbing_test

import (
	"context"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"testing"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

func runCmdWithDir(dir string, args ...string) error {
	cmd := exec.CommandContext(context.Background(), args[0], args[1:]...)
	cmd.Dir = dir

	err := cmd.Run()
	if err != nil {
		return fmt.Errorf("running command %q: %w", args[0], err)
	}

	return nil
}

func runCmd(args ...string) error {
	return runCmdWithDir("", args...)
}

// setupBenchmarkRepo creates a temporary git repo with a large commit.
func setupBenchmarkRepo(tb testing.TB, fileCount int) (*gitlib.Repository, *gitlib.Commit, string) {
	tb.Helper()

	dir := tb.TempDir()

	initCmd := []string{"git", "init", dir}

	err := runCmd(initCmd...)
	if err != nil {
		tb.Fatal(err)
	}

	// Create files.
	for i := range fileCount {
		name := filepath.Join(dir, fmt.Sprintf("file_%d.txt", i))
		// Write some content to force blob creation.
		content := fmt.Sprintf("content for file %d\nline 2\nline 3", i)

		err = os.WriteFile(name, []byte(content), 0o600)
		if err != nil {
			tb.Fatal(err)
		}
	}

	// Add and commit.
	err = runCmdWithDir(dir, "git", "add", ".")
	if err != nil {
		tb.Fatal(err)
	}

	err = runCmdWithDir(dir, "git", "commit", "-m", "Initial commit")
	if err != nil {
		tb.Fatal(err)
	}

	repo, err := gitlib.OpenRepository(dir)
	if err != nil {
		tb.Fatal(err)
	}

	head, err := repo.Head()
	if err != nil {
		tb.Fatal(err)
	}

	commit, err := repo.LookupCommit(context.Background(), head)
	if err != nil {
		tb.Fatal(err)
	}

	return repo, commit, dir
}

func BenchmarkBlobCache_Consume(b *testing.B) {
	// Setup a repo with 1000 files modified in HEAD.
	fileCount := 1000
	repo, commit, repoPath := setupBenchmarkRepo(b, fileCount)

	defer os.RemoveAll(repoPath)
	defer repo.Free()
	defer commit.Free()

	// Construct changes manually by walking the tree.
	tree, err := commit.Tree()
	if err != nil {
		b.Fatal(err)
	}

	changes := make([]*gitlib.Change, 0, fileCount)

	count := tree.EntryCount()
	for i := range count {
		entry := tree.EntryByIndex(i)
		changes = append(changes, &gitlib.Change{
			Action: gitlib.Insert,
			To: gitlib.ChangeEntry{
				Name: entry.Name(),
				Hash: entry.Hash(),
			},
		})
	}

	analyzeCtx := &analyze.Context{Commit: commit}

	// Define scenarios.
	concurrencyLevels := []int{1, 2, 4, 8, 16}

	for _, n := range concurrencyLevels {
		b.Run(fmt.Sprintf("Goroutines-%d", n), func(b *testing.B) {
			bc := &plumbing.BlobCacheAnalyzer{
				Goroutines: n,
			}

			// Initialize opens `n` repos.
			initErr := bc.Initialize(repo)
			if initErr != nil {
				b.Fatal(initErr)
			}

			// We inject the prepared changes.
			bc.TreeDiff = &plumbing.TreeDiffAnalyzer{Changes: changes}

			b.ResetTimer()

			for b.Loop() {
				_, consumeErr := bc.Consume(context.Background(), analyzeCtx)
				if consumeErr != nil {
					b.Fatal(consumeErr)
				}
			}

			// Cleanup extra repos (skip 0 as it is the main repo managed by defer).
			for i := 1; i < len(bc.Repos()); i++ {
				if bc.Repos()[i] != nil {
					bc.Repos()[i].Free()
				}
			}
		})
	}
}
