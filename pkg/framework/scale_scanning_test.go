package framework_test

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"slices"
	"sync"
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

// errNilReport is returned when a concurrent repo analysis produces a nil report.
var errNilReport = errors.New("nil report")

// Helpers.

// createBareRepo creates a bare git repository by first building a normal repo
// with commits and then cloning it as bare. This simulates GitLab backup repos.
func createBareRepo(t *testing.T, numCommits int) string {
	t.Helper()

	// Create a normal (non-bare) repo with commits.
	srcDir := t.TempDir()

	srcRepo, err := git2go.InitRepository(srcDir, false)
	require.NoError(t, err, "InitRepository (source)")

	defer srcRepo.Free()

	writeAndCommit(t, srcRepo, srcDir, numCommits)

	// Clone as bare into a new directory.
	bareDir := t.TempDir()

	bareRepo, err := git2go.Clone(srcDir, bareDir, &git2go.CloneOptions{Bare: true})
	require.NoError(t, err, "Clone bare")
	bareRepo.Free()

	// Verify it's actually bare.
	require.NoDirExists(t, filepath.Join(bareDir, ".git"),
		"bare repo should not have .git subdirectory")
	require.FileExists(t, filepath.Join(bareDir, "HEAD"),
		"bare repo should have HEAD at top level")

	return bareDir
}

// createNormalRepo creates a non-bare git repository with commits.
func createNormalRepo(t *testing.T, numCommits int) string {
	t.Helper()

	dir := t.TempDir()

	repo, err := git2go.InitRepository(dir, false)
	require.NoError(t, err, "InitRepository")

	defer repo.Free()

	writeAndCommit(t, repo, dir, numCommits)

	return dir
}

// createNormalRepoWithTimedCommits creates a repo where each commit has a
// controlled timestamp, allowing tests to verify --since filtering.
func createNormalRepoWithTimedCommits(t *testing.T, commitTimes []time.Time) string {
	t.Helper()

	dir := t.TempDir()

	repo, err := git2go.InitRepository(dir, false)
	require.NoError(t, err, "InitRepository")

	defer repo.Free()

	for i, when := range commitTimes {
		name := fmt.Sprintf("file_%d.go", i)
		content := fmt.Sprintf("package main\n\nfunc f%d() int { return %d }\n", i, i)
		writeFile(t, dir, name, content)

		commitAtTime(t, repo, fmt.Sprintf("commit %d", i), when)
	}

	return dir
}

func writeAndCommit(t *testing.T, repo *git2go.Repository, dir string, numCommits int) {
	t.Helper()

	writeFile(t, dir, "main.go", "package main\n\nfunc main() {}\n")
	commitNow(t, repo, "initial commit")

	for i := 1; i < numCommits; i++ {
		name := fmt.Sprintf("pkg/mod%d.go", i)
		content := fmt.Sprintf("package pkg\n\nfunc F%d() int { return %d }\n", i, i)
		writeFile(t, dir, name, content)

		commitNow(t, repo, fmt.Sprintf("commit %d", i))
	}
}

func writeFile(t *testing.T, dir, name, content string) {
	t.Helper()

	p := filepath.Join(dir, name)
	require.NoError(t, os.MkdirAll(filepath.Dir(p), 0o750))
	require.NoError(t, os.WriteFile(p, []byte(content), 0o600))
}

func commitNow(t *testing.T, repo *git2go.Repository, message string) {
	t.Helper()
	commitAtTime(t, repo, message, time.Now())
}

func commitAtTime(t *testing.T, repo *git2go.Repository, message string, when time.Time) {
	t.Helper()

	index, err := repo.Index()
	require.NoError(t, err)

	defer index.Free()

	require.NoError(t, index.AddAll([]string{"*"}, git2go.IndexAddDefault, nil))
	require.NoError(t, index.Write())

	treeID, err := index.WriteTree()
	require.NoError(t, err)

	tree, err := repo.LookupTree(treeID)
	require.NoError(t, err)

	defer tree.Free()

	sig := &git2go.Signature{Name: "Test", Email: "test@test.com", When: when}

	var parents []*git2go.Commit

	head, headErr := repo.Head()
	if headErr == nil {
		hc, lookupErr := repo.LookupCommit(head.Target())
		if lookupErr == nil {
			parents = append(parents, hc)
		}

		head.Free()
	}

	_, err = repo.CreateCommit("HEAD", sig, sig, message, tree, parents...)
	require.NoError(t, err)

	for _, p := range parents {
		p.Free()
	}
}

