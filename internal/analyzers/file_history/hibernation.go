package filehistory

import "github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"

// Hibernate compresses the analyzer's state to reduce memory usage.
// Clears the merge tracker since processed merge commits won't be seen again
// during streaming (commits are processed chronologically).
func (h *HistoryAnalyzer) Hibernate() error {
	// Reset merge tracker - only used to prevent double-counting
	// within a chunk. Between chunks, we won't see same commits.
	h.merges.Reset()

	// Clear file history map to prevent unbounded growth across chunks.
	// The aggregator (SpillStore[FileHistory]) independently tracks file
	// history from TCs, so h.files is redundant between chunks.
	// Within the next chunk, processFileChanges re-populates h.files.
	h.files = make(map[string]*FileHistory)

	return nil
}

// Boot restores the analyzer from hibernated state.
// Re-initializes the merge tracker for the next chunk.
func (h *HistoryAnalyzer) Boot() error {
	// Ensure merge tracker is ready for new chunk.
	if h.merges == nil {
		h.merges = analyze.NewMergeTracker()
	}

	return nil
}

// workingStateSize is the estimated bytes of working state per commit
// for the file-history analyzer (per-file history with developer stats).
const workingStateSize = 2 * 1024

// avgTCSize is the estimated bytes of TC payload per commit
// for the file-history analyzer.
const avgTCSize = 10 * 1024

// WorkingStateSize returns the estimated bytes of working state per commit.
func (h *HistoryAnalyzer) WorkingStateSize() int64 { return workingStateSize }

// AvgTCSize returns the estimated bytes of TC payload per commit.
func (h *HistoryAnalyzer) AvgTCSize() int64 { return avgTCSize }
