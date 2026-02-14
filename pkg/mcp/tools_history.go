//go:build ignore
// +build ignore

package mcp

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path/filepath"

	mcpsdk "github.com/modelcontextprotocol/go-sdk/mcp"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/burndown"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/couples"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/devs"
	filehistory "github.com/Sumatoshi-tech/codefang/pkg/analyzers/file_history"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/imports"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/sentiment"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/shotness"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/typos"
	"github.com/Sumatoshi-tech/codefang/pkg/framework"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

// defaultMCPCommitLimit is the default commit limit for MCP history tool.
const defaultMCPCommitLimit = 1000

// ErrUnknownHistoryAnalyzer indicates the requested history analyzer name is not recognized.
var ErrUnknownHistoryAnalyzer = errors.New("unknown history analyzer")

// handleHistory processes codefang_history tool calls.
func handleHistory(
	ctx context.Context,
	_ *mcpsdk.CallToolRequest,
	input HistoryInput,
) (*mcpsdk.CallToolResult, ToolOutput, error) {
	err := validateHistoryInput(input)
	if err != nil {
		return errorResult(err)
	}

	return executeHistory(ctx, input)
}

// executeHistory runs the full history analysis pipeline.
func executeHistory(ctx context.Context, input HistoryInput) (*mcpsdk.CallToolResult, ToolOutput, error) {
	limit := input.Limit
	if limit <= 0 {
		limit = defaultMCPCommitLimit
	}

	repository, err := gitlib.LoadRepository(input.RepoPath)
	if err != nil {
		return errorResult(fmt.Errorf("load repository: %w", err))
	}
	defer repository.Free()

	pipeline := buildMCPPipeline(repository)

	analyzerKeys := input.Analyzers
	if len(analyzerKeys) == 0 {
		analyzerKeys = allHistoryKeys()
	}

	selectedLeaves, err := selectMCPLeaves(pipeline.leaves, analyzerKeys)
	if err != nil {
		return errorResult(err)
	}

	commits, err := gitlib.LoadCommits(ctx, repository, gitlib.CommitLoadOptions{
		Limit:       limit,
		FirstParent: input.FirstParent,
		Since:       input.Since,
	})
	if err != nil {
		return errorResult(fmt.Errorf("load commits: %w", err))
	}

	results, err := runMCPPipeline(ctx, repository, input.RepoPath, pipeline, selectedLeaves, commits)
	if err != nil {
		return errorResult(err)
	}

	return formatHistoryResults(selectedLeaves, results)
}

// runMCPPipeline configures and executes the analyzer pipeline over commits.
func runMCPPipeline(
	ctx context.Context,
	repository *gitlib.Repository,
	repoPath string,
	pipeline *mcpPipeline,
	selectedLeaves []analyze.HistoryAnalyzer,
	commits []*gitlib.Commit,
) (map[analyze.HistoryAnalyzer]analyze.Report, error) {
	facts := buildMCPFacts(pipeline)

	err := configureMCPAnalyzers(pipeline.core, facts)
	if err != nil {
		return nil, err
	}

	for _, leaf := range selectedLeaves {
		err = leaf.Configure(facts)
		if err != nil {
			return nil, fmt.Errorf("configure %s: %w", leaf.Name(), err)
		}
	}

	allAnalyzers := make([]analyze.HistoryAnalyzer, 0, len(pipeline.core)+len(selectedLeaves))
	allAnalyzers = append(allAnalyzers, pipeline.core...)
	allAnalyzers = append(allAnalyzers, selectedLeaves...)

	runner := framework.NewRunner(repository, repoPath, allAnalyzers...)
	runner.CoreCount = len(pipeline.core)

	err = runner.Initialize(ctx)
	if err != nil {
		return nil, fmt.Errorf("initialize pipeline: %w", err)
	}

	commitData, err := collectCommitData(ctx, repository, commits)
	if err != nil {
		return nil, fmt.Errorf("process commits: %w", err)
	}

	err = runner.ProcessChunkFromData(ctx, commitData, 0)
	if err != nil {
		return nil, fmt.Errorf("pipeline execution: %w", err)
	}

	return runner.Finalize(ctx)
}

// collectCommitData uses a Coordinator to process commits with context support.
func collectCommitData(
	ctx context.Context,
	repository *gitlib.Repository,
	commits []*gitlib.Commit,
) ([]framework.CommitData, error) {
	coordinator := framework.NewCoordinator(repository, framework.DefaultCoordinatorConfig())
	dataChan := coordinator.Process(ctx, commits)

	result := make([]framework.CommitData, 0, len(commits))

	for data := range dataChan {
		if data.Error != nil {
			return nil, data.Error
		}

		result = append(result, data)
	}

	return result, nil
}

// formatHistoryResults encodes the analysis results as JSON tool output.
func formatHistoryResults(
	selectedLeaves []analyze.HistoryAnalyzer,
	results map[analyze.HistoryAnalyzer]analyze.Report,
) (*mcpsdk.CallToolResult, ToolOutput, error) {
	var buf bytes.Buffer

	err := analyze.OutputHistoryResults(selectedLeaves, results, analyze.FormatJSON, &buf)
	if err != nil {
		return errorResult(fmt.Errorf("format results: %w", err))
	}

	var parsed any

	err = json.Unmarshal(buf.Bytes(), &parsed)
	if err != nil {
		return errorResult(fmt.Errorf("decode results: %w", err))
	}

	return jsonResult(parsed)
}

