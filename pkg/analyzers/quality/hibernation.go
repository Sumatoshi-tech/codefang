package quality

// Hibernate compresses the analyzer's state to reduce memory usage.
// The quality analyzer has no temporary state to clear — all accumulated
// tick quality data must be preserved for final aggregation.
func (h *HistoryAnalyzer) Hibernate() error {
	return nil
}

// Boot restores the analyzer from hibernated state.
func (h *HistoryAnalyzer) Boot() error {
	return nil
}

// stateGrowthPerCommit is the estimated per-commit memory growth in bytes
// for the quality analyzer (TickQuality struct fields per tick).
const stateGrowthPerCommit = 512

// StateGrowthPerCommit returns the estimated per-commit memory growth in bytes.
func (h *HistoryAnalyzer) StateGrowthPerCommit() int64 { return stateGrowthPerCommit }
