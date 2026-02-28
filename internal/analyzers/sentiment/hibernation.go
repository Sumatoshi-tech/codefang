package sentiment

import "github.com/Sumatoshi-tech/codefang/internal/streaming"

var _ streaming.Hibernatable = (*Analyzer)(nil)

// Hibernate is a no-op. The sentiment analyzer has no cumulative working state;
// all output is emitted per-commit as TCs.
func (s *Analyzer) Hibernate() error { return nil }

// Boot is a no-op.
func (s *Analyzer) Boot() error { return nil }

// workingStateSize is 0 — the sentiment analyzer accumulates no working state.
const workingStateSize = 0

// avgTCSize is the estimated bytes of TC payload per commit (comment list).
const avgTCSize = 500

// WorkingStateSize returns the estimated bytes of working state per commit.
func (s *Analyzer) WorkingStateSize() int64 { return workingStateSize }

// AvgTCSize returns the estimated bytes of TC payload per commit.
func (s *Analyzer) AvgTCSize() int64 { return avgTCSize }
