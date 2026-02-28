package typos

import "github.com/Sumatoshi-tech/codefang/internal/streaming"

var _ streaming.Hibernatable = (*Analyzer)(nil)

// Hibernate is a no-op. The typos analyzer has no cumulative working state;
// all output is emitted per-commit as TCs.
func (t *Analyzer) Hibernate() error { return nil }

// Boot is a no-op.
func (t *Analyzer) Boot() error { return nil }

// workingStateSize is 0 — the typos analyzer accumulates no working state.
const workingStateSize = 0

// avgTCSize is the estimated bytes of TC payload per commit.
const avgTCSize = 200

// WorkingStateSize returns the estimated bytes of working state per commit.
func (t *Analyzer) WorkingStateSize() int64 { return workingStateSize }

// AvgTCSize returns the estimated bytes of TC payload per commit.
func (t *Analyzer) AvgTCSize() int64 { return avgTCSize }
