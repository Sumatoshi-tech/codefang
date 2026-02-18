package couples

import "github.com/Sumatoshi-tech/codefang/pkg/gitlib"

// Hibernate compresses the analyzer's state to reduce memory usage.
// Clears the merges map since processed merge commits won't be seen again
// during streaming (commits are processed chronologically).
func (c *HistoryAnalyzer) Hibernate() error {
	// Clear merges map - only used to prevent double-counting
	// within a chunk. Between chunks, we won't see same commits.
	c.merges = make(map[gitlib.Hash]bool)

	// Clear lastCommit reference to allow GC.
	c.lastCommit = nil

	return nil
}

// Boot restores the analyzer from hibernated state.
// Re-initializes the merges map for the next chunk.
func (c *HistoryAnalyzer) Boot() error {
	// Ensure merges map is ready for new chunk.
	if c.merges == nil {
		c.merges = make(map[gitlib.Hash]bool)
	}

	return nil
}

// stateGrowthPerCommit is the estimated per-commit memory growth in bytes
// for the couples analyzer (file coupling matrix, developer-file maps).
const stateGrowthPerCommit = 100 * 1024

// StateGrowthPerCommit returns the estimated per-commit memory growth in bytes.
func (c *HistoryAnalyzer) StateGrowthPerCommit() int64 { return stateGrowthPerCommit }
