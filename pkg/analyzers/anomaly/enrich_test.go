package anomaly

import (
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

func TestEnrichFromReports_Basic(t *testing.T) {
	t.Parallel()
	withIsolatedRegistry(t)

	// Register a test extractor with a spike at tick 4.
	RegisterTimeSeriesExtractor("test-source", func(_ analyze.Report) ([]int, map[string][]float64) {
		return []int{0, 1, 2, 3, 4}, map[string][]float64{
			"metric_a": {1.0, 1.0, 1.0, 1.0, 100.0},
		}
	})

	anomalyReport := analyze.Report{
		"anomalies":       []Record{},
		"commit_metrics":  map[string]*CommitAnomalyData{},
		"commits_by_tick": map[int][]gitlib.Hash{},
		"threshold":       float32(2.0),
		"window_size":     3,
	}

	otherReports := map[string]analyze.Report{
		"test-source": {},
	}

	EnrichFromReports(anomalyReport, otherReports, 3, 2.0)

	extAnomalies, ok := anomalyReport["external_anomalies"].([]ExternalAnomaly)
	require.True(t, ok)
	assert.NotEmpty(t, extAnomalies)

	// The spike at tick 4 should be detected.
	found := false

	for _, a := range extAnomalies {
		if a.Source == "test-source" && a.Dimension == "metric_a" && a.Tick == 4 {
			found = true

			assert.Greater(t, a.ZScore, 2.0)
			assert.InDelta(t, 100.0, a.RawValue, 0.001)
		}
	}

	assert.True(t, found, "expected anomaly at tick 4")

	extSummaries, ok := anomalyReport["external_summaries"].([]ExternalSummary)
	require.True(t, ok)
	require.Len(t, extSummaries, 1)
	assert.Equal(t, "test-source", extSummaries[0].Source)
	assert.Equal(t, "metric_a", extSummaries[0].Dimension)
	assert.Positive(t, extSummaries[0].Anomalies)
}

func TestEnrichFromReports_NoMatchingExtractors(t *testing.T) {
	t.Parallel()
	withIsolatedRegistry(t)

	anomalyReport := analyze.Report{
		"anomalies": []Record{},
	}

	otherReports := map[string]analyze.Report{
		"unknown": {},
	}

	EnrichFromReports(anomalyReport, otherReports, 20, 2.0)

	extAnomalies, ok := anomalyReport["external_anomalies"].([]ExternalAnomaly)
	require.True(t, ok)
	assert.Empty(t, extAnomalies)
}

func TestEnrichFromReports_ExtractorReturnsNil(t *testing.T) {
	t.Parallel()
	withIsolatedRegistry(t)

	RegisterTimeSeriesExtractor("empty-source", func(_ analyze.Report) ([]int, map[string][]float64) {
		return nil, nil
	})

	anomalyReport := analyze.Report{}

	otherReports := map[string]analyze.Report{
		"empty-source": {},
	}

	EnrichFromReports(anomalyReport, otherReports, 20, 2.0)

	extAnomalies, ok := anomalyReport["external_anomalies"].([]ExternalAnomaly)
	require.True(t, ok)
	assert.Empty(t, extAnomalies)
}

func TestEnrichFromReports_MultipleDimensions(t *testing.T) {
	t.Parallel()
	withIsolatedRegistry(t)

	RegisterTimeSeriesExtractor("multi-dim", func(_ analyze.Report) ([]int, map[string][]float64) {
		return []int{0, 1, 2, 3, 4}, map[string][]float64{
			"dim_a": {1.0, 1.0, 1.0, 1.0, 1.0},  // Stable -- no anomalies.
			"dim_b": {1.0, 1.0, 1.0, 1.0, 50.0}, // Spike at tick 4.
		}
	})

	anomalyReport := analyze.Report{}

	otherReports := map[string]analyze.Report{
		"multi-dim": {},
	}

	EnrichFromReports(anomalyReport, otherReports, 3, 2.0)

	extSummaries, ok := anomalyReport["external_summaries"].([]ExternalSummary)
	require.True(t, ok)
	require.Len(t, extSummaries, 2)

	// Summaries are sorted by dimension name.
	assert.Equal(t, "dim_a", extSummaries[0].Dimension)
	assert.Equal(t, 0, extSummaries[0].Anomalies)

	assert.Equal(t, "dim_b", extSummaries[1].Dimension)
	assert.Positive(t, extSummaries[1].Anomalies)
}

func TestEnrichFromReports_NoAnomalies(t *testing.T) {
	t.Parallel()
	withIsolatedRegistry(t)

	// All values are identical -- no anomalies possible.
	RegisterTimeSeriesExtractor("stable", func(_ analyze.Report) ([]int, map[string][]float64) {
		return []int{0, 1, 2, 3, 4}, map[string][]float64{
			"stable_metric": {5.0, 5.0, 5.0, 5.0, 5.0},
		}
	})

	anomalyReport := analyze.Report{}

	otherReports := map[string]analyze.Report{
		"stable": {},
	}

	EnrichFromReports(anomalyReport, otherReports, 3, 2.0)

	extAnomalies, ok := anomalyReport["external_anomalies"].([]ExternalAnomaly)
	require.True(t, ok)
	assert.Empty(t, extAnomalies)

	extSummaries, ok := anomalyReport["external_summaries"].([]ExternalSummary)
	require.True(t, ok)
	require.Len(t, extSummaries, 1)
	assert.Equal(t, 0, extSummaries[0].Anomalies)
	assert.InDelta(t, 0.0, extSummaries[0].HighestZ, 0.001)
}

func TestDetectExternalAnomalies(t *testing.T) {
	t.Parallel()

	ticks := []int{0, 1, 2, 3, 4}
	dimensions := map[string][]float64{
		"metric": {1.0, 1.0, 1.0, 1.0, 100.0},
	}

	anomalies, summaries := detectExternalAnomalies("src", ticks, dimensions, 3, 2.0)

	assert.NotEmpty(t, anomalies)
	require.Len(t, summaries, 1)
	assert.Equal(t, "src", summaries[0].Source)
	assert.Equal(t, "metric", summaries[0].Dimension)
	assert.Greater(t, summaries[0].HighestZ, 2.0)
}

func TestDetectExternalAnomalies_MismatchedLengths(t *testing.T) {
	t.Parallel()

	ticks := []int{0, 1, 2}
	dimensions := map[string][]float64{
		"bad_dim": {1.0, 2.0}, // Length mismatch -- should be skipped.
	}

	anomalies, summaries := detectExternalAnomalies("src", ticks, dimensions, 3, 2.0)

	assert.Empty(t, anomalies)
	assert.Empty(t, summaries)
}
