package shotness

import (
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/streaming"
)

var _ streaming.Hibernatable = (*Analyzer)(nil)

// Hibernate compresses the analyzer's state to reduce memory usage.
// Resets the merge tracker since processed commits won't be seen again
// during streaming (commits are processed chronologically).
func (s *Analyzer) Hibernate() error {
	if s.merges != nil {
		s.merges.Reset()
	}

	return nil
}

// Boot restores the analyzer from hibernated state.
// Re-initializes the merge tracker for the next chunk.
func (s *Analyzer) Boot() error {
	if s.merges == nil {
		s.merges = analyze.NewMergeTracker()
	}

	return nil
}

// workingStateSize is the estimated bytes of working state per commit
// for the shotness analyzer (nodes + files maps grow with unique functions).
const workingStateSize = 50 * 1024

// avgTCSize is the estimated bytes of TC payload per commit
// for the shotness analyzer (NodeDelta map + coupling pairs).
const avgTCSize = 2 * 1024

// WorkingStateSize returns the estimated bytes of working state per commit.
func (s *Analyzer) WorkingStateSize() int64 { return workingStateSize }

// AvgTCSize returns the estimated bytes of TC payload per commit.
func (s *Analyzer) AvgTCSize() int64 { return avgTCSize }
