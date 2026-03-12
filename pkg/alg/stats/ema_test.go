package stats

import (
	"testing"

	"github.com/stretchr/testify/assert"
)

func TestNewEMA(t *testing.T) {
	t.Parallel()

	ema := NewEMA(0.3)
	assert.InDelta(t, 0, ema.Value(), 0.0001)
}

func TestEMA_FirstUpdateInitializes(t *testing.T) {
	t.Parallel()

	ema := NewEMA(0.3)
	got := ema.Update(10.0)
	assert.InDelta(t, 10.0, got, 0.0001)
	assert.InDelta(t, 10.0, ema.Value(), 0.0001)
}

func TestEMA_SubsequentUpdatesSmooth(t *testing.T) {
	t.Parallel()

	ema := NewEMA(0.3)
	ema.Update(10.0) // Initialize to 10.

	// Second update: 0.3 * 20 + 0.7 * 10 = 6 + 7 = 13.
	got := ema.Update(20.0)
	assert.InDelta(t, 13.0, got, 0.0001)
}

func TestEMA_AlphaOneTrucksExactly(t *testing.T) {
	t.Parallel()

	ema := NewEMA(1.0)
	ema.Update(10.0)
	got := ema.Update(20.0)
	// alpha=1: 1.0 * 20 + 0.0 * 10 = 20.
	assert.InDelta(t, 20.0, got, 0.0001)
}

func TestEMA_Initialized(t *testing.T) {
	t.Parallel()

	ema := NewEMA(0.3)
	assert.False(t, ema.Initialized())

	ema.Update(10.0)
	assert.True(t, ema.Initialized())
}

func TestEMA_Convergence(t *testing.T) {
	t.Parallel()

	ema := NewEMA(0.3)

	iterations := 100

	for range iterations {
		ema.Update(50.0)
	}

	// After many updates of the same value, EMA should converge to it.
	assert.InDelta(t, 50.0, ema.Value(), 0.0001)
}
