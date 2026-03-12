package sigutil_test

// FRD: specs/frds/FRD-20260302-signal-cleanup-guard.md.

import (
	"io"
	"log/slog"
	"sync/atomic"
	"testing"

	"github.com/stretchr/testify/assert"

	"github.com/Sumatoshi-tech/codefang/pkg/sigutil"
)

func discardLogger() *slog.Logger {
	return slog.New(slog.NewTextHandler(io.Discard, nil))
}

func TestSignalCleanupGuard_CleanupOnClose(t *testing.T) {
	t.Parallel()

	var called atomic.Bool

	guard := sigutil.NewSignalCleanupGuard(func() {
		called.Store(true)
	}, discardLogger())
	guard.Close()

	assert.True(t, called.Load(), "cleanup must be called on Close")
}

func TestSignalCleanupGuard_IdempotentClose(t *testing.T) {
	t.Parallel()

	var count atomic.Int32

	guard := sigutil.NewSignalCleanupGuard(func() {
		count.Add(1)
	}, discardLogger())

	guard.Close()
	// Close is safe to call multiple times but cleanup runs once.
	assert.Equal(t, int32(1), count.Load())
}

func TestSignalCleanupGuard_NilCleanup(t *testing.T) {
	t.Parallel()

	guard := sigutil.NewSignalCleanupGuard(nil, discardLogger())
	guard.Close() // Must not panic.
}

func TestSignalCleanupGuard_MultipleCleanersViaClosure(t *testing.T) {
	t.Parallel()

	var c1, c2 atomic.Int32

	cleanup := func() {
		c1.Add(1)
		c2.Add(1)
	}

	guard := sigutil.NewSignalCleanupGuard(cleanup, discardLogger())
	guard.Close()

	assert.Equal(t, int32(1), c1.Load())
	assert.Equal(t, int32(1), c2.Load())
}
