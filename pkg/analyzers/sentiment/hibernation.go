package sentiment

// Hibernate compresses the analyzer's state to reduce memory usage.
// The sentiment analyzer has no temporary state to clear - all accumulated
// comments must be preserved for final aggregation.
func (s *HistoryAnalyzer) Hibernate() error {
	// No-op: sentiment analyzer has no temporary state to clear.
	// commentsByCommit must be preserved for final sentiment analysis.
	return nil
}

// Boot restores the analyzer from hibernated state.
// The sentiment analyzer requires no reinitialization.
func (s *HistoryAnalyzer) Boot() error {
	// No-op: nothing to reinitialize.
	return nil
}

// stateGrowthPerCommit is the estimated per-commit memory growth in bytes
// for the sentiment analyzer (comment strings accumulated per tick).
const stateGrowthPerCommit = 30 * 1024

// StateGrowthPerCommit returns the estimated per-commit memory growth in bytes.
func (s *HistoryAnalyzer) StateGrowthPerCommit() int64 { return stateGrowthPerCommit }
