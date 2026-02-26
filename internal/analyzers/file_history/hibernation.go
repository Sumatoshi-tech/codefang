package filehistory

import "github.com/Sumatoshi-tech/codefang/pkg/gitlib"

// Hibernate compresses the analyzer's state to reduce memory usage.
// Clears the merges map since processed merge commits won't be seen again
// during streaming (commits are processed chronologically).
func (h *HistoryAnalyzer) Hibernate() error {
	// Clear merges map - only used to prevent double-counting
	// within a chunk. Between chunks, we won't see same commits.
	h.merges = make(map[gitlib.Hash]bool)

	// lastCommit is no longer a pointer reference so no need to clear it.
	return nil
}

// Boot restores the analyzer from hibernated state.
// Re-initializes the merges map for the next chunk.
func (h *HistoryAnalyzer) Boot() error {
	// Ensure merges map is ready for new chunk.
	if h.merges == nil {
		h.merges = make(map[gitlib.Hash]bool)
	}

	return nil
}

// workingStateSize is the estimated bytes of working state per commit
// for the file-history analyzer (per-file history with developer stats).
const workingStateSize = 40 * 1024

// avgTCSize is the estimated bytes of TC payload per commit
// for the file-history analyzer.
const avgTCSize = 10 * 1024

// WorkingStateSize returns the estimated bytes of working state per commit.
func (h *HistoryAnalyzer) WorkingStateSize() int64 { return workingStateSize }

// AvgTCSize returns the estimated bytes of TC payload per commit.
func (h *HistoryAnalyzer) AvgTCSize() int64 { return avgTCSize }
