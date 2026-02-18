package anomaly

// stateGrowthPerCommit is the estimated per-commit memory growth in bytes
// for the anomaly analyzer (file names and tick metrics accumulated per tick).
const stateGrowthPerCommit = 10 * 1024

// Hibernate compresses the analyzer's state to reduce memory usage between
// streaming chunks. The anomaly analyzer must preserve all tick metrics for
// final Z-score computation, so this is a no-op.
func (h *HistoryAnalyzer) Hibernate() error {
	return nil
}

// Boot restores the analyzer from hibernated state.
// The anomaly analyzer requires no reinitialization.
func (h *HistoryAnalyzer) Boot() error {
	if h.commitMetrics == nil {
		h.commitMetrics = make(map[string]*CommitAnomalyData)
	}

	return nil
}

// StateGrowthPerCommit returns the estimated per-commit memory growth in bytes.
func (h *HistoryAnalyzer) StateGrowthPerCommit() int64 { return stateGrowthPerCommit }
