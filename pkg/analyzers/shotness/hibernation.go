package shotness

import "github.com/Sumatoshi-tech/codefang/pkg/gitlib"

// Hibernate compresses the analyzer's state to reduce memory usage.
// Clears the merges map since processed merge commits won't be seen again
// during streaming (commits are processed chronologically).
func (s *HistoryAnalyzer) Hibernate() error {
	// Clear merges map - only used to prevent double-counting
	// within a chunk. Between chunks, we won't see same commits.
	s.merges = make(map[gitlib.Hash]bool)

	return nil
}

// Boot restores the analyzer from hibernated state.
// Re-initializes the merges map for the next chunk.
func (s *HistoryAnalyzer) Boot() error {
	// Ensure merges map is ready for new chunk
	if s.merges == nil {
		s.merges = make(map[gitlib.Hash]bool)
	}

	return nil
}
