package streaming_test

import (
	"bytes"
	"context"
	"fmt"
	"os"
	"path/filepath"
	"slices"
	"testing"
	"time"

	git2go "github.com/libgit2/git2go/v34"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/devs"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/framework"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
	"github.com/Sumatoshi-tech/codefang/pkg/streaming"
)

// TestAcceptance_StreamingMatchesBaseline verifies that running the pipeline
// in streaming mode (multiple chunks with hibernate/boot between them) produces
// identical output to running all commits in a single pass.
//
// This is the acceptance test for Feature 2.1 (Chunk Orchestrator).
func TestAcceptance_StreamingMatchesBaseline(t *testing.T) {
	t.Parallel()

	const (
		numCommits = 500
		chunkSize  = 100
	)

	repoPath := createSyntheticRepo(t, numCommits)

	libRepo, err := gitlib.OpenRepository(repoPath)
	require.NoError(t, err, "OpenRepository")

	defer libRepo.Free()

	commits := collectAllCommits(t, libRepo)
	require.Greater(t, len(commits), numCommits-5, "should have ~%d commits", numCommits)

	// Reverse to oldest-first (collectAllCommits returns newest-first).
	slices.Reverse(commits)

	// Build analyzer pipeline (same as production).
	buildPipeline := func() ([]analyze.HistoryAnalyzer, *devs.HistoryAnalyzer) {
		treeDiff := &plumbing.TreeDiffAnalyzer{Repository: libRepo}
		identity := &plumbing.IdentityDetector{}
		ticks := &plumbing.TicksSinceStart{}
		blobCache := &plumbing.BlobCacheAnalyzer{TreeDiff: treeDiff, Repository: libRepo}
		fileDiff := &plumbing.FileDiffAnalyzer{BlobCache: blobCache, TreeDiff: treeDiff}
		lineStats := &plumbing.LinesStatsCalculator{TreeDiff: treeDiff, BlobCache: blobCache, FileDiff: fileDiff}
		langDetect := &plumbing.LanguagesDetectionAnalyzer{TreeDiff: treeDiff, BlobCache: blobCache}

		leaf := &devs.HistoryAnalyzer{
			Identity: identity, TreeDiff: treeDiff, Ticks: ticks,
			Languages: langDetect, LineStats: lineStats,
		}

		all := []analyze.HistoryAnalyzer{
			treeDiff, identity, ticks, blobCache, fileDiff, lineStats, langDetect, leaf,
		}

		return all, leaf
	}

	// --- Non-streaming baseline ---.
	baselineAnalyzers, baselineLeaf := buildPipeline()
	runner1 := framework.NewRunner(libRepo, repoPath, baselineAnalyzers...)
	baselineResults, err := runner1.Run(context.Background(), commits)
	require.NoError(t, err, "non-streaming Run")
	baselineYAML := serializeReport(t, baselineLeaf, baselineResults[baselineLeaf])
	require.NotEmpty(t, baselineYAML, "baseline report should not be empty")

	// --- Streaming mode ---.
	streamAnalyzers, streamLeaf := buildPipeline()
	runner2 := framework.NewRunner(libRepo, repoPath, streamAnalyzers...)

	require.NoError(t, runner2.Initialize(), "streaming Initialize")

	chunks := planChunks(len(commits), chunkSize)
	require.Greater(t, len(chunks), 1, "should have multiple chunks")
	t.Logf("processing %d commits in %d chunks (chunk size %d)", len(commits), len(chunks), chunkSize)

	for i, chunk := range chunks {
		if i > 0 {
			// Hibernate/boot between chunks, just like production streaming.
			for _, a := range streamAnalyzers {
				if h, ok := a.(streaming.Hibernatable); ok {
					require.NoError(t, h.Hibernate(), "Hibernate chunk %d", i)
				}
			}

			for _, a := range streamAnalyzers {
				if h, ok := a.(streaming.Hibernatable); ok {
					require.NoError(t, h.Boot(), "Boot chunk %d", i)
				}
			}
		}

		_, processErr := runner2.ProcessChunk(context.Background(), commits[chunk.start:chunk.end], chunk.start, i)
		require.NoError(t, processErr, "ProcessChunk %d", i)
	}

	streamResults, err := runner2.Finalize()
	require.NoError(t, err, "streaming Finalize")
	streamYAML := serializeReport(t, streamLeaf, streamResults[streamLeaf])

	// --- Compare ---.
	require.YAMLEq(t, baselineYAML, streamYAML,
		"streaming output must be identical to non-streaming baseline")
}

