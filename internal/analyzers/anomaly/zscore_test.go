package anomaly

import (
	"math"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/alg/stats"
)

func DetectAnomalies(scores []float64, threshold float64) []int {
	var anomalies []int

	for i, score := range scores {
		if math.Abs(score) > threshold {
			anomalies = append(anomalies, i)
		}
	}

	return anomalies
}

func TestComputeZScores_BasicSpike(t *testing.T) {
	t.Parallel()

	// Stable values with one spike at index 4.
	values := []float64{10, 10, 10, 10, 100, 10, 10}
	window := 3

	scores := ComputeZScores(values, window)

	require.Len(t, scores, len(values))

	// First 'window' entries have partial windows; spike at index 4 should have high Z-score.
	assert.Greater(t, math.Abs(scores[4]), 2.0, "spike should have Z-score > 2")

	// Stable values should have low Z-scores.
	assert.Less(t, math.Abs(scores[1]), 1.0, "stable value should have low Z-score")
}

func TestComputeZScores_ZeroStdDev(t *testing.T) {
	t.Parallel()

	// All identical values: stddev = 0, Z-score should be 0.
	values := []float64{5, 5, 5, 5, 5}
	window := 3

	scores := ComputeZScores(values, window)

	require.Len(t, scores, len(values))

	for i, score := range scores {
		assert.InDelta(t, 0, score, 1e-9, "identical values should have Z-score 0 at index %d", i)
	}
}

func TestComputeZScores_EmptyInput(t *testing.T) {
	t.Parallel()

	scores := ComputeZScores(nil, 3)
	assert.Empty(t, scores)

	scores = ComputeZScores([]float64{}, 3)
	assert.Empty(t, scores)
}

func TestComputeZScores_SingleValue(t *testing.T) {
	t.Parallel()

	scores := ComputeZScores([]float64{42}, 3)

	require.Len(t, scores, 1)
	assert.InDelta(t, 0, scores[0], 1e-9, "single value should have Z-score 0")
}

func TestComputeZScores_WindowLargerThanData(t *testing.T) {
	t.Parallel()

	values := []float64{10, 20, 10}
	window := 100

	scores := ComputeZScores(values, window)

	require.Len(t, scores, len(values))
	// Should not panic; uses all available data as partial window.
}

func TestComputeZScores_WindowOfOne(t *testing.T) {
	t.Parallel()

	// Window of 1: only one value in window, stddev = 0.
	// When value differs from the single-element mean, sentinel is returned.
	values := []float64{10, 10, 10}
	window := 1

	scores := ComputeZScores(values, window)

	require.Len(t, scores, len(values))

	// Index 0: empty window => Z=0.
	assert.InDelta(t, 0, scores[0], 1e-9, "empty window should have Z-score 0")
	// Index 1: window=[10], value=10, stddev=0, value==mean => Z=0.
	assert.InDelta(t, 0, scores[1], 1e-9, "same value should have Z-score 0")
	// Index 2: window=[10], value=10, stddev=0, value==mean => Z=0.
	assert.InDelta(t, 0, scores[2], 1e-9, "same value should have Z-score 0")
}

func TestComputeZScores_SentinelOnZeroStdDevWithDiff(t *testing.T) {
	t.Parallel()

	// Window of 1: stddev=0, but value differs from mean => sentinel returned.
	values := []float64{10, 50}
	window := 1

	scores := ComputeZScores(values, window)

	require.Len(t, scores, 2)
	assert.InDelta(t, 0, scores[0], 1e-9, "empty window => Z=0")
	assert.InDelta(t, stats.ZScoreMaxSentinel, scores[1], 1e-9, "diff from mean with zero stddev => sentinel")
}

func TestComputeZScores_KnownValues(t *testing.T) {
	t.Parallel()

	// Window=3: for index 3, window is [values[0], values[1], values[2]].
	// mean = (10+10+10)/3 = 10, stddev = 0 => Z-score = 0 for value 10.
	// For index 4 (value=50), window is [10, 10, 10], mean=10, stddev=0 => Z=0?
	// No - with stddev=0 and value != mean, we still return 0 by convention.
	// Let's use a window that produces non-zero stddev.
	values := []float64{10, 12, 8, 11, 50, 9, 10}
	window := 4

	scores := ComputeZScores(values, window)

	require.Len(t, scores, len(values))

	// Index 4 (value=50): window is [10, 12, 8, 11], mean=10.25, stddev~=1.48.
	// Z = (50 - 10.25) / 1.48 ~ 26.9 — very high.
	assert.Greater(t, scores[4], 5.0, "large spike should produce high Z-score")
}

func TestDetectAnomalies_BasicDetection(t *testing.T) {
	t.Parallel()

	scores := []float64{0.1, 0.2, -0.3, 0.1, 5.0, -0.2, 0.1}
	threshold := 2.0

	anomalies := DetectAnomalies(scores, threshold)

	require.Len(t, anomalies, 1)
	assert.Equal(t, 4, anomalies[0])
}

func TestDetectAnomalies_NegativeSpike(t *testing.T) {
	t.Parallel()

	scores := []float64{0.1, 0.2, -5.0, 0.1}
	threshold := 2.0

	anomalies := DetectAnomalies(scores, threshold)

	require.Len(t, anomalies, 1)
	assert.Equal(t, 2, anomalies[0], "negative Z-score should also be detected")
}

func TestDetectAnomalies_NoAnomalies(t *testing.T) {
	t.Parallel()

	scores := []float64{0.1, 0.2, -0.3, 0.1}
	threshold := 2.0

	anomalies := DetectAnomalies(scores, threshold)
	assert.Empty(t, anomalies)
}

func TestDetectAnomalies_EmptyInput(t *testing.T) {
	t.Parallel()

	anomalies := DetectAnomalies(nil, 2.0)
	assert.Empty(t, anomalies)
}

func TestMeanStdDev_Basic(t *testing.T) {
	t.Parallel()

	mean, stddev := MeanStdDev([]float64{10, 20, 30})

	assert.InDelta(t, 20.0, mean, 1e-9)
	// Population stddev of [10,20,30] = sqrt(((10-20)^2 + (20-20)^2 + (30-20)^2)/3) = sqrt(200/3) ~ 8.165.
	assert.InDelta(t, math.Sqrt(200.0/3.0), stddev, 1e-9)
}

func TestMeanStdDev_SingleValue(t *testing.T) {
	t.Parallel()

	mean, stddev := MeanStdDev([]float64{42})

	assert.InDelta(t, 42.0, mean, 1e-9)
	assert.InDelta(t, 0.0, stddev, 1e-9)
}

func TestMeanStdDev_Empty(t *testing.T) {
	t.Parallel()

	mean, stddev := MeanStdDev(nil)

	assert.InDelta(t, 0.0, mean, 1e-9)
	assert.InDelta(t, 0.0, stddev, 1e-9)
}
