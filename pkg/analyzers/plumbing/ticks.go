package plumbing

import (
	"io"
	"time"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/pipeline"
	pkgplumbing "github.com/Sumatoshi-tech/codefang/pkg/plumbing"
	"github.com/go-git/go-git/v6"
	gitplumbing "github.com/go-git/go-git/v6/plumbing"
)

type TicksSinceStart struct {
	// Configuration
	TickSize time.Duration

	// State
	remote       string
	tick0        *time.Time
	previousTick int
	commits      map[int][]gitplumbing.Hash
	
	// Output
	Tick int

	// Internal
	l interface {
		Warnf(format string, args ...interface{})
	}
}

const (
	ConfigTicksSinceStartTickSize = "TicksSinceStart.TickSize"
	DefaultTicksSinceStartTickSize = 24
)

func (t *TicksSinceStart) Name() string {
	return "TicksSinceStart"
}

func (t *TicksSinceStart) Flag() string {
	return "ticks"
}

func (t *TicksSinceStart) Description() string {
	return "Provides relative tick information for every commit."
}

func (t *TicksSinceStart) ListConfigurationOptions() []pipeline.ConfigurationOption {
	return []pipeline.ConfigurationOption{{
		Name:        ConfigTicksSinceStartTickSize,
		Description: "How long each 'tick' represents in hours.",
		Flag:        "tick-size",
		Type:        pipeline.IntConfigurationOption,
		Default:     DefaultTicksSinceStartTickSize},
	}
}

func (t *TicksSinceStart) Configure(facts map[string]interface{}) error {
	if val, exists := facts[ConfigTicksSinceStartTickSize].(int); exists {
		t.TickSize = time.Duration(val) * time.Hour
	} else {
		t.TickSize = DefaultTicksSinceStartTickSize * time.Hour
	}
	if t.commits == nil {
		t.commits = map[int][]gitplumbing.Hash{}
	}
	facts[pkgplumbing.FactCommitsByTick] = t.commits
	facts[pkgplumbing.FactTickSize] = t.TickSize
	return nil
}

func (t *TicksSinceStart) Initialize(repository *git.Repository) error {
	if t.TickSize == 0 {
		t.TickSize = DefaultTicksSinceStartTickSize * time.Hour
	}
	t.tick0 = &time.Time{}
	t.previousTick = 0
	if t.commits == nil || len(t.commits) > 0 {
		t.commits = map[int][]gitplumbing.Hash{}
	}
	t.remote = "<no remote>" // Simplified
	return nil
}

func (t *TicksSinceStart) Consume(ctx *analyze.Context) error {
	commit := ctx.Commit
	index := ctx.Index
	
	if index == 0 {
		tick0 := commit.Committer.When
		*t.tick0 = FloorTime(tick0, t.TickSize)
	}

	tick := int(commit.Committer.When.Sub(*t.tick0) / t.TickSize)
	if tick < t.previousTick {
		tick = t.previousTick
	}

	t.previousTick = tick
	tickCommits := t.commits[tick]
	if tickCommits == nil {
		tickCommits = []gitplumbing.Hash{}
	}

	exists := false
	if commit.NumParents() > 0 {
		for i := range tickCommits {
			if tickCommits[len(tickCommits)-i-1] == commit.Hash {
				exists = true
				break
			}
		}
	}
	if !exists {
		t.commits[tick] = append(tickCommits, commit.Hash)
	}
	
	t.Tick = tick
	return nil
}

func FloorTime(t time.Time, d time.Duration) time.Time {
	result := t.Round(d)
	if result.After(t) {
		result = result.Add(-d)
	}
	return result
}

func (t *TicksSinceStart) Finalize() (analyze.Report, error) {
	return nil, nil
}

func (t *TicksSinceStart) Fork(n int) []analyze.HistoryAnalyzer {
	res := make([]analyze.HistoryAnalyzer, n)
	for i := 0; i < n; i++ {
		clone := *t
		res[i] = &clone
	}
	return res
}

func (t *TicksSinceStart) Merge(branches []analyze.HistoryAnalyzer) {
}

func (t *TicksSinceStart) Serialize(result analyze.Report, binary bool, writer io.Writer) error {
	return nil
}
