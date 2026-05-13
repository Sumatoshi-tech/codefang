package plumbing

import (
	"log"
	"sync/atomic"
	"time"
)

// minSaneCommitTime is the lower bound for a plausible committer timestamp.
// Git itself first shipped in 2005; commits stamped before 1990-01-01 are
// almost certainly the result of a corrupt commit object, an unset system
// clock (epoch 0 → 1970), or a deliberate `GIT_COMMITTER_DATE=` override.
//
// Without this clamp a single such commit pegged tick0 to ~1970, after
// which every modern commit's Sub(tick0) overflowed the int64-nanosecond
// [time.Duration] and clamped to ~292 years. That clamp leaked into burndown
// as a 106 740-day "analysis period". See ticks.go: the bug was sticky via
// max(tick, previousTick).
var minSaneCommitTime = time.Date(1990, time.January, 1, 0, 0, 0, 0, time.UTC)

// maxClockSkew is the upper-bound grace allowed past wall-clock time. A
// committer timestamp more than this far in the future is treated as
// anomalous regardless of repo content.
const maxClockSkew = 24 * time.Hour

// anomalyLogIntervalNanos throttles the per-event "anomalous committer
// timestamp" log line so a repo with thousands of bad commits doesn't
// drown the operator-facing log. Same shape as
// burndown/mismatch_tracker's log throttle.
const anomalyLogIntervalNanos = int64(time.Second)

// timeAnomalyTracker counts committer-timestamp anomalies detected during
// tick computation and rate-limits the warning log. Atomics make the
// tracker safe to call from the per-shard clones returned by Fork(); the
// sequential plumbing analyzer never actually races, but using atomics
// keeps Fork() safe by construction.
type timeAnomalyTracker struct {
	beforeMin    atomic.Int64 // Counter: timestamps before minSaneCommitTime.
	afterMax     atomic.Int64 // Counter: timestamps too far in the future.
	dropped      atomic.Int64 // Suppressed since last emitted log line.
	lastLogNanos atomic.Int64 // Monotonic-ish slot timestamp.
}

// recordBeforeMin bumps the before-min counter and emits a rate-limited
// warning. when is the bogus committer time we observed, replacement is
// the time we substituted into tick math.
func (t *timeAnomalyTracker) recordBeforeMin(when, replacement time.Time) {
	t.beforeMin.Add(1)
	t.maybeLog("before-min", when, replacement)
}

// recordAfterMax bumps the after-max counter and emits a rate-limited
// warning. Mirrors recordBeforeMin for the future-clamp side.
func (t *timeAnomalyTracker) recordAfterMax(when, replacement time.Time) {
	t.afterMax.Add(1)
	t.maybeLog("after-max", when, replacement)
}

// maybeLog emits one warning per anomalyLogIntervalNanos at most. Mirrors
// burndown.mismatchTracker.maybeLog: try to claim the slot via CAS; on
// failure (slot still warm), bump dropped and return silently. On success,
// flush the dropped tail in the emitted line.
func (t *timeAnomalyTracker) maybeLog(kind string, when, replacement time.Time) {
	now := time.Now().UnixNano()
	last := t.lastLogNanos.Load()

	if now-last < anomalyLogIntervalNanos {
		t.dropped.Add(1)

		return
	}

	if !t.lastLogNanos.CompareAndSwap(last, now) {
		t.dropped.Add(1)

		return
	}

	dropped := t.dropped.Swap(0)
	if dropped == 0 {
		log.Printf("ticks: %s anomalous committer timestamp %s, substituted %s",
			kind, when.Format(time.RFC3339), replacement.Format(time.RFC3339))

		return
	}

	log.Printf("ticks: %s anomalous committer timestamp %s, substituted %s [dropped=%d since last]",
		kind, when.Format(time.RFC3339), replacement.Format(time.RFC3339), dropped)
}

// snapshot returns the running counts. Used by accessor TimeAnomalies()
// for tests and external observers.
func (t *timeAnomalyTracker) snapshot() TimeAnomalyStats {
	return TimeAnomalyStats{
		BeforeMin: t.beforeMin.Load(),
		AfterMax:  t.afterMax.Load(),
	}
}

// TimeAnomalyStats reports anomalous committer-timestamp detections.
//
// BeforeMin counts commits whose committer time was earlier than the
// hard-coded floor (1990-01-01 UTC) — typically epoch-0 (1970) values
// from corrupt commit objects, unset system clocks, or deliberate
// GIT_COMMITTER_DATE overrides.
//
// AfterMax counts commits whose committer time was more than 24h past
// the analyzer's wall-clock — typically forged future timestamps
// ("--date=2099-01-01") or clock skew at commit time.
//
// In both cases the substituted time is the previous valid committer
// timestamp (or 1990-01-01 UTC if no valid commit has been seen yet),
// so the bad commit collapses onto the timeline at a sensible point
// instead of overflowing the int64-nanosecond Duration in ticks.go.
type TimeAnomalyStats struct {
	BeforeMin int64
	AfterMax  int64
}

// Total returns the combined count of anomalies on both bounds.
func (s TimeAnomalyStats) Total() int64 {
	return s.BeforeMin + s.AfterMax
}
