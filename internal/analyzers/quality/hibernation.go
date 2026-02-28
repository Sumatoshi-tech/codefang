package quality

import "github.com/Sumatoshi-tech/codefang/internal/streaming"

var _ streaming.Hibernatable = (*Analyzer)(nil)

// Hibernate is a no-op. The quality analyzer has no cumulative working state;
// all output is emitted per-commit as TCs.
func (a *Analyzer) Hibernate() error { return nil }

// Boot is a no-op.
func (a *Analyzer) Boot() error { return nil }

// workingStateSize is 0 — the quality analyzer accumulates no working state.
const workingStateSize = 0

// avgTCSize is the estimated bytes of TC payload per commit (quality metrics).
const avgTCSize = 2 * 1024

// WorkingStateSize returns the estimated bytes of working state per commit.
func (a *Analyzer) WorkingStateSize() int64 { return workingStateSize }

// AvgTCSize returns the estimated bytes of TC payload per commit.
func (a *Analyzer) AvgTCSize() int64 { return avgTCSize }
