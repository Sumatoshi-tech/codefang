package anomaly

// FRD: specs/frds/FRD-20260301-anomaly-enrich-from-store.md.

import (
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestDetectExternalAnomalies(t *testing.T) {
	t.Parallel()

	ticks := []int{0, 1, 2, 3, 4}
	dimensions := map[string][]float64{
		"metric": {1.0, 1.0, 1.0, 1.0, 100.0},
	}

	const windowSize = 3

	const threshold = 2.0

	anomalies, summaries := detectExternalAnomalies("src", ticks, dimensions, windowSize, threshold)

	assert.NotEmpty(t, anomalies)
	require.Len(t, summaries, 1)
	assert.Equal(t, "src", summaries[0].Source)
	assert.Equal(t, "metric", summaries[0].Dimension)
	assert.Greater(t, summaries[0].HighestZ, threshold)
}

func TestDetectExternalAnomalies_MismatchedLengths(t *testing.T) {
	t.Parallel()

	ticks := []int{0, 1, 2}
	dimensions := map[string][]float64{
		"bad_dim": {1.0, 2.0}, // Length mismatch -- should be skipped.
	}

	const windowSize = 3

	const threshold = 2.0

	anomalies, summaries := detectExternalAnomalies("src", ticks, dimensions, windowSize, threshold)

	assert.Empty(t, anomalies)
	assert.Empty(t, summaries)
}

func TestDetectExternalAnomalies_MultipleDimensions(t *testing.T) {
	t.Parallel()

	ticks := []int{0, 1, 2, 3, 4}
	dimensions := map[string][]float64{
		"dim_a": {1.0, 1.0, 1.0, 1.0, 1.0},  // Stable -- no anomalies.
		"dim_b": {1.0, 1.0, 1.0, 1.0, 50.0}, // Spike at tick 4.
	}

	const windowSize = 3

	const threshold = 2.0

	_, summaries := detectExternalAnomalies("multi-dim", ticks, dimensions, windowSize, threshold)

	require.Len(t, summaries, 2)

	// Summaries are sorted by dimension name.
	assert.Equal(t, "dim_a", summaries[0].Dimension)
	assert.Equal(t, 0, summaries[0].Anomalies)

	assert.Equal(t, "dim_b", summaries[1].Dimension)
	assert.Positive(t, summaries[1].Anomalies)
}

func TestDetectExternalAnomalies_NoAnomalies(t *testing.T) {
	t.Parallel()

	// All values are identical -- no anomalies possible.
	ticks := []int{0, 1, 2, 3, 4}
	dimensions := map[string][]float64{
		"stable_metric": {5.0, 5.0, 5.0, 5.0, 5.0},
	}

	const windowSize = 3

	const threshold = 2.0

	anomalies, summaries := detectExternalAnomalies("stable", ticks, dimensions, windowSize, threshold)

	assert.Empty(t, anomalies)
	require.Len(t, summaries, 1)
	assert.Equal(t, 0, summaries[0].Anomalies)
	assert.InDelta(t, 0.0, summaries[0].HighestZ, 0.001)
}

func TestDetectExternalAnomalies_Empty(t *testing.T) {
	t.Parallel()

	anomalies, summaries := detectExternalAnomalies("src", nil, nil, 3, 2.0)

	assert.Empty(t, anomalies)
	assert.Empty(t, summaries)
}