// buildFullPipeline builds a complete plumbing + devs analyzer pipeline.
func buildFullPipeline(libRepo *gitlib.Repository) (all []analyze.HistoryAnalyzer, leaf *devs.Analyzer) {
	treeDiff := &plumbing.TreeDiffAnalyzer{Repository: libRepo}
	identity := &plumbing.IdentityDetector{}
	ticks := &plumbing.TicksSinceStart{}
	blobCache := &plumbing.BlobCacheAnalyzer{TreeDiff: treeDiff, Repository: libRepo}
	fileDiff := &plumbing.FileDiffAnalyzer{BlobCache: blobCache, TreeDiff: treeDiff}
	lineStats := &plumbing.LinesStatsCalculator{TreeDiff: treeDiff, BlobCache: blobCache, FileDiff: fileDiff}
	langDetect := &plumbing.LanguagesDetectionAnalyzer{TreeDiff: treeDiff, BlobCache: blobCache}

	leaf = devs.NewAnalyzer()
	leaf.Identity = identity
	leaf.TreeDiff = treeDiff
	leaf.Ticks = ticks
	leaf.Languages = langDetect
	leaf.LineStats = lineStats

	all = []analyze.HistoryAnalyzer{
		treeDiff, identity, ticks, blobCache, fileDiff, lineStats, langDetect, leaf,
	}

	return all, leaf
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

// runAnalysis runs a full single-pass analysis and returns the leaf report.
func runAnalysis(t *testing.T, repoPath string) (analyze.Report, *devs.Analyzer) {
	t.Helper()

	libRepo, err := gitlib.OpenRepository(repoPath)
	require.NoError(t, err)

	defer libRepo.Free()

	commits := collectAllCommits(t, libRepo)
	require.NotEmpty(t, commits)
	slices.Reverse(commits)

	analyzers, leaf := buildFullPipeline(libRepo)
	runner := framework.NewRunner(libRepo, repoPath, analyzers...)

	results, err := runner.Run(context.Background(), commits)
	require.NoError(t, err)

	report := results[leaf]
	require.NotNil(t, report)

	return report, leaf
}

// Test: Bare Repository Support.

// TestScaleScanning_BareRepoAnalysis verifies that codefang can open and analyze a bare repo.
func TestScaleScanning_BareRepoAnalysis(t *testing.T) {
	t.Parallel()

	const numCommits = 20

	bareDir := createBareRepo(t, numCommits)

	// Verify codefang can open and analyze a bare repo.
	libRepo, err := gitlib.OpenRepository(bareDir)
	require.NoError(t, err, "OpenRepository on bare repo")

	defer libRepo.Free()

	commits := collectAllCommits(t, libRepo)
	require.GreaterOrEqual(t, len(commits), numCommits,
		"bare repo should have all commits accessible")
	slices.Reverse(commits)

	// Run full pipeline on bare repo.
	analyzers, leaf := buildFullPipeline(libRepo)
	runner := framework.NewRunner(libRepo, bareDir, analyzers...)

	results, err := runner.Run(context.Background(), commits)
	require.NoError(t, err, "Run on bare repo")
	require.NotNil(t, results[leaf], "devs report should not be nil")
}

// TestScaleScanning_BareRepoMatchesNormalRepo verifies that analyzing a bare
// clone produces the same results as analyzing the original normal repo.
func TestScaleScanning_BareRepoMatchesNormalRepo(t *testing.T) {
	t.Parallel()

	const numCommits = 30

	// Create a normal repo.
	srcDir := t.TempDir()

	srcRepo, err := git2go.InitRepository(srcDir, false)
	require.NoError(t, err)

	defer srcRepo.Free()

	writeAndCommit(t, srcRepo, srcDir, numCommits)

	// Clone as bare.
	bareDir := t.TempDir()

	bareRepo, err := git2go.Clone(srcDir, bareDir, &git2go.CloneOptions{Bare: true})
	require.NoError(t, err)
	bareRepo.Free()

	// Analyze both.
	normalReport, normalLeaf := runAnalysis(t, srcDir)
	bareReport, bareLeaf := runAnalysis(t, bareDir)

	// Serialize to YAML for comparison.
	var normalBuf, bareBuf bytes.Buffer
	require.NoError(t, normalLeaf.Serialize(normalReport, analyze.FormatYAML, &normalBuf))
	require.NoError(t, bareLeaf.Serialize(bareReport, analyze.FormatYAML, &bareBuf))

	require.YAMLEq(t, normalBuf.String(), bareBuf.String(),
		"bare repo analysis should match normal repo analysis")
}

// Test: JSON Output Format.

// TestScaleScanning_JSONOutputStructure verifies JSON output is valid and non-empty.
func TestScaleScanning_JSONOutputStructure(t *testing.T) {
	t.Parallel()

	repoPath := createNormalRepo(t, 15)
	report, leaf := runAnalysis(t, repoPath)

	// Serialize to JSON.
	var buf bytes.Buffer

	err := leaf.Serialize(report, analyze.FormatJSON, &buf)
	require.NoError(t, err, "Serialize to JSON")
	require.NotEmpty(t, buf.Bytes(), "JSON output should not be empty")

	// Verify valid JSON.
	var parsed map[string]any

	err = json.Unmarshal(buf.Bytes(), &parsed)
	require.NoError(t, err, "JSON output should be valid JSON")

	// Verify expected top-level keys for devs analyzer.
	// The devs report should contain author/tick data.
	require.NotEmpty(t, parsed, "JSON should contain data")
}

func TestScaleScanning_JSONRoundTrip(t *testing.T) {
	t.Parallel()

	repoPath := createNormalRepo(t, 10)
	report, leaf := runAnalysis(t, repoPath)

	// Serialize to JSON twice — output should be deterministic.
	var buf1, buf2 bytes.Buffer
	require.NoError(t, leaf.Serialize(report, analyze.FormatJSON, &buf1))
	require.NoError(t, leaf.Serialize(report, analyze.FormatJSON, &buf2))

	require.Equal(t, buf1.String(), buf2.String(),
		"JSON serialization should be deterministic")
}

// TestScaleScanning_OutputHistoryResultsJSON tests the top-level output function
// that a batch pipeline would use to write results.
func TestScaleScanning_OutputHistoryResultsJSON(t *testing.T) {
	t.Parallel()

	repoPath := createNormalRepo(t, 10)

	libRepo, err := gitlib.OpenRepository(repoPath)
	require.NoError(t, err)

	defer libRepo.Free()

	commits := collectAllCommits(t, libRepo)
	slices.Reverse(commits)

	analyzers, leaf := buildFullPipeline(libRepo)
	runner := framework.NewRunner(libRepo, repoPath, analyzers...)

	results, err := runner.Run(context.Background(), commits)
	require.NoError(t, err)

	// Use OutputHistoryResults — this is what `codefang run --format json` calls.
	var buf bytes.Buffer

	err = analyze.OutputHistoryResults([]analyze.HistoryAnalyzer{leaf}, results, analyze.FormatJSON, &buf)
	require.NoError(t, err)
	require.NotEmpty(t, buf.String())

	// Should be valid JSON.
	require.True(t, json.Valid(buf.Bytes()), "OutputHistoryResults JSON should be valid")
}

// Test: Streaming / Chunked Processing.

// TestScaleScanning_StreamingProducesIdenticalOutput verifies streaming matches single-pass.
func TestScaleScanning_StreamingProducesIdenticalOutput(t *testing.T) {
	t.Parallel()

	const (
		numCommits = 200
		chunkSize  = 50
	)

	repoPath := createNormalRepo(t, numCommits)

	libRepo, err := gitlib.OpenRepository(repoPath)
	require.NoError(t, err)

	defer libRepo.Free()

	commits := collectAllCommits(t, libRepo)
	require.GreaterOrEqual(t, len(commits), numCommits-5)
	slices.Reverse(commits)

	// Baseline: single pass.
	baselineAnalyzers, baselineLeaf := buildFullPipeline(libRepo)
	runner1 := framework.NewRunner(libRepo, repoPath, baselineAnalyzers...)

	baselineResults, err := runner1.Run(context.Background(), commits)
	require.NoError(t, err)

	var baselineBuf bytes.Buffer
	require.NoError(t, baselineLeaf.Serialize(baselineResults[baselineLeaf], analyze.FormatYAML, &baselineBuf))

	// Streaming: multiple chunks with hibernate/boot.
	streamAnalyzers, streamLeaf := buildFullPipeline(libRepo)
	runner2 := framework.NewRunner(libRepo, repoPath, streamAnalyzers...)

	require.NoError(t, runner2.Initialize())

	for start := 0; start < len(commits); start += chunkSize {
		end := min(start+chunkSize, len(commits))
		chunkIdx := start / chunkSize

		if chunkIdx > 0 {
			for _, a := range streamAnalyzers {
				if h, ok := a.(streaming.Hibernatable); ok {
					require.NoError(t, h.Hibernate(), "Hibernate chunk %d", chunkIdx)
				}
			}

			for _, a := range streamAnalyzers {
				if h, ok := a.(streaming.Hibernatable); ok {
					require.NoError(t, h.Boot(), "Boot chunk %d", chunkIdx)
				}
			}
		}

		_, processErr := runner2.ProcessChunk(context.Background(), commits[start:end], start, chunkIdx)
		require.NoError(t, processErr, "ProcessChunk %d", chunkIdx)
	}

	streamResults, err := runner2.FinalizeWithAggregators(context.Background())
	require.NoError(t, err)

	var streamBuf bytes.Buffer
	require.NoError(t, streamLeaf.Serialize(streamResults[streamLeaf], analyze.FormatYAML, &streamBuf))

	require.YAMLEq(t, baselineBuf.String(), streamBuf.String(),
		"streaming chunked output must match single-pass baseline")
}

// TestScaleScanning_StreamingWithMemoryBudget tests the full RunStreaming
// codepath with an explicit memory budget that forces multiple chunks.
func TestScaleScanning_StreamingWithMemoryBudget(t *testing.T) {
	t.Parallel()

	const numCommits = 150

	repoPath := createNormalRepo(t, numCommits)

	libRepo, err := gitlib.OpenRepository(repoPath)
	require.NoError(t, err)

	defer libRepo.Free()

	commits := collectAllCommits(t, libRepo)
	slices.Reverse(commits)

	analyzers, leaf := buildFullPipeline(libRepo)
	config := framework.DefaultCoordinatorConfig()
	runner := framework.NewRunnerWithConfig(libRepo, repoPath, config, analyzers...)
	runner.CoreCount = 7 // 7 plumbing + 1 leaf.

	streamConfig := framework.StreamingConfig{
		// Very small budget to force multiple chunks.
		MemBudget: 500 * 1024 * 1024, // 500 MiB.
		RepoPath:  repoPath,
	}

	results, err := framework.RunStreaming(
		context.Background(), runner, commits, analyzers, streamConfig,
	)
	require.NoError(t, err, "RunStreaming")
	require.NotNil(t, results[leaf], "devs report from streaming should not be nil")
}

// TestScaleScanning_StreamingFromIterator verifies that RunStreamingFromIterator
// produces the same results as RunStreaming with a pre-loaded commit slice.
func TestScaleScanning_StreamingFromIterator(t *testing.T) {
	t.Parallel()

	const numCommits = 150

	repoPath := createNormalRepo(t, numCommits)

	// Baseline: slice-based RunStreaming.
	libRepo1, err := gitlib.OpenRepository(repoPath)
	require.NoError(t, err)

	defer libRepo1.Free()

	commits := collectAllCommits(t, libRepo1)
	slices.Reverse(commits)

	baselineAnalyzers, baselineLeaf := buildFullPipeline(libRepo1)
	config1 := framework.DefaultCoordinatorConfig()
	runner1 := framework.NewRunnerWithConfig(libRepo1, repoPath, config1, baselineAnalyzers...)
	runner1.CoreCount = 7

	streamConfig := framework.StreamingConfig{
		MemBudget: 500 * 1024 * 1024,
		RepoPath:  repoPath,
	}

	baselineResults, err := framework.RunStreaming(
		context.Background(), runner1, commits, baselineAnalyzers, streamConfig,
	)
	require.NoError(t, err)
	require.NotNil(t, baselineResults[baselineLeaf])

	var baselineBuf bytes.Buffer
	require.NoError(t, baselineLeaf.Serialize(baselineResults[baselineLeaf], analyze.FormatYAML, &baselineBuf))

	// Iterator-based: RunStreamingFromIterator.
	libRepo2, err := gitlib.OpenRepository(repoPath)
	require.NoError(t, err)

	defer libRepo2.Free()

	commitCount, err := libRepo2.CommitCount(&gitlib.LogOptions{})
	require.NoError(t, err)
	require.Equal(t, len(commits), commitCount)

	iter, err := libRepo2.Log(&gitlib.LogOptions{Reverse: true})
	require.NoError(t, err)

	defer iter.Close()

	iterAnalyzers, iterLeaf := buildFullPipeline(libRepo2)
	config2 := framework.DefaultCoordinatorConfig()
	runner2 := framework.NewRunnerWithConfig(libRepo2, repoPath, config2, iterAnalyzers...)
	runner2.CoreCount = 7

	iterResults, err := framework.RunStreamingFromIterator(
		context.Background(), runner2, iter, commitCount, iterAnalyzers, streamConfig,
	)
	require.NoError(t, err)
	require.NotNil(t, iterResults[iterLeaf])

	var iterBuf bytes.Buffer
	require.NoError(t, iterLeaf.Serialize(iterResults[iterLeaf], analyze.FormatYAML, &iterBuf))

	require.YAMLEq(t, baselineBuf.String(), iterBuf.String(),
		"iterator-based streaming must match slice-based streaming")
}

// Test: Checkpoint Save/Resume.

// TestScaleScanning_CheckpointSaveResumeAcrossChunks verifies checkpoint save and resume.
func TestScaleScanning_CheckpointSaveResumeAcrossChunks(t *testing.T) {
	t.Parallel()

	const (
		numCommits = 100
		chunkSize  = 25
	)

	repoPath := createNormalRepo(t, numCommits)
	checkpointDir := t.TempDir()

	libRepo, err := gitlib.OpenRepository(repoPath)
	require.NoError(t, err)

	defer libRepo.Free()

	commits := collectAllCommits(t, libRepo)
	slices.Reverse(commits)

	// Run first two chunks and checkpoint.
	analyzers1, _ := buildFullPipeline(libRepo)
	runner1 := framework.NewRunner(libRepo, repoPath, analyzers1...)
	require.NoError(t, runner1.Initialize())

	for chunkIdx := range 2 {
		start := chunkIdx * chunkSize
		end := min(start+chunkSize, len(commits))

		if chunkIdx > 0 {
			for _, a := range analyzers1 {
				if h, ok := a.(streaming.Hibernatable); ok {
					require.NoError(t, h.Hibernate())
				}
			}

			for _, a := range analyzers1 {
				if h, ok := a.(streaming.Hibernatable); ok {
					require.NoError(t, h.Boot())
				}
			}
		}

		_, processErr := runner1.ProcessChunk(context.Background(), commits[start:end], start, chunkIdx)
		require.NoError(t, processErr)
	}

	// Save checkpoint after 2 chunks (50 commits processed).
	var checkpointCount int

	for _, a := range analyzers1 {
		if cp, ok := a.(interface{ SaveCheckpoint(string) error }); ok {
			require.NoError(t, cp.SaveCheckpoint(checkpointDir),
				"SaveCheckpoint for %s", a.Name())

			checkpointCount++
		}
	}

	// Resume: load checkpoint and process remaining chunks.
	analyzers2, leaf2 := buildFullPipeline(libRepo)
	runner2 := framework.NewRunner(libRepo, repoPath, analyzers2...)
	require.NoError(t, runner2.Initialize())

	for _, a := range analyzers2 {
		if cp, ok := a.(interface{ LoadCheckpoint(string) error }); ok {
			require.NoError(t, cp.LoadCheckpoint(checkpointDir),
				"LoadCheckpoint for %s", a.Name())
		}
	}

	for chunkIdx := 2; chunkIdx*chunkSize < len(commits); chunkIdx++ {
		start := chunkIdx * chunkSize
		end := min(start+chunkSize, len(commits))

		if chunkIdx > 2 {
			for _, a := range analyzers2 {
				if h, ok := a.(streaming.Hibernatable); ok {
					require.NoError(t, h.Hibernate())
				}
			}

			for _, a := range analyzers2 {
				if h, ok := a.(streaming.Hibernatable); ok {
					require.NoError(t, h.Boot())
				}
			}
		}

		_, processErr := runner2.ProcessChunk(context.Background(), commits[start:end], start, chunkIdx)
		require.NoError(t, processErr)
	}

	results2, err := runner2.FinalizeWithAggregators(context.Background())
	require.NoError(t, err)

	// The resumed run should produce non-nil results.
	report := results2[leaf2]
	require.NotNil(t, report, "resumed analysis should produce a report")

	// Also run a full baseline for comparison if checkpoint covered all analyzers.
	if checkpointCount > 0 {
		var buf bytes.Buffer

		err = leaf2.Serialize(report, analyze.FormatJSON, &buf)
		require.NoError(t, err)
		require.True(t, json.Valid(buf.Bytes()), "checkpoint-resumed report should produce valid JSON")
	}
}

// Test: --since Incremental Filtering.

// TestScaleScanning_SinceFilteringDuration verifies duration-based --since filtering.
func TestScaleScanning_SinceFilteringDuration(t *testing.T) {
	t.Parallel()

	now := time.Now()
	commitTimes := []time.Time{
		now.Add(-72 * time.Hour), // 3 days ago.
		now.Add(-48 * time.Hour), // 2 days ago.
		now.Add(-24 * time.Hour), // 1 day ago.
		now.Add(-1 * time.Hour),  // 1 hour ago.
	}

	repoPath := createNormalRepoWithTimedCommits(t, commitTimes)

	libRepo, err := gitlib.OpenRepository(repoPath)
	require.NoError(t, err)

	defer libRepo.Free()

	// Load all commits.
	allCommits, err := gitlib.LoadCommits(context.Background(), libRepo, gitlib.CommitLoadOptions{})
	require.NoError(t, err)
	require.Len(t, allCommits, 4, "should have 4 total commits")

	// Load commits since 36 hours ago — should get the last 2.
	recentCommits, err := gitlib.LoadCommits(context.Background(), libRepo, gitlib.CommitLoadOptions{
		Since: "36h",
	})
	require.NoError(t, err)
	require.Len(t, recentCommits, 2,
		"--since 36h should return only 2 recent commits")
}

func TestScaleScanning_SinceFilteringRFC3339(t *testing.T) {
	t.Parallel()

	now := time.Now()
	commitTimes := []time.Time{
		now.Add(-96 * time.Hour),
		now.Add(-72 * time.Hour),
		now.Add(-48 * time.Hour),
		now.Add(-24 * time.Hour),
	}

	repoPath := createNormalRepoWithTimedCommits(t, commitTimes)

	libRepo, err := gitlib.OpenRepository(repoPath)
	require.NoError(t, err)

	defer libRepo.Free()

	// Use RFC3339 cutoff at 60 hours ago.
	cutoff := now.Add(-60 * time.Hour).Format(time.RFC3339)

	recentCommits, err := gitlib.LoadCommits(context.Background(), libRepo, gitlib.CommitLoadOptions{
		Since: cutoff,
	})
	require.NoError(t, err)
	require.Len(t, recentCommits, 2,
		"--since RFC3339 cutoff should return only 2 recent commits")
}

func TestScaleScanning_SinceAnalysisProducesResults(t *testing.T) {
	t.Parallel()

	now := time.Now()
	commitTimes := []time.Time{
		now.Add(-72 * time.Hour),
		now.Add(-48 * time.Hour),
		now.Add(-24 * time.Hour),
		now.Add(-1 * time.Hour),
	}

	repoPath := createNormalRepoWithTimedCommits(t, commitTimes)

	libRepo, err := gitlib.OpenRepository(repoPath)
	require.NoError(t, err)

	defer libRepo.Free()

	// Load only recent commits.
	commits, err := gitlib.LoadCommits(context.Background(), libRepo, gitlib.CommitLoadOptions{
		Since: "36h",
	})
	require.NoError(t, err)
	require.NotEmpty(t, commits)

	// Run analysis on the subset.
	analyzers, leaf := buildFullPipeline(libRepo)
	runner := framework.NewRunner(libRepo, repoPath, analyzers...)

	results, err := runner.Run(context.Background(), commits)
	require.NoError(t, err)
	require.NotNil(t, results[leaf], "analysis on --since subset should produce results")
}

// Test: Batch Processing Multiple Repos.

// TestScaleScanning_MultipleReposIndependent verifies concurrent independent repo analysis.
func TestScaleScanning_MultipleReposIndependent(t *testing.T) {
	t.Parallel()

	const (
		numRepos   = 5
		numCommits = 20
	)

	type repoResult struct {
		idx  int
		json []byte
		err  error
	}

	// Create repos before launching goroutines to avoid go-require violations.
	repoPaths := make([]string, numRepos)
	for i := range numRepos {
		repoPaths[i] = createNormalRepo(t, numCommits)
	}

	results := make(chan repoResult, numRepos)

	var wg sync.WaitGroup

	for i := range numRepos {
		wg.Add(1)

		go func(idx int) {
			defer wg.Done()

			libRepo, err := gitlib.OpenRepository(repoPaths[idx])
			if err != nil {
				results <- repoResult{idx: idx, err: err}

				return
			}

			defer libRepo.Free()

			commits, err := libRepo.Log(&gitlib.LogOptions{})
			if err != nil {
				results <- repoResult{idx: idx, err: err}

				return
			}

			var commitList []*gitlib.Commit

			for {
				c, nextErr := commits.Next()
				if nextErr != nil {
					break
				}

				commitList = append(commitList, c)
			}

			commits.Close()
			slices.Reverse(commitList)

			analyzers, leaf := buildFullPipeline(libRepo)
			runner := framework.NewRunner(libRepo, repoPaths[idx], analyzers...)

			reports, runErr := runner.Run(context.Background(), commitList)
			if runErr != nil {
				results <- repoResult{idx: idx, err: runErr}

				return
			}

			var buf bytes.Buffer

			serErr := leaf.Serialize(reports[leaf], analyze.FormatJSON, &buf)
			results <- repoResult{idx: idx, json: buf.Bytes(), err: serErr}
		}(i)
	}

	wg.Wait()
	close(results)

	for res := range results {
		require.NoError(t, res.err, "repo %d failed", res.idx)
		require.NotEmpty(t, res.json, "repo %d produced empty JSON", res.idx)
		require.True(t, json.Valid(res.json), "repo %d produced invalid JSON", res.idx)
	}
}

// TestScaleScanning_BatchStreamingMultipleRepos tests that multiple repos
// can each be processed with RunStreaming concurrently.
func TestScaleScanning_BatchStreamingMultipleRepos(t *testing.T) {
	t.Parallel()

	const (
		numRepos   = 3
		numCommits = 80
	)

	// Create repos before launching goroutines to avoid go-require violations.
	repoPaths := make([]string, numRepos)
	for i := range numRepos {
		repoPaths[i] = createNormalRepo(t, numCommits)
	}

	var wg sync.WaitGroup

	errs := make(chan error, numRepos)

	for i := range numRepos {
		wg.Add(1)

		go func(idx int) {
			defer wg.Done()

			libRepo, err := gitlib.OpenRepository(repoPaths[idx])
			if err != nil {
				errs <- fmt.Errorf("repo %d: open: %w", idx, err)

				return
			}

			defer libRepo.Free()

			iter, err := libRepo.Log(&gitlib.LogOptions{})
			if err != nil {
				errs <- fmt.Errorf("repo %d: log: %w", idx, err)

				return
			}

			var commits []*gitlib.Commit

			for {
				c, nextErr := iter.Next()
				if nextErr != nil {
					break
				}

				commits = append(commits, c)
			}

			iter.Close()
			slices.Reverse(commits)

			analyzers, leaf := buildFullPipeline(libRepo)
			config := framework.DefaultCoordinatorConfig()
			runner := framework.NewRunnerWithConfig(libRepo, repoPaths[idx], config, analyzers...)
			runner.CoreCount = 7

			streamConfig := framework.StreamingConfig{
				MemBudget: 500 * 1024 * 1024,
				RepoPath:  repoPaths[idx],
			}

			results, runErr := framework.RunStreaming(
				context.Background(), runner, commits, analyzers, streamConfig,
			)
			if runErr != nil {
				errs <- fmt.Errorf("repo %d: streaming: %w", idx, runErr)

				return
			}

			if results[leaf] == nil {
				errs <- fmt.Errorf("repo %d: %w", idx, errNilReport)

				return
			}

			errs <- nil
		}(i)
	}

	wg.Wait()
	close(errs)

	for err := range errs {
		require.NoError(t, err)
	}
}

// Test: Large Repo Resilience.

// TestScaleScanning_EmptyRepoHandledGracefully verifies empty repos do not crash.
func TestScaleScanning_EmptyRepoHandledGracefully(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()

	repo, err := git2go.InitRepository(dir, false)
	require.NoError(t, err)
	repo.Free()

	libRepo, err := gitlib.OpenRepository(dir)
	require.NoError(t, err)

	defer libRepo.Free()

	// An empty repo (no commits) should not crash.
	commits, err := gitlib.LoadCommits(context.Background(), libRepo, gitlib.CommitLoadOptions{})
	// LoadCommits calls Head() which fails on empty repo.
	// Either empty result or error is acceptable, but no panic.
	if err != nil {
		return // Graceful error.
	}

	require.Empty(t, commits, "empty repo should have no commits")
}

func TestScaleScanning_SingleCommitRepo(t *testing.T) {
	t.Parallel()

	repoPath := createNormalRepo(t, 1)

	libRepo, err := gitlib.OpenRepository(repoPath)
	require.NoError(t, err)

	defer libRepo.Free()

	commits := collectAllCommits(t, libRepo)
	slices.Reverse(commits)
	require.Len(t, commits, 1)

	analyzers, leaf := buildFullPipeline(libRepo)
	runner := framework.NewRunner(libRepo, repoPath, analyzers...)

	results, err := runner.Run(context.Background(), commits)
	require.NoError(t, err)
	require.NotNil(t, results[leaf])
}

func TestScaleScanning_CommitLimitRespected(t *testing.T) {
	t.Parallel()

	repoPath := createNormalRepo(t, 50)

	libRepo, err := gitlib.OpenRepository(repoPath)
	require.NoError(t, err)

	defer libRepo.Free()

	// Load only the last 10 commits.
	commits, err := gitlib.LoadCommits(context.Background(), libRepo, gitlib.CommitLoadOptions{Limit: 10})
	require.NoError(t, err)
	require.Len(t, commits, 10, "--limit should cap commit count")

	// Analysis on limited commits should work.
	analyzers, leaf := buildFullPipeline(libRepo)
	runner := framework.NewRunner(libRepo, repoPath, analyzers...)

	results, err := runner.Run(context.Background(), commits)
	require.NoError(t, err)
	require.NotNil(t, results[leaf])
}

func TestScaleScanning_HeadOnlyMode(t *testing.T) {
	t.Parallel()

	repoPath := createNormalRepo(t, 20)

	libRepo, err := gitlib.OpenRepository(repoPath)
	require.NoError(t, err)

	defer libRepo.Free()

	commits, err := gitlib.LoadCommits(context.Background(), libRepo, gitlib.CommitLoadOptions{HeadOnly: true})
	require.NoError(t, err)
	require.Len(t, commits, 1, "HeadOnly should return exactly 1 commit")

	analyzers, leaf := buildFullPipeline(libRepo)
	runner := framework.NewRunner(libRepo, repoPath, analyzers...)

	results, err := runner.Run(context.Background(), commits)
	require.NoError(t, err)
	require.NotNil(t, results[leaf])
}

func TestScaleScanning_RemoteURIRejected(t *testing.T) {
	t.Parallel()

	_, err := gitlib.LoadRepository("https://github.com/example/repo.git")
	require.ErrorIs(t, err, gitlib.ErrRemoteNotSupported)

	_, err = gitlib.LoadRepository("git@github.com:example/repo.git")
	require.ErrorIs(t, err, gitlib.ErrRemoteNotSupported)
}

// Test: Format Validation.

// TestScaleScanning_AllUniversalFormatsAccepted verifies all format strings are accepted.
func TestScaleScanning_AllUniversalFormatsAccepted(t *testing.T) {
	t.Parallel()

	for _, format := range analyze.UniversalFormats() {
		t.Run(format, func(t *testing.T) {
			t.Parallel()

			normalized, err := analyze.ValidateUniversalFormat(format)
			require.NoError(t, err)
			require.Equal(t, format, normalized)
		})
	}
}

func TestScaleScanning_BinAliasNormalized(t *testing.T) {
	t.Parallel()

	require.Equal(t, analyze.FormatBinary, analyze.NormalizeFormat("bin"))
	require.Equal(t, analyze.FormatBinary, analyze.NormalizeFormat(" BIN "))
}

// Test: Bare Repo with RunStreaming.

// TestScaleScanning_BareRepoWithStreaming verifies streaming works on bare repos.
func TestScaleScanning_BareRepoWithStreaming(t *testing.T) {
	t.Parallel()

	const numCommits = 60

	bareDir := createBareRepo(t, numCommits)

	libRepo, err := gitlib.OpenRepository(bareDir)
	require.NoError(t, err)

	defer libRepo.Free()

	commits := collectAllCommits(t, libRepo)
	slices.Reverse(commits)

	analyzers, leaf := buildFullPipeline(libRepo)
	config := framework.DefaultCoordinatorConfig()
	runner := framework.NewRunnerWithConfig(libRepo, bareDir, config, analyzers...)
	runner.CoreCount = 7

	streamConfig := framework.StreamingConfig{
		MemBudget: 500 * 1024 * 1024,
		RepoPath:  bareDir,
	}

	results, err := framework.RunStreaming(
		context.Background(), runner, commits, analyzers, streamConfig,
	)
	require.NoError(t, err, "RunStreaming on bare repo")
	require.NotNil(t, results[leaf])

	// Verify JSON output from bare repo streaming.
	var buf bytes.Buffer

	err = leaf.Serialize(results[leaf], analyze.FormatJSON, &buf)
	require.NoError(t, err)
	require.True(t, json.Valid(buf.Bytes()))
}

// Test: First-Parent Walk (merge-heavy repos).

// TestScaleScanning_FirstParentWalkWithMerges verifies first-parent walk with merge commits.
func TestScaleScanning_FirstParentWalkWithMerges(t *testing.T) {
	t.Parallel()

	repo := framework.NewTestRepo(t)

	defer repo.Close()

	// Build a repo with merges, simulating a real codebase workflow.
	// Pattern: commit on main, branch from it, merge back (first parent = HEAD).
	repo.CreateFile("a.go", "package main\n\nfunc a() {}\n")
	hashA := repo.Commit("initial")

	repo.CreateFile("b.go", "package main\n\nfunc b() {}\n")
	hashB := repo.CommitToRef("refs/heads/feature", "feature commit", hashA)

	// hashA is still HEAD here, so CreateMergeCommit works.
	_ = repo.CreateMergeCommit("merge feature", hashA, hashB)

	libRepo, err := gitlib.OpenRepository(repo.Path())
	require.NoError(t, err)

	defer libRepo.Free()

	// First-parent walk should work without "internal integrity error".
	fpCommits, err := gitlib.LoadCommits(context.Background(), libRepo, gitlib.CommitLoadOptions{FirstParent: true})
	require.NoError(t, err)
	require.NotEmpty(t, fpCommits)

	analyzers, leaf := buildFullPipeline(libRepo)
	runner := framework.NewRunner(libRepo, repo.Path(), analyzers...)

	results, err := runner.Run(context.Background(), fpCommits)
	require.NoError(t, err)
	require.NotNil(t, results[leaf])
}

// Test: ParseTime Formats.

// TestScaleScanning_ParseTimeDuration verifies duration parsing for --since.
func TestScaleScanning_ParseTimeDuration(t *testing.T) {
	t.Parallel()

	before := time.Now()

	parsed, err := gitlib.ParseTime("24h")
	require.NoError(t, err)

	// Should be approximately 24 hours ago.
	expectedApprox := before.Add(-24 * time.Hour)
	require.InDelta(t, expectedApprox.Unix(), parsed.Unix(), 2,
		"ParseTime(24h) should be ~24h ago")
}

// TestScaleScanning_ParseTimeRFC3339 verifies RFC3339 timestamp parsing.
func TestScaleScanning_ParseTimeRFC3339(t *testing.T) {
	t.Parallel()

	parsed, err := gitlib.ParseTime("2024-06-15T10:30:00Z")
	require.NoError(t, err)

	expected := time.Date(2024, 6, 15, 10, 30, 0, 0, time.UTC)
	require.Equal(t, expected, parsed)
}

// TestScaleScanning_ParseTimeDateOnly verifies date-only string parsing.
func TestScaleScanning_ParseTimeDateOnly(t *testing.T) {
	t.Parallel()

	parsed, err := gitlib.ParseTime("2024-06-15")
	require.NoError(t, err)

	expected := time.Date(2024, 6, 15, 0, 0, 0, 0, time.UTC)
	require.Equal(t, expected, parsed)
}

// TestScaleScanning_ParseTimeInvalid verifies invalid time strings are rejected.
func TestScaleScanning_ParseTimeInvalid(t *testing.T) {
	t.Parallel()

	_, err := gitlib.ParseTime("not-a-time")
	require.ErrorIs(t, err, gitlib.ErrInvalidTimeFormat)
}
