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

	// Clear cumulative node/files maps to prevent unbounded growth across
	// chunks. Per-commit data is captured in TCs; the aggregator accumulates
	// counts and coupling independently. Within the next chunk, addNode
	// re-populates s.nodes from UAST processing.
	s.DiscardState()

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
const workingStateSize = 2 * 1024

// avgTCSize is the estimated bytes of TC payload per commit
// for the shotness analyzer (NodeDelta map + coupling pairs).
const avgTCSize = 2 * 1024
