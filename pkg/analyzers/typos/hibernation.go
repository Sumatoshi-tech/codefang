package typos

import "github.com/Sumatoshi-tech/codefang/pkg/levenshtein"

// Hibernate compresses the analyzer's state to reduce memory usage.
// Clears the levenshtein context buffers between streaming chunks.
// The typos slice is preserved for final aggregation.
func (t *HistoryAnalyzer) Hibernate() error {
	t.lcontext = nil

	return nil
}

// Boot restores the analyzer from hibernated state.
// Recreates the levenshtein context with fresh buffers.
func (t *HistoryAnalyzer) Boot() error {
	if t.lcontext == nil {
		t.lcontext = &levenshtein.Context{}
	}

	return nil
}

// stateGrowthPerCommit is the estimated per-commit memory growth in bytes
// for the typos analyzer (accumulated typo structs).
const stateGrowthPerCommit = 20 * 1024

// StateGrowthPerCommit returns the estimated per-commit memory growth in bytes.
func (t *HistoryAnalyzer) StateGrowthPerCommit() int64 { return stateGrowthPerCommit }
