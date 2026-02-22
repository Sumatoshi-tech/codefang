package streaming_test

import (
	"io"
	"log/slog"
	"sync/atomic"
	"testing"

	"github.com/stretchr/testify/assert"

	"github.com/Sumatoshi-tech/codefang/pkg/streaming"
)

type mockSpillCleaner struct {
	calls atomic.Int32
}

func (m *mockSpillCleaner) CleanupSpills() {
	m.calls.Add(1)
}

func discardLogger() *slog.Logger {
	return slog.New(slog.NewTextHandler(io.Discard, nil))
}

func TestSpillCleanupGuard_CleanupOnClose(t *testing.T) {
	t.Parallel()

	c1 := &mockSpillCleaner{}
	c2 := &mockSpillCleaner{}

	guard := streaming.NewSpillCleanupGuard(
		[]streaming.SpillCleaner{c1, c2}, discardLogger())
	guard.Close()

	assert.Equal(t, int32(1), c1.calls.Load())
	assert.Equal(t, int32(1), c2.calls.Load())
}

func TestSpillCleanupGuard_IdempotentClose(t *testing.T) {
	t.Parallel()

	c := &mockSpillCleaner{}

	guard := streaming.NewSpillCleanupGuard(
		[]streaming.SpillCleaner{c}, discardLogger())
	guard.Close()

	// Second close should not call cleanup again.
	assert.Equal(t, int32(1), c.calls.Load())
}

func TestSpillCleanupGuard_NilCleaners(t *testing.T) {
	t.Parallel()

	guard := streaming.NewSpillCleanupGuard(nil, discardLogger())
	guard.Close()
}