// validateHistoryInput validates the history tool input parameters.
func validateHistoryInput(input HistoryInput) error {
	if input.RepoPath == "" {
		return ErrEmptyRepoPath
	}

	if !filepath.IsAbs(input.RepoPath) {
		return ErrRepoPathNotAbsolute
	}

	info, err := os.Stat(input.RepoPath)
	if err != nil {
		return fmt.Errorf("%w: %s", ErrRepoNotFound, input.RepoPath)
	}

	if !info.IsDir() {
		return fmt.Errorf("%w: %s is not a directory", ErrRepoNotFound, input.RepoPath)
	}

	gitDir := filepath.Join(input.RepoPath, ".git")

	_, err = os.Stat(gitDir)
	if err != nil {
		return fmt.Errorf("%w: %s", ErrNotGitRepo, input.RepoPath)
	}

	return nil
}

// mcpPipeline holds the plumbing core and leaf analyzers for MCP history runs.
type mcpPipeline struct {
	core   []analyze.HistoryAnalyzer
	leaves map[string]analyze.HistoryAnalyzer
}

func buildMCPPipeline(repository *gitlib.Repository) *mcpPipeline {
	treeDiff := &plumbing.TreeDiffAnalyzer{Repository: repository}
	identity := &plumbing.IdentityDetector{}
	ticks := &plumbing.TicksSinceStart{}
	blobCache := &plumbing.BlobCacheAnalyzer{TreeDiff: treeDiff, Repository: repository}
	fileDiff := &plumbing.FileDiffAnalyzer{BlobCache: blobCache, TreeDiff: treeDiff}
	lineStats := &plumbing.LinesStatsCalculator{
		TreeDiff: treeDiff, BlobCache: blobCache, FileDiff: fileDiff,
	}
	langDetect := &plumbing.LanguagesDetectionAnalyzer{TreeDiff: treeDiff, BlobCache: blobCache}
	uastChanges := &plumbing.UASTChangesAnalyzer{TreeDiff: treeDiff, BlobCache: blobCache}

	return &mcpPipeline{
		core: []analyze.HistoryAnalyzer{
			treeDiff, identity, ticks, blobCache, fileDiff, lineStats, langDetect, uastChanges,
		},
		leaves: buildMCPLeaves(treeDiff, identity, ticks, blobCache, fileDiff, lineStats, langDetect, uastChanges),
	}
}

func buildMCPLeaves(
	treeDiff *plumbing.TreeDiffAnalyzer,
	identity *plumbing.IdentityDetector,
	ticks *plumbing.TicksSinceStart,
	blobCache *plumbing.BlobCacheAnalyzer,
	fileDiff *plumbing.FileDiffAnalyzer,
	lineStats *plumbing.LinesStatsCalculator,
	langDetect *plumbing.LanguagesDetectionAnalyzer,
	uastChanges *plumbing.UASTChangesAnalyzer,
) map[string]analyze.HistoryAnalyzer {
	return map[string]analyze.HistoryAnalyzer{
		"burndown": &burndown.HistoryAnalyzer{
			BlobCache: blobCache, Ticks: ticks, Identity: identity,
			FileDiff: fileDiff, TreeDiff: treeDiff,
		},
		"couples": &couples.HistoryAnalyzer{
			Identity: identity, TreeDiff: treeDiff,
		},
		"devs": &devs.HistoryAnalyzer{
			Identity: identity, TreeDiff: treeDiff, Ticks: ticks,
			Languages: langDetect, LineStats: lineStats,
		},
		"file-history": &filehistory.Analyzer{
			Identity: identity, TreeDiff: treeDiff, LineStats: lineStats,
		},
		"imports": &imports.HistoryAnalyzer{
			TreeDiff: treeDiff, BlobCache: blobCache,
			Identity: identity, Ticks: ticks,
		},
		"sentiment": &sentiment.HistoryAnalyzer{
			UAST: uastChanges, Ticks: ticks,
		},
		"shotness": &shotness.HistoryAnalyzer{
			FileDiff: fileDiff, UAST: uastChanges,
		},
		"typos": &typos.HistoryAnalyzer{
			UAST: uastChanges, BlobCache: blobCache, FileDiff: fileDiff,
		},
	}
}

func allHistoryKeys() []string {
	return []string{
		"burndown", "couples", "devs", "file-history",
		"imports", "sentiment", "shotness", "typos",
	}
}

func selectMCPLeaves(
	leaves map[string]analyze.HistoryAnalyzer,
	keys []string,
) ([]analyze.HistoryAnalyzer, error) {
	selected := make([]analyze.HistoryAnalyzer, 0, len(keys))

	for _, name := range keys {
		leaf, found := leaves[name]
		if !found {
			return nil, fmt.Errorf("%w: %s", ErrUnknownHistoryAnalyzer, name)
		}

		selected = append(selected, leaf)
	}

	return selected, nil
}

func buildMCPFacts(pipeline *mcpPipeline) map[string]any {
	facts := map[string]any{}

	allAnalyzers := make([]analyze.HistoryAnalyzer, 0, len(pipeline.core)+len(pipeline.leaves))
	allAnalyzers = append(allAnalyzers, pipeline.core...)

	for _, leaf := range pipeline.leaves {
		allAnalyzers = append(allAnalyzers, leaf)
	}

	for _, analyzer := range allAnalyzers {
		for _, opt := range analyzer.ListConfigurationOptions() {
			if opt.Default != nil {
				facts[opt.Name] = opt.Default
			}
		}
	}

	return facts
}

func configureMCPAnalyzers(analyzers []analyze.HistoryAnalyzer, facts map[string]any) error {
	for _, analyzer := range analyzers {
		err := analyzer.Configure(facts)
		if err != nil {
			return fmt.Errorf("configure %s: %w", analyzer.Name(), err)
		}
	}

	return nil
}
