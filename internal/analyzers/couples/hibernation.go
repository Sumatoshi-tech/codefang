package couples

import "github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"

// Hibernate compresses the analyzer's state to reduce memory usage.
// Clears ephemeral working state that is chunk-scoped.
func (c *HistoryAnalyzer) Hibernate() error {
	// Reset merge tracker - only used to prevent double-counting
	// within a chunk. Between chunks, we won't see same commits.
	if c.merges != nil {
		c.merges.Reset()
	}

	// Clear lastCommit reference to allow GC.
	c.lastCommit = nil

	return nil
}

// Boot restores the analyzer from hibernated state.
// Re-initializes the merge tracker for the next chunk.
func (c *HistoryAnalyzer) Boot() error {
	if c.merges == nil {
		c.merges = analyze.NewMergeTracker()
	}

	return nil
}

// CleanupSpills is a no-op. The aggregator owns spill cleanup.
func (c *HistoryAnalyzer) CleanupSpills() {}

// workingStateSize is the estimated bytes of working state per commit
// for the couples analyzer (seenFiles set, merges map).
const workingStateSize = 80 * 1024

// avgTCSize is the estimated bytes of TC payload per commit
// for the couples analyzer.
const avgTCSize = 20 * 1024

// WorkingStateSize returns the estimated bytes of working state per commit.
func (c *HistoryAnalyzer) WorkingStateSize() int64 { return workingStateSize }

// AvgTCSize returns the estimated bytes of TC payload per commit.
func (c *HistoryAnalyzer) AvgTCSize() int64 { return avgTCSize }
