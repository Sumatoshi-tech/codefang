package imports

import "github.com/Sumatoshi-tech/codefang/internal/streaming"

var _ streaming.Hibernatable = (*HistoryAnalyzer)(nil)

// Hibernate is a no-op. The imports history analyzer has no cumulative working
// state; all output is emitted per-commit as TCs and the UAST parser is shared.
func (h *HistoryAnalyzer) Hibernate() error { return nil }

// Boot is a no-op.
func (h *HistoryAnalyzer) Boot() error { return nil }

// workingStateSize is 0 — the imports analyzer accumulates no working state.
const workingStateSize = 0

// avgTCSize is the estimated bytes of TC payload per commit.
const avgTCSize = 1024

// WorkingStateSize returns the estimated bytes of working state per commit.
func (h *HistoryAnalyzer) WorkingStateSize() int64 { return workingStateSize }

// AvgTCSize returns the estimated bytes of TC payload per commit.
func (h *HistoryAnalyzer) AvgTCSize() int64 { return avgTCSize }
