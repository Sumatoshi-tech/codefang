package plumbing

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"testing"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

func runCmdWithDir(dir string, args ...string) error {
	cmd := exec.Command(args[0], args[1:]...)
	cmd.Dir = dir
	return cmd.Run()
}

func runCmd(args ...string) error {
	return runCmdWithDir("", args...)
}

// setupBenchmarkRepo creates a temporary git repo with a large commit.
func setupBenchmarkRepo(t testing.TB, fileCount int) (*gitlib.Repository, *gitlib.Commit, string) {
	dir, err := os.MkdirTemp("", "codefang-bench-*")
	if err != nil {
		t.Fatal(err)
	}

	initCmd := []string{"git", "init", dir}
	if err := runCmd(initCmd...); err != nil {
		t.Fatal(err)
	}

	// Create files
	for i := 0; i < fileCount; i++ {
		name := filepath.Join(dir, fmt.Sprintf("file_%d.txt", i))
		// Write some content to force blob creation
		content := fmt.Sprintf("content for file %d\nline 2\nline 3", i)
		if err := os.WriteFile(name, []byte(content), 0644); err != nil {
			t.Fatal(err)
		}
	}

	// Add and commit
	if err := runCmdWithDir(dir, "git", "add", "."); err != nil {
		t.Fatal(err)
	}
	if err := runCmdWithDir(dir, "git", "commit", "-m", "Initial commit"); err != nil {
		t.Fatal(err)
	}

	repo, err := gitlib.OpenRepository(dir)
	if err != nil {
		t.Fatal(err)
	}

	head, err := repo.Head()
	if err != nil {
		t.Fatal(err)
	}

	commit, err := repo.LookupCommit(head)
	if err != nil {
		t.Fatal(err)
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

	// Construct changes manually by walking the tree
	tree, err := commit.Tree()
	if err != nil {
		b.Fatal(err)
	}
	
	changes := make([]*gitlib.Change, 0, fileCount)
	
	count := uint64(tree.EntryCount())
	for i := uint64(0); i < count; i++ {
		entry := tree.EntryByIndex(i)
		changes = append(changes, &gitlib.Change{
			Action: gitlib.Insert,
			To: gitlib.ChangeEntry{
				Name: entry.Name(),
				Hash: entry.Hash(),
			},
		})
	}

	ctx := &analyze.Context{Commit: commit}

	// Define scenarios
	concurrencyLevels := []int{1, 2, 4, 8, 16}

	for _, n := range concurrencyLevels {
		b.Run(fmt.Sprintf("Goroutines-%d", n), func(b *testing.B) {
			bc := &BlobCacheAnalyzer{
				Goroutines: n,
			}

			// Initialize opens `n` repos.
			if err := bc.Initialize(repo); err != nil {
				b.Fatal(err)
			}
			
			// We inject the prepared changes.
			bc.TreeDiff = &TreeDiffAnalyzer{Changes: changes}

			b.ResetTimer()
			for i := 0; i < b.N; i++ {
				if err := bc.Consume(ctx); err != nil {
					b.Fatal(err)
				}
			}
			
			// Cleanup extra repos (skip 0 as it is the main repo managed by defer)
			for i := 1; i < len(bc.repos); i++ {
				if bc.repos[i] != nil {
					bc.repos[i].Free()
				}
			}
		})
	}
}
