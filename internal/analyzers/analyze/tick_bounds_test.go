package analyze_test

import (
	"testing"
	"time"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
)

var (
	testTime1 = time.Date(2024, 1, 15, 10, 0, 0, 0, time.UTC)
	testTime2 = time.Date(2024, 1, 16, 12, 0, 0, 0, time.UTC)
	testTime3 = time.Date(2024, 1, 17, 14, 0, 0, 0, time.UTC)
)

func TestBuildTickBounds_Empty(t *testing.T) {
	t.Parallel()

	result := analyze.BuildTickBounds(nil)

	assert.Empty(t, result)
}

func TestBuildTickBounds_SingleTick(t *testing.T) {
	t.Parallel()

	ticks := []analyze.TICK{
		{Tick: 0, StartTime: testTime1, EndTime: testTime2},
	}

	result := analyze.BuildTickBounds(ticks)

	require.Len(t, result, 1)
	assert.Equal(t, testTime1, result[0].StartTime)
	assert.Equal(t, testTime2, result[0].EndTime)
}

func TestBuildTickBounds_MultipleTicks(t *testing.T) {
	t.Parallel()

	ticks := []analyze.TICK{
		{Tick: 0, StartTime: testTime1, EndTime: testTime2},
		{Tick: 1, StartTime: testTime2, EndTime: testTime3},
	}

	result := analyze.BuildTickBounds(ticks)

	require.Len(t, result, 2)
	assert.Equal(t, testTime1, result[0].StartTime)
	assert.Equal(t, testTime2, result[1].StartTime)
	assert.Equal(t, testTime3, result[1].EndTime)
}

func TestBuildTickBounds_ZeroTimesSkipped(t *testing.T) {
	t.Parallel()

	ticks := []analyze.TICK{
		{Tick: 0},
		{Tick: 1, StartTime: testTime1, EndTime: testTime2},
	}

	result := analyze.BuildTickBounds(ticks)

	require.Len(t, result, 2)
	assert.True(t, result[0].StartTime.IsZero())
	assert.Equal(t, testTime1, result[1].StartTime)
}

func TestTickBoundsFormatStartTime(t *testing.T) {
	t.Parallel()

	bounds := analyze.TickBounds{StartTime: testTime1, EndTime: testTime2}

	assert.Equal(t, "2024-01-15T10:00:00Z", bounds.FormatStartTime())
	assert.Equal(t, "2024-01-16T12:00:00Z", bounds.FormatEndTime())
}

func TestTickBoundsFormatStartTime_Zero(t *testing.T) {
	t.Parallel()

	bounds := analyze.TickBounds{}

	assert.Empty(t, bounds.FormatStartTime())
	assert.Empty(t, bounds.FormatEndTime())
}
