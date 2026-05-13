package plumbing

import (
	"context"
	"sync"
	"testing"
	"time"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

func newTicks(t *testing.T) *TicksSinceStart {
	t.Helper()

	ts := &TicksSinceStart{}

	err := ts.Initialize(nil)
	if err != nil {
		t.Fatalf("Initialize: %v", err)
	}

	return ts
}

// makeCommit builds a minimal gitlib.TestCommit suitable for driving
// TicksSinceStart.Consume — only Hash, Committer, and NumParents are read
// by the tick path.
func makeCommit(when time.Time, hashByte byte) *gitlib.TestCommit {
	parent := gitlib.Hash{}
	parent[0] = hashByte // Any non-zero parent makes NumParents() > 0.
	commit := gitlib.NewTestCommit(
		gitlib.Hash{hashByte},
		gitlib.Signature{Name: "T", Email: "t@t", When: when},
		"msg",
		parent,
	)

	return commit
}

func consume(t *testing.T, ts *TicksSinceStart, when time.Time, index int) int {
	t.Helper()

	commit := makeCommit(when, byte(index+1))

	_, err := ts.Consume(context.Background(), &analyze.Context{
		Commit: commit,
		Index:  index,
	})
	if err != nil {
		t.Fatalf("Consume: %v", err)
	}

	return ts.Tick
}

func TestSanitizeWhen_BeforeMin_FirstCommit_FallsBackToMinSaneTime(t *testing.T) {
	t.Parallel()

	ts := newTicks(t)

	// First commit at unix epoch (1970) — the canonical "epoch-zero
	// committer" failure mode that previously pegged tick0 to 1970 and
	// produced a 106 740-day analysis period.
	got := consume(t, ts, time.Unix(0, 0), 0)
	if got != 0 {
		t.Errorf("first-commit tick = %d, want 0 (anomaly must collapse to start)", got)
	}

	if stats := ts.TimeAnomalies(); stats.BeforeMin != 1 {
		t.Errorf("BeforeMin = %d, want 1", stats.BeforeMin)
	}

	if !ts.tick0Set {
		t.Error("tick0Set must be true after first consume even on anomaly")
	}

	if !ts.tick0.Equal(FloorTime(minSaneCommitTime, ts.TickSize)) {
		t.Errorf("tick0 = %s, want floor(%s) (must seed from sanitized substitute)",
			ts.tick0.Format(time.RFC3339), minSaneCommitTime.Format(time.RFC3339))
	}
}

func TestSanitizeWhen_BeforeMin_AfterValidCommit_UsesLastValid(t *testing.T) {
	t.Parallel()

	ts := newTicks(t)

	// Seed with a normal commit to populate lastValidWhen.
	good := time.Date(2024, time.April, 1, 0, 0, 0, 0, time.UTC)

	tick0 := consume(t, ts, good, 0)
	if tick0 != 0 {
		t.Fatalf("seed tick = %d, want 0", tick0)
	}

	// Then a bogus epoch-0 commit. Its tick must equal the previous tick
	// (no time travel) — substitution = lastValidWhen.
	tick1 := consume(t, ts, time.Unix(0, 0), 1)
	if tick1 != 0 {
		t.Errorf("anomalous tick after valid = %d, want 0 (must reuse lastValidWhen)", tick1)
	}

	// And a normal commit one day later still ticks forward as expected.
	tick2 := consume(t, ts, good.Add(24*time.Hour), 2)
	if tick2 != 1 {
		t.Errorf("post-anomaly tick = %d, want 1 (anomaly must not poison the timeline)", tick2)
	}
}

func TestSanitizeWhen_AfterMax_ForgedFutureCommit_DoesNotPoisonTimeline(t *testing.T) {
	t.Parallel()

	ts := newTicks(t)

	good := time.Date(2024, time.April, 1, 0, 0, 0, 0, time.UTC)
	consume(t, ts, good, 0)

	// `git commit --date=2099-01-01` style — far past now+24h.
	forged := time.Date(2099, time.January, 1, 0, 0, 0, 0, time.UTC)
	tickForged := consume(t, ts, forged, 1)

	// Without the fix the forged tick would explode (and stick via
	// max(tick, previousTick)). With the fix it collapses to the
	// previous valid tick.
	if tickForged != 0 {
		t.Errorf("forged-future tick = %d, want 0 (must clamp to lastValidWhen)", tickForged)
	}

	// Subsequent valid commit ticks forward by exactly 1 day.
	next := consume(t, ts, good.Add(24*time.Hour), 2)
	if next != 1 {
		t.Errorf("post-forged tick = %d, want 1 (forgery must not stick via previousTick)", next)
	}

	if stats := ts.TimeAnomalies(); stats.AfterMax != 1 {
		t.Errorf("AfterMax = %d, want 1", stats.AfterMax)
	}
}

func TestSanitizeWhen_NormalRange_UnchangedAndUpdatesLastValid(t *testing.T) {
	t.Parallel()

	ts := newTicks(t)

	when := time.Date(2024, time.April, 1, 12, 0, 0, 0, time.UTC)
	got := ts.sanitizeWhen(when)

	if !got.Equal(when) {
		t.Errorf("in-window time was modified: got %s, want %s", got, when)
	}

	if !ts.lastValidWhen.Equal(when) {
		t.Errorf("lastValidWhen = %s, want %s (must update on valid input)", ts.lastValidWhen, when)
	}

	if stats := ts.TimeAnomalies(); stats.Total() != 0 {
		t.Errorf("anomalies total = %d, want 0 for in-window input", stats.Total())
	}
}

func TestSanitizeWhen_ClockSkewWithinGrace_PassesThrough(t *testing.T) {
	t.Parallel()

	ts := newTicks(t)

	// 1 hour into the future is within maxClockSkew (24h) — should pass.
	when := time.Now().Add(1 * time.Hour)
	got := ts.sanitizeWhen(when)

	if !got.Equal(when) {
		t.Errorf("within-grace future time was rejected: got %s, want %s", got, when)
	}

	if stats := ts.TimeAnomalies(); stats.AfterMax != 0 {
		t.Errorf("AfterMax = %d, want 0 (grace window must allow small clock skew)", stats.AfterMax)
	}
}

func TestTimeAnomalyTracker_RateLimit_DropsBurstWithinInterval(t *testing.T) {
	t.Parallel()

	var tr timeAnomalyTracker

	when := time.Unix(0, 0)
	repl := minSaneCommitTime

	for range 1000 {
		tr.recordBeforeMin(when, repl)
	}

	if got := tr.dropped.Load(); got != 999 {
		t.Errorf("dropped = %d, want 999 (1000 events, 1 logged, 999 suppressed)", got)
	}

	if got := tr.snapshot().BeforeMin; got != 1000 {
		t.Errorf("BeforeMin = %d, want 1000 (counter must record every event)", got)
	}
}

func TestTimeAnomalyTracker_ConcurrentRecord_NoLostUpdates(t *testing.T) {
	t.Parallel()

	var (
		tr        timeAnomalyTracker
		wg        sync.WaitGroup
		perWorker = int64(500)
		workers   = 8
	)

	when := time.Unix(0, 0)
	repl := minSaneCommitTime

	wg.Add(workers)

	for range workers {
		go func() {
			defer wg.Done()

			for range int(perWorker) {
				tr.recordBeforeMin(when, repl)
			}
		}()
	}

	wg.Wait()

	want := perWorker * int64(workers)
	if got := tr.snapshot().BeforeMin; got != want {
		t.Errorf("BeforeMin = %d, want %d (concurrent atomic updates must not lose any)", got, want)
	}
}

func TestTimeAnomalyStats_Total_SumsBothBounds(t *testing.T) {
	t.Parallel()

	s := TimeAnomalyStats{BeforeMin: 4, AfterMax: 7}
	if got := s.Total(); got != 11 {
		t.Errorf("Total = %d, want 11", got)
	}
}

// TestRegressionAnalysisPeriodOverflow reproduces the bug shape: a single
// epoch-0 commit followed by normal commits used to produce a tick range
// of ~106 751 days (the [time.Duration] int64 overflow clamp). With the
// sanitization in place the tick range is bounded by real commit deltas.
func TestRegressionAnalysisPeriodOverflow_NoLongerProduces292Years(t *testing.T) {
	t.Parallel()

	ts := newTicks(t)

	// First commit: epoch-0 (the trigger).
	consume(t, ts, time.Unix(0, 0), 0)

	// Then 5 commits one day apart in 2024.
	base := time.Date(2024, time.April, 1, 0, 0, 0, 0, time.UTC)

	for i := range 5 {
		got := consume(t, ts, base.Add(time.Duration(i)*24*time.Hour), i+1)
		// Ticks are measured from minSaneCommitTime (1990-01-01). So
		// each 2024-04-0X commit lands ~12 510..12 514 days in. The
		// important property: ticks are NOT clamped to ~106 751.
		const overflowSentinel = 100_000

		if got > overflowSentinel {
			t.Errorf("tick %d for normal commit i=%d — overflow clamp regressed",
				got, i)
		}
	}
}
