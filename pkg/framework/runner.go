package framework

import (
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/go-git/go-git/v6"
	"github.com/go-git/go-git/v6/plumbing/object"
)

type Runner struct {
	Analyzers []analyze.HistoryAnalyzer
	Repo      *git.Repository
}

func NewRunner(repo *git.Repository, analyzers ...analyze.HistoryAnalyzer) *Runner {
	return &Runner{
		Analyzers: analyzers,
		Repo:      repo,
	}
}

func (r *Runner) Run(commits []*object.Commit) (map[analyze.HistoryAnalyzer]analyze.Report, error) {
	// 1. Initialize
	for _, a := range r.Analyzers {
		if err := a.Initialize(r.Repo); err != nil {
			return nil, err
		}
	}

	// 2. Iterate commits
	for i, commit := range commits {
		ctx := &analyze.Context{
			Commit:  commit,
			Index:   i,
			Time:    commit.Committer.When,
			IsMerge: commit.NumParents() > 1,
		}

		for _, a := range r.Analyzers {
			if err := a.Consume(ctx); err != nil {
				return nil, err
			}
		}
	}

	// 3. Finalize
	reports := make(map[analyze.HistoryAnalyzer]analyze.Report)
	for _, a := range r.Analyzers {
		rep, err := a.Finalize()
		if err != nil {
			return nil, err
		}
		reports[a] = rep
	}
	return reports, nil
}
