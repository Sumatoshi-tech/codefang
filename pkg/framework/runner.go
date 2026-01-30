// Package framework provides orchestration for running analysis pipelines.
package framework

import (
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

// Runner orchestrates multiple HistoryAnalyzers over a commit sequence.
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

// Run executes all analyzers over the given commits: initialize, consume each commit, then finalize.
func (runner *Runner) Run(commits []*gitlib.Commit) (map[analyze.HistoryAnalyzer]analyze.Report, error) {
	// 1. Initialize.
	for _, a := range runner.Analyzers {
		err := a.Initialize(runner.Repo)
		if err != nil {
			return nil, err
		}
	}

	// 2. Iterate commits.
	for i, commit := range commits {
		ctx := &analyze.Context{
			Commit:  commit,
			Index:   i,
			Time:    commit.Committer().When,
			IsMerge: commit.NumParents() > 1,
		}

		for _, a := range runner.Analyzers {
			err := a.Consume(ctx)
			if err != nil {
				return nil, err
			}
		}
	}

	// 3. Finalize.
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
