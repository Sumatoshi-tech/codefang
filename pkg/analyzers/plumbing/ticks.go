package plumbing

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"time"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
	"github.com/Sumatoshi-tech/codefang/pkg/pipeline"
	pkgplumbing "github.com/Sumatoshi-tech/codefang/pkg/plumbing"
)

// TicksSinceStart computes relative time ticks for each commit since the start.
type TicksSinceStart struct {
	tick0        *time.Time
	commits      map[int][]gitlib.Hash
	remote       string
	TickSize     time.Duration
	previousTick int
	Tick         int
}

const (
	// ConfigTicksSinceStartTickSize is the configuration key for the tick size in hours.
	ConfigTicksSinceStartTickSize = "TicksSinceStart.TickSize"
	// DefaultTicksSinceStartTickSize is the default tick size in hours.
	DefaultTicksSinceStartTickSize = 24
)

// Name returns the name of the analyzer.
func (t *TicksSinceStart) Name() string {
	return "TicksSinceStart"
}

// Flag returns the CLI flag for the analyzer.
func (t *TicksSinceStart) Flag() string {
	return "ticks"
}

// Description returns a human-readable description of the analyzer.
func (t *TicksSinceStart) Description() string {
	return t.Descriptor().Description
}

// Descriptor returns stable analyzer metadata.
func (t *TicksSinceStart) Descriptor() analyze.Descriptor {
	return analyze.NewDescriptor(
		analyze.ModeHistory,
		t.Name(),
		"Provides relative tick information for every commit.",
	)
}

// ListConfigurationOptions returns the configuration options for the analyzer.
func (t *TicksSinceStart) ListConfigurationOptions() []pipeline.ConfigurationOption {
	return []pipeline.ConfigurationOption{{
		Name:        ConfigTicksSinceStartTickSize,
		Description: "How long each 'tick' represents in hours.",
		Flag:        "tick-size",
		Type:        pipeline.IntConfigurationOption,
		Default:     DefaultTicksSinceStartTickSize},
	}
}

// Configure sets up the analyzer with the provided facts.
func (t *TicksSinceStart) Configure(facts map[string]any) error {
	if val, exists := facts[ConfigTicksSinceStartTickSize].(int); exists {
		t.TickSize = time.Duration(val) * time.Hour
	} else {
		t.TickSize = DefaultTicksSinceStartTickSize * time.Hour
	}

	if t.commits == nil {
		t.commits = map[int][]gitlib.Hash{}
	}

	facts[pkgplumbing.FactCommitsByTick] = t.commits
	facts[pkgplumbing.FactTickSize] = t.TickSize

	return nil
}

// Initialize prepares the analyzer for processing commits.
func (t *TicksSinceStart) Initialize(_ *gitlib.Repository) error {
	if t.TickSize == 0 {
		t.TickSize = DefaultTicksSinceStartTickSize * time.Hour
	}

	t.tick0 = &time.Time{}

	t.previousTick = 0
	if t.commits == nil || len(t.commits) > 0 {
		t.commits = map[int][]gitlib.Hash{}
	}

	t.remote = "<no remote>" // Simplified.

	return nil
}

// Consume processes a single commit with the provided dependency results.
func (t *TicksSinceStart) Consume(_ context.Context, ac *analyze.Context) (analyze.TC, error) {
	commit := ac.Commit
	index := ac.Index

	if index == 0 {
		tick0 := commit.Committer().When
		*t.tick0 = FloorTime(tick0, t.TickSize)
	}

	tick := max(int(commit.Committer().When.Sub(*t.tick0)/t.TickSize), t.previousTick)

	t.previousTick = tick

	tickCommits := t.commits[tick]
	if tickCommits == nil {
		tickCommits = []gitlib.Hash{}
	}

	exists := false
	commitHash := commit.Hash()

	if commit.NumParents() > 0 {
		for i := range tickCommits {
			if tickCommits[len(tickCommits)-i-1] == commitHash {
				exists = true

				break
			}
		}
	}

	if !exists {
		t.commits[tick] = append(tickCommits, commitHash)
	}

	t.Tick = tick

	return analyze.TC{}, nil
}

// FloorTime rounds a timestamp down to the nearest tick boundary.
func FloorTime(t time.Time, d time.Duration) time.Time {
	result := t.Round(d)
	if result.After(t) {
		result = result.Add(-d)
	}

	return result
}

// Fork creates a copy of the analyzer for parallel processing.
func (t *TicksSinceStart) Fork(n int) []analyze.HistoryAnalyzer {
	res := make([]analyze.HistoryAnalyzer, n)
	for i := range n {
		clone := *t
		res[i] = &clone
	}

	return res
}

// Merge combines results from forked analyzer branches.
func (t *TicksSinceStart) Merge(_ []analyze.HistoryAnalyzer) {
}

// Serialize writes the analysis result to the given writer.
func (t *TicksSinceStart) Serialize(report analyze.Report, format string, writer io.Writer) error {
	if format == analyze.FormatJSON {
		err := json.NewEncoder(writer).Encode(report)
		if err != nil {
			return fmt.Errorf("json encode: %w", err)
		}
	}

	return nil
}

// WorkingStateSize returns 0 — plumbing analyzers are excluded from budget planning.
func (t *TicksSinceStart) WorkingStateSize() int64 { return 0 }

// AvgTCSize returns 0 — plumbing analyzers do not emit meaningful TC payloads.
func (t *TicksSinceStart) AvgTCSize() int64 { return 0 }

// NewAggregator returns nil — plumbing analyzers do not aggregate.
func (t *TicksSinceStart) NewAggregator(_ analyze.AggregatorOptions) analyze.Aggregator { return nil }

// SerializeTICKs returns ErrNotImplemented — plumbing analyzers do not produce TICKs.
func (t *TicksSinceStart) SerializeTICKs(_ []analyze.TICK, _ string, _ io.Writer) error {
	return analyze.ErrNotImplemented
}

// ReportFromTICKs returns ErrNotImplemented — plumbing analyzers do not produce reports.
func (t *TicksSinceStart) ReportFromTICKs(_ context.Context, _ []analyze.TICK) (analyze.Report, error) {
	return nil, analyze.ErrNotImplemented
}

// CurrentTick returns the tick value of the last processed commit.
func (t *TicksSinceStart) CurrentTick() int {
	return t.Tick
}
