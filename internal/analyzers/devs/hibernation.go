package devs

import (
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/streaming"
)

var _ streaming.Hibernatable = (*Analyzer)(nil)

// Hibernate compresses the analyzer's state to reduce memory usage.
// Resets the merge tracker since processed commits won't be seen again
// during streaming (commits are processed chronologically).
func (a *Analyzer) Hibernate() error {
	if a.merges != nil {
		a.merges.Reset()
	}

	return nil
}

// Boot restores the analyzer from hibernated state.
// Re-initializes the merge tracker for the next chunk.
func (a *Analyzer) Boot() error {
	if a.merges == nil {
		a.merges = analyze.NewMergeTracker()
	}

	return nil
}

// workingStateSize is the estimated bytes of working state per commit
// for the devs analyzer (merge tracker, ReversedPeopleDict).
const workingStateSize = 4 * 1024

// avgTCSize is the estimated bytes of TC payload per commit
// for the devs analyzer (per-language stats).
const avgTCSize = 500

// WorkingStateSize returns the estimated bytes of working state per commit.
func (a *Analyzer) WorkingStateSize() int64 { return workingStateSize }

// AvgTCSize returns the estimated bytes of TC payload per commit.
func (a *Analyzer) AvgTCSize() int64 { return avgTCSize }
