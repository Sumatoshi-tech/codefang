package burndown

import (
	"sync"
	"testing"
)

func TestMismatchTracker_RecordReset_BumpsResetsCounter(t *testing.T) {
	t.Parallel()

	var tr mismatchTracker

	tr.recordReset("foo.go", 12)
	tr.recordReset("bar.go", 34)

	stats := tr.snapshot()
	if stats.Resets != 2 {
		t.Errorf("Resets = %d, want 2", stats.Resets)
	}

	if stats.ForceRemoves != 0 {
		t.Errorf("ForceRemoves = %d, want 0", stats.ForceRemoves)
	}
}

func TestMismatchTracker_RecordForceRemove_BumpsForceRemovesCounter(t *testing.T) {
	t.Parallel()

	var tr mismatchTracker

	tr.recordForceRemove("foo.go", 99)

	stats := tr.snapshot()
	if stats.ForceRemoves != 1 {
		t.Errorf("ForceRemoves = %d, want 1", stats.ForceRemoves)
	}

	if stats.Resets != 0 {
		t.Errorf("Resets = %d, want 0", stats.Resets)
	}
}

func TestMismatchTracker_RateLimit_DropsBurstWithinInterval(t *testing.T) {
	t.Parallel()

	var tr mismatchTracker

	// Fire a burst of 1000 resets back-to-back. Only the first should win
	// the log slot; the rest must be counted as dropped.
	for range 1000 {
		tr.recordReset("foo.go", 1)
	}

	if got := tr.dropped.Load(); got != 999 {
		t.Errorf("dropped = %d, want 999 (1000 events, 1 logged, 999 suppressed)", got)
	}

	if got := tr.snapshot().Resets; got != 1000 {
		t.Errorf("Resets = %d, want 1000 (counter must record every event regardless of log throttle)", got)
	}
}

func TestMismatchTracker_RateLimit_AllowsAfterInterval(t *testing.T) {
	t.Parallel()

	var tr mismatchTracker

	// First call wins the slot.
	tr.recordReset("foo.go", 1)
	first := tr.lastLogNanos.Load()

	// Force the next call into a fresh interval by rewinding the timestamp.
	tr.lastLogNanos.Store(first - mismatchLogIntervalNanos - 1)

	// Reset dropped so we can verify the second call resets the dropped tail.
	tr.dropped.Store(5)

	tr.recordReset("bar.go", 2)

	if got := tr.dropped.Load(); got != 0 {
		t.Errorf("dropped after fresh interval = %d, want 0 (Swap should clear it on emit)", got)
	}

	if tr.lastLogNanos.Load() == first {
		t.Errorf("lastLogNanos did not advance — second call did not claim the slot")
	}
}

func TestMismatchTracker_ChunkDelta_TracksSinceBaseline(t *testing.T) {
	t.Parallel()

	var tr mismatchTracker

	tr.recordReset("a", 1)
	tr.recordReset("b", 1)
	tr.resetChunkBaseline()

	if got := tr.chunkDelta(); got != 0 {
		t.Errorf("chunkDelta after baseline = %d, want 0", got)
	}

	tr.recordReset("c", 1)
	tr.recordForceRemove("d", 1)

	if got := tr.chunkDelta(); got != 2 {
		t.Errorf("chunkDelta after 2 events = %d, want 2", got)
	}

	// Cumulative counters keep climbing.
	if got := tr.snapshot().Total(); got != 4 {
		t.Errorf("Total = %d, want 4 (cumulative across baseline reset)", got)
	}
}

func TestMismatchTracker_ConcurrentRecord_NoLostUpdates(t *testing.T) {
	t.Parallel()

	var (
		tr        mismatchTracker
		wg        sync.WaitGroup
		perWorker = int64(500)
		workers   = 8
	)

	wg.Add(workers)

	for range workers {
		go func() {
			defer wg.Done()

			for range int(perWorker) {
				tr.recordReset("x", 1)
			}
		}()
	}

	wg.Wait()

	want := perWorker * int64(workers)
	if got := tr.snapshot().Resets; got != want {
		t.Errorf("Resets = %d, want %d (concurrent atomic updates must not lose any)", got, want)
	}

	// At most one log per interval; bound the number that could have won
	// the slot during this short test (a few, definitely not all).
	logged := tr.snapshot().Resets - tr.dropped.Load()
	if logged < 1 {
		t.Errorf("logged events = %d, want at least 1", logged)
	}

	if logged > want {
		t.Errorf("logged events = %d > total = %d, dropped count is broken", logged, want)
	}
}

func TestMismatchStats_Total_SumsBothCounters(t *testing.T) {
	t.Parallel()

	s := MismatchStats{Resets: 7, ForceRemoves: 3}

	if got := s.Total(); got != 10 {
		t.Errorf("Total = %d, want 10", got)
	}
}
