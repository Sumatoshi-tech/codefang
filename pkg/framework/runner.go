// Package framework provides orchestration for running analysis pipelines.
package framework

import (
	"context"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

// Runner orchestrates multiple HistoryAnalyzers over a commit sequence.
// It always uses the Coordinator pipeline (batch blob load + batch diff in C).
type Runner struct {
	Repo      *gitlib.Repository
	RepoPath  string
	Analyzers []analyze.HistoryAnalyzer
	Config    CoordinatorConfig
}

// NewRunner creates a new Runner for the given repository and analyzers.
// Uses DefaultCoordinatorConfig(). Use NewRunnerWithConfig for custom configuration.
func NewRunner(repo *gitlib.Repository, repoPath string, analyzers ...analyze.HistoryAnalyzer) *Runner {
	return NewRunnerWithConfig(repo, repoPath, DefaultCoordinatorConfig(), analyzers...)
}

// NewRunnerWithConfig creates a new Runner with custom coordinator configuration.
func NewRunnerWithConfig(
	repo *gitlib.Repository,
	repoPath string,
	config CoordinatorConfig,
	analyzers ...analyze.HistoryAnalyzer,
) *Runner {
	return &Runner{
		Repo:      repo,
		RepoPath:  repoPath,
		Analyzers: analyzers,
		Config:    config,
	}
}

// Run executes all analyzers over the given commits: initialize, consume each commit via pipeline, then finalize.
func (runner *Runner) Run(commits []*gitlib.Commit) (map[analyze.HistoryAnalyzer]analyze.Report, error) {
	for _, a := range runner.Analyzers {
		err := a.Initialize(runner.Repo)
		if err != nil {
			return nil, err
		}
	}

	if len(commits) == 0 {
		reports := make(map[analyze.HistoryAnalyzer]analyze.Report)

		for _, a := range runner.Analyzers {
			rep, err := a.Finalize()
			if err != nil {
				return nil, err
			}

			reports[a] = rep
		}

		return reports, nil
	}

	return runner.runInternal(commits)
}

// runInternal uses the Coordinator to process commits (batch blob + batch diff in C), then feeds analyzers.
func (runner *Runner) runInternal(commits []*gitlib.Commit) (map[analyze.HistoryAnalyzer]analyze.Report, error) {
	processErr := runner.processCommits(commits, 0)
	if processErr != nil {
		return nil, processErr
	}

	reports := make(map[analyze.HistoryAnalyzer]analyze.Report)

	for _, a := range runner.Analyzers {
		rep, finalizeErr := a.Finalize()
		if finalizeErr != nil {
			return nil, finalizeErr
		}

		reports[a] = rep
	}

	return reports, nil
}

// Initialize initializes all analyzers. Call once before processing chunks.
func (runner *Runner) Initialize() error {
	for _, a := range runner.Analyzers {
		err := a.Initialize(runner.Repo)
		if err != nil {
			return err
		}
	}

	return nil
}

// ProcessChunk processes a chunk of commits without Initialize/Finalize.
// Use this for streaming mode where Initialize is called once at start
// and Finalize once at end.
// The indexOffset is added to the commit index to maintain correct ordering across chunks.
func (runner *Runner) ProcessChunk(commits []*gitlib.Commit, indexOffset int) error {
	return runner.processCommits(commits, indexOffset)
}

// Finalize finalizes all analyzers and returns their reports.
func (runner *Runner) Finalize() (map[analyze.HistoryAnalyzer]analyze.Report, error) {
	reports := make(map[analyze.HistoryAnalyzer]analyze.Report)

	for _, a := range runner.Analyzers {
		rep, err := a.Finalize()
		if err != nil {
			return nil, err
		}

		reports[a] = rep
	}

	return reports, nil
}

// processCommits processes commits through the pipeline without Initialize/Finalize.
func (runner *Runner) processCommits(commits []*gitlib.Commit, indexOffset int) error {
	ctx := context.Background()
	coordinator := NewCoordinator(runner.Repo, runner.Config)
	dataChan := coordinator.Process(ctx, commits)

	for data := range dataChan {
		if data.Error != nil {
			return data.Error
		}

		commit := data.Commit
		analyzeCtx := &analyze.Context{
			Commit:    commit,
			Index:     data.Index + indexOffset,
			Time:      commit.Committer().When,
			IsMerge:   commit.NumParents() > 1,
			Changes:   data.Changes,
			BlobCache: data.BlobCache,
			FileDiffs: data.FileDiffs,
		}

		for _, a := range runner.Analyzers {
			err := a.Consume(analyzeCtx)
			if err != nil {
				return err
			}
		}
	}

	return nil
}
