package observability_test

// FRD: specs/frds/FRD-20260302-sysmetrics-move.md.

import (
	"runtime"
	"testing"

	"github.com/stretchr/testify/assert"

	"github.com/Sumatoshi-tech/codefang/internal/observability"
)

func TestTakeHeapSnapshot_ReturnsPositiveValues(t *testing.T) {
	t.Parallel()

	snap := observability.TakeHeapSnapshot()
	assert.Positive(t, snap.HeapInuse)
	assert.Positive(t, snap.HeapAlloc)
	assert.Positive(t, snap.TakenAtNS)
}

func TestTakeHeapSnapshot_SysIncludesRuntime(t *testing.T) {
	t.Parallel()

	snap := observability.TakeHeapSnapshot()

	// Sys should be at least as large as HeapInuse.
	assert.GreaterOrEqual(t, snap.Sys, snap.HeapInuse,
		"Sys should be >= HeapInuse (it includes heap + stack + other)")
}

func TestTakeHeapSnapshot_TimestampIsRecent(t *testing.T) {
	t.Parallel()

	snap := observability.TakeHeapSnapshot()

	// Timestamp should be a reasonable Unix nano (after 2020-01-01).
	const minTimestamp int64 = 1577836800_000000000 // 2020-01-01 UTC in ns.
	assert.Greater(t, snap.TakenAtNS, minTimestamp)
}

func TestTakeHeapSnapshot_NewFields(t *testing.T) {
	t.Parallel()

	snap := observability.TakeHeapSnapshot()

	assert.Positive(t, snap.HeapObjects, "HeapObjects should be positive")
	assert.Positive(t, snap.StackInuse, "StackInuse should be positive")
	assert.Positive(t, snap.NextGC, "NextGC should be positive")
	assert.Positive(t, snap.Goroutines, "Goroutines should be positive")
}

func TestReadRSSBytes_NonNegative(t *testing.T) {
	t.Parallel()

	rss := observability.ReadRSSBytes()

	// RSS is 0 on non-Linux or positive on Linux.
	assert.GreaterOrEqual(t, rss, int64(0))

	if runtime.GOOS == "linux" {
		assert.Positive(t, rss, "RSS should be positive on Linux")
	}
}

func TestReadSmapsRollup_NonNegative(t *testing.T) {
	t.Parallel()

	smaps := observability.ReadSmapsRollup()

	if runtime.GOOS == "linux" {
		assert.Positive(t, smaps.Rss, "smaps Rss should be positive on Linux")
		assert.GreaterOrEqual(t, smaps.Anonymous, int64(0))
		// FileBacked = Rss - Anonymous, can be zero if all memory is anonymous.
		assert.GreaterOrEqual(t, smaps.FileBacked, int64(0))
	}
}
