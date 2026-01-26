// Package framework provides the analysis runner that orchestrates commit history analysis.
package framework

import (
	"github.com/go-git/go-git/v6"
	"github.com/go-git/go-git/v6/plumbing/object"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
)

// Runner orchestrates multiple HistoryAnalyzers over a commit sequence.
type Runner struct {
	Repo      *git.Repository
	Analyzers []analyze.HistoryAnalyzer
}

// NewRunner creates a new Runner for the given repository and analyzers.
func NewRunner(repo *git.Repository, analyzers ...analyze.HistoryAnalyzer) *Runner {
	return &Runner{
		Repo:      repo,
		Analyzers: analyzers,
	}
}

// Run executes all analyzers over the given commits: initialize, consume each commit, then finalize.
func (runner *Runner) Run(commits []*object.Commit) (map[analyze.HistoryAnalyzer]analyze.Report, error) {
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
			Time:    commit.Committer.When,
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
