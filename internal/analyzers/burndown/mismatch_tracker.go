package burndown

import (
	"log"
	"sync/atomic"
	"time"
)

// mismatchLogIntervalNanos throttles the per-event src-mismatch log line.
// Bursts are common (one large commit can reset thousands of file states
// in a single tick after the blob-pipeline cap silently skips a monster
// commit upstream); without throttling, the log becomes the long pole.
// 1 second across all shards keeps the operator-facing signal while
// dropping the cost from O(mismatches) stdout flushes to O(seconds).
const mismatchLogIntervalNanos = int64(time.Second)

// mismatchTracker counts src-mismatch reset events on the burndown analyzer
// and rate-limits the per-event log line. All fields are accessed atomically
// so the tracker is safe to call from per-shard goroutines.
//
// The counter splits resets (file present, line count diverged) from
// force-removes (file deleted while line count diverged) so consumers can
// tell apart the two recovery paths handled by history_changes.go.
type mismatchTracker struct {
	resets        atomic.Int64
	forceRemoves  atomic.Int64
	dropped       atomic.Int64 // events suppressed since the last emitted log line.
	lastLogNanos  atomic.Int64 // monotonic-ish timestamp of last emitted log line.
	chunkBaseline atomic.Int64 // resets+forceRemoves at start of current chunk.
}

// recordReset bumps the reset counter and emits a rate-limited log line.
// name is the file path; tracked is the analyzer's stale line count for it.
func (t *mismatchTracker) recordReset(name string, tracked int) {
	t.resets.Add(1)
	t.maybeLog(name, tracked, "resetting")
}

// recordForceRemove bumps the force-remove counter and emits a rate-limited
// log line. Mirrors recordReset for the deletion path so the two recovery
// modes show up as separate counters.
func (t *mismatchTracker) recordForceRemove(name string, tracked int) {
	t.forceRemoves.Add(1)
	t.maybeLog("deletion "+name, tracked, "force-removing")
}

// maybeLog emits a log line at most once per mismatchLogIntervalNanos, atomic
// across shards. Suppressed events are counted in `dropped` and surfaced as a
// `dropped=N since last` suffix on the next emitted line.
func (t *mismatchTracker) maybeLog(name string, tracked int, kind string) {
	now := time.Now().UnixNano()
	last := t.lastLogNanos.Load()

	if now-last < mismatchLogIntervalNanos {
		t.dropped.Add(1)

		return
	}

	if !t.lastLogNanos.CompareAndSwap(last, now) {
		// Another shard claimed this slot — count as dropped to keep the
		// total consistent with one-log-per-interval semantics.
		t.dropped.Add(1)

		return
	}

	dropped := t.dropped.Swap(0)
	if dropped == 0 {
		log.Printf("burndown: src mismatch for %s (tracked=%d, diff_old=...), %s",
			name, tracked, kind)

		return
	}

	log.Printf("burndown: src mismatch for %s (tracked=%d, diff_old=...), %s [dropped=%d since last]",
		name, tracked, kind, dropped)
}

// snapshot returns the running counts. Used by Hibernate() for chunk summaries
// and exposed to external observers via HistoryAnalyzer.MismatchStats.
func (t *mismatchTracker) snapshot() MismatchStats {
	return MismatchStats{
		Resets:       t.resets.Load(),
		ForceRemoves: t.forceRemoves.Load(),
	}
}

// resetChunkBaseline marks the cumulative count at the start of a chunk so
// per-chunk deltas can be reported on the next Hibernate.
func (t *mismatchTracker) resetChunkBaseline() {
	t.chunkBaseline.Store(t.resets.Load() + t.forceRemoves.Load())
}

// chunkDelta returns the number of mismatch events recorded since the last
// resetChunkBaseline call.
func (t *mismatchTracker) chunkDelta() int64 {
	return (t.resets.Load() + t.forceRemoves.Load()) - t.chunkBaseline.Load()
}

// MismatchStats reports cumulative src-mismatch reset events on the burndown
// analyzer. Consumers (tests, observability) read these via
// HistoryAnalyzer.MismatchStats().
//
// Resets count file modifications where the analyzer's tracked line count
// did not match the diff's OldLinesOfCode — typically after the blob
// pipeline silently skipped a "monster" commit (see ErrCommitTooLarge), so
// the analyzer's state lags reality by one or more commits' worth of edits.
// ForceRemoves count the same divergence on the deletion path.
//
// A non-zero value implies burndown's per-file survival history is stale
// for the affected files at the reset point — the file is treated as a
// fresh insertion thereafter. Surface this to operators when interpreting
// per-file results on repos with large mass-update commits (vendor moves,
// generated-code regenerations, Pods updates).
type MismatchStats struct {
	Resets       int64
	ForceRemoves int64
}

// Total returns the sum of resets and force-removes.
func (s MismatchStats) Total() int64 {
	return s.Resets + s.ForceRemoves
}