// chunkBounds is a local chunk boundary type for the acceptance test.
type chunkBounds struct {
	start int
	end   int
}

// planChunks splits totalCommits into chunks of the given size.
func planChunks(totalCommits, chunkSize int) []chunkBounds {
	var chunks []chunkBounds

	for start := 0; start < totalCommits; start += chunkSize {
		end := min(start+chunkSize, totalCommits)
		chunks = append(chunks, chunkBounds{start: start, end: end})
	}

	return chunks
}

// createSyntheticRepo creates a temporary git repo with numCommits commits
// and returns its path. Each commit creates or modifies Go source files.
func createSyntheticRepo(t *testing.T, numCommits int) string {
	t.Helper()

	dir := t.TempDir()

	repo, err := git2go.InitRepository(dir, false)
	require.NoError(t, err, "InitRepository")

	defer repo.Free()

	writeFile := func(name, content string) {
		t.Helper()

		p := filepath.Join(dir, name)

		require.NoError(t, os.MkdirAll(filepath.Dir(p), 0o750))
		require.NoError(t, os.WriteFile(p, []byte(content), 0o600))
	}

	commit := func(message string) {
		t.Helper()

		index, indexErr := repo.Index()
		require.NoError(t, indexErr, "Index")

		defer index.Free()

		require.NoError(t, index.AddAll([]string{"*"}, git2go.IndexAddDefault, nil))
		require.NoError(t, index.Write())

		treeID, writeErr := index.WriteTree()
		require.NoError(t, writeErr, "WriteTree")

		tree, lookupErr := repo.LookupTree(treeID)
		require.NoError(t, lookupErr, "LookupTree")

		defer tree.Free()

		sig := &git2go.Signature{Name: "Test", Email: "test@test.com", When: time.Now()}

		var parents []*git2go.Commit

		head, headErr := repo.Head()
		if headErr == nil {
			headCommit, lookupCommitErr := repo.LookupCommit(head.Target())
			if lookupCommitErr == nil && headCommit != nil {
				parents = append(parents, headCommit)
			}

			head.Free()
		}

		_, createErr := repo.CreateCommit("HEAD", sig, sig, message, tree, parents...)
		require.NoError(t, createErr, "CreateCommit")

		for _, p := range parents {
			p.Free()
		}
	}

	// Initial file set.
	writeFile("main.go", "package main\n\nfunc main() {}\n")
	writeFile("util.go", "package main\n\nfunc helper() string { return \"ok\" }\n")
	commit("initial commit")

	for i := 1; i < numCommits; i++ {
		switch i % 5 {
		case 0:
			name := fmt.Sprintf("pkg/mod%d.go", i)
			content := fmt.Sprintf("package pkg\n\nfunc Func%d() int { return %d }\n", i, i)
			writeFile(name, content)
		case 1:
			content := fmt.Sprintf("package main\n\nimport \"fmt\"\n\nfunc main() { fmt.Println(%d) }\n", i)
			writeFile("main.go", content)
		case 2:
			content := fmt.Sprintf("package main\n\nfunc helper() string { return \"v%d\" }\n", i)
			writeFile("util.go", content)
		case 3:
			content := fmt.Sprintf("package main\n\nfunc helper() string { return \"v%d\" }\n\nfunc extra%d() {}\n", i, i)
			writeFile("util.go", content)
		case 4:
			content := fmt.Sprintf("package main\n\nfunc main() {}\n\nvar Version = %d\n", i)
			writeFile("main.go", content)
		}

		commit(fmt.Sprintf("commit %d", i))
	}

	return dir
}

// collectAllCommits returns all commits (newest-first) from the repo.
func collectAllCommits(t *testing.T, repo *gitlib.Repository) []*gitlib.Commit {
	t.Helper()

	iter, err := repo.Log(&gitlib.LogOptions{})
	require.NoError(t, err, "Log")

	defer iter.Close()

	var commits []*gitlib.Commit

	for {
		c, nextErr := iter.Next()
		if nextErr != nil {
			break
		}

		commits = append(commits, c)
	}

	return commits
}

// serializeReport serializes an analyzer report to YAML for comparison.
func serializeReport(t *testing.T, analyzer analyze.HistoryAnalyzer, report analyze.Report) string {
	t.Helper()

	var buf bytes.Buffer

	err := analyzer.Serialize(report, analyze.FormatYAML, &buf)
	require.NoError(t, err, "Serialize to YAML")

	return buf.String()
}
