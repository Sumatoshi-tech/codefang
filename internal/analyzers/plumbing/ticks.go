package plumbing

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"time"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	pkgplumbing "github.com/Sumatoshi-tech/codefang/internal/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
	"github.com/Sumatoshi-tech/codefang/pkg/pipeline"
)

// TicksSinceStart computes relative time ticks for each commit since the start.
type TicksSinceStart struct {
	tick0         *time.Time
	commits       map[int][]gitlib.Hash
	remote        string
	TickSize      time.Duration
	previousTick  int
	Tick          int
	lastValidWhen time.Time           // Most recent in-window committer timestamp; substitution source.
	tick0Set      bool                // tick0 has been seeded by an in-window commit.
	anomalies     *timeAnomalyTracker // Shared across Fork() clones so aggregated counts survive forking.
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
	t.tick0Set = false
	t.lastValidWhen = time.Time{}

	if t.anomalies == nil {
		t.anomalies = &timeAnomalyTracker{}
	}

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
	when := t.sanitizeWhen(commit.Committer().When)

	if !t.tick0Set {
		*t.tick0 = FloorTime(when, t.TickSize)
		t.tick0Set = true
	}

	tick := max(int(when.Sub(*t.tick0)/t.TickSize), t.previousTick)

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

// sanitizeWhen clamps a committer timestamp into the sane analysis window
// [minSaneCommitTime, [time.Now]()+maxClockSkew]. Out-of-window values are
// substituted with the most recent in-window timestamp seen, falling back
// to minSaneCommitTime on the first commit. Each substitution is counted
// and surfaced via TimeAnomalies(); the warning log is rate-limited.
//
// In-window inputs pass through unchanged and update lastValidWhen so
// future anomalies have a fresh substitution source.
func (t *TicksSinceStart) sanitizeWhen(when time.Time) time.Time {
	upperBound := time.Now().Add(maxClockSkew)

	switch {
	case when.Before(minSaneCommitTime):
		replacement := t.substituteWhen()
		t.anomalies.recordBeforeMin(when, replacement)

		return replacement
	case when.After(upperBound):
		replacement := t.substituteWhen()
		t.anomalies.recordAfterMax(when, replacement)

		return replacement
	}

	t.lastValidWhen = when

	return when
}

// substituteWhen picks a stand-in for an out-of-window committer time:
// the most recent in-window value if we have one, otherwise the
// minSaneCommitTime floor (so the bad commit collapses to tick 0 instead
// of inflating the analysis period).
func (t *TicksSinceStart) substituteWhen() time.Time {
	if t.lastValidWhen.IsZero() {
		return minSaneCommitTime
	}

	return t.lastValidWhen
}

// TimeAnomalies returns the cumulative count of committer-timestamp
// anomalies clamped during this analyzer's run. See [TimeAnomalyStats]
// for the operational meaning.
func (t *TicksSinceStart) TimeAnomalies() TimeAnomalyStats {
	if t.anomalies == nil {
		return TimeAnomalyStats{}
	}

	return t.anomalies.snapshot()
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
