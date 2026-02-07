package couples

import "github.com/Sumatoshi-tech/codefang/pkg/gitlib"

// Hibernate compresses the analyzer's state to reduce memory usage.
// Clears the merges map since processed merge commits won't be seen again
// during streaming (commits are processed chronologically).
func (c *HistoryAnalyzer) Hibernate() error {
	// Clear merges map - only used to prevent double-counting
	// within a chunk. Between chunks, we won't see same commits.
	c.merges = make(map[gitlib.Hash]bool)

	// Clear lastCommit reference to allow GC
	c.lastCommit = nil

	return nil
}

// Boot restores the analyzer from hibernated state.
// Re-initializes the merges map for the next chunk.
func (c *HistoryAnalyzer) Boot() error {
	// Ensure merges map is ready for new chunk
	if c.merges == nil {
		c.merges = make(map[gitlib.Hash]bool)
	}

	return nil
}
