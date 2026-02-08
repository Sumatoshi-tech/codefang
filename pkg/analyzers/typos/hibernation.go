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
