package devs

import "github.com/Sumatoshi-tech/codefang/pkg/gitlib"

// Hibernate compresses the analyzer's state to reduce memory usage.
// Clears the merges map since processed merge commits won't be seen again
// during streaming (commits are processed chronologically).
func (d *HistoryAnalyzer) Hibernate() error {
	// Clear merges map - these are only used to prevent double-counting
	// merge commits within a chunk. Between chunks, we won't see
	// the same commits again, so we can safely release this memory.
	d.merges = make(map[gitlib.Hash]bool)

	return nil
}

// Boot restores the analyzer from hibernated state.
// Re-initializes the merges map for the next chunk.
func (d *HistoryAnalyzer) Boot() error {
	// Ensure merges map is ready for new chunk.
	if d.merges == nil {
		d.merges = make(map[gitlib.Hash]bool)
	}

	return nil
}
