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
}

// NewRunner creates a new Runner for the given repository and analyzers.
func NewRunner(repo *gitlib.Repository, repoPath string, analyzers ...analyze.HistoryAnalyzer) *Runner {
	return &Runner{
		Repo:      repo,
		RepoPath:  repoPath,
		Analyzers: analyzers,
	}
}

// Run executes all analyzers over the given commits: initialize, consume each commit via pipeline, then finalize.
func (runner *Runner) Run(commits []*gitlib.Commit) (map[analyze.HistoryAnalyzer]analyze.Report, error) {
	for _, a := range runner.Analyzers {
		if err := a.Initialize(runner.Repo); err != nil {
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

	return runner.run(commits)
}

// run uses the Coordinator to process commits (batch blob + batch diff in C), then feeds analyzers.
func (runner *Runner) run(commits []*gitlib.Commit) (map[analyze.HistoryAnalyzer]analyze.Report, error) {
	ctx := context.Background()
	coordinator := NewCoordinator(runner.Repo, DefaultCoordinatorConfig())
	dataChan := coordinator.Process(ctx, commits)

	for data := range dataChan {
		if data.Error != nil {
			return nil, data.Error
		}

		commit := data.Commit
		analyzeCtx := &analyze.Context{
			Commit:    commit,
			Index:     data.Index,
			Time:      commit.Committer().When,
			IsMerge:   commit.NumParents() > 1,
			Changes:   data.Changes,
			BlobCache: data.BlobCache,
			FileDiffs: data.FileDiffs,
		}

		for _, a := range runner.Analyzers {
			if err := a.Consume(analyzeCtx); err != nil {
				return nil, err
			}
		}
	}

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
