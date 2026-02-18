package filehistory

import "github.com/Sumatoshi-tech/codefang/pkg/gitlib"

// Hibernate compresses the analyzer's state to reduce memory usage.
// Clears the merges map since processed merge commits won't be seen again
// during streaming (commits are processed chronologically).
func (h *Analyzer) Hibernate() error {
	// Clear merges map - only used to prevent double-counting
	// within a chunk. Between chunks, we won't see same commits.
	h.merges = make(map[gitlib.Hash]bool)

	// Clear lastCommit reference to allow GC.
	h.lastCommit = nil

	return nil
}

// Boot restores the analyzer from hibernated state.
// Re-initializes the merges map for the next chunk.
func (h *Analyzer) Boot() error {
	// Ensure merges map is ready for new chunk.
	if h.merges == nil {
		h.merges = make(map[gitlib.Hash]bool)
	}

	return nil
}

// stateGrowthPerCommit is the estimated per-commit memory growth in bytes
// for the file-history analyzer (per-file history with developer stats).
const stateGrowthPerCommit = 50 * 1024

// StateGrowthPerCommit returns the estimated per-commit memory growth in bytes.
func (h *Analyzer) StateGrowthPerCommit() int64 { return stateGrowthPerCommit }
