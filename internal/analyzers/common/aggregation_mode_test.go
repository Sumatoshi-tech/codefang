package common

// FRD: specs/frds/FRD-20260311-summary-only-aggregation.md.

import (
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
)

func TestAggregationMode_ZeroValueIsFull(t *testing.T) {
	t.Parallel()

	var mode analyze.AggregationMode
	assert.Equal(t, analyze.AggregationModeFull, mode)
}

func TestSpillableCollector_SummaryOnly_CollectIsNoOp(t *testing.T) {
	t.Parallel()

	sdc := NewSpillableDataCollector("functions", "name", testSpillZero)
	sdc.SetAggregationMode(analyze.AggregationModeSummaryOnly)

	report := analyze.Report{
		"functions": []map[string]any{
			{"name": "func1", "complexity": 5},
			{"name": "func2", "complexity": 10},
		},
	}

	sdc.CollectFromReport(report)

	assert.Zero(t, sdc.GetDataCount())
	assert.Empty(t, sdc.GetSortedData())
}

func TestSpillableCollector_Full_CollectsNormally(t *testing.T) {
	t.Parallel()

	sdc := NewSpillableDataCollector("functions", "name", testSpillZero)
	sdc.SetAggregationMode(analyze.AggregationModeFull)

	report := analyze.Report{
		"functions": []map[string]any{
			{"name": "func1", "complexity": 5},
		},
	}

	sdc.CollectFromReport(report)

	assert.Equal(t, 1, sdc.GetDataCount())
}

func TestDetailedDataCollector_SummaryOnly_CollectIsNoOp(t *testing.T) {
	t.Parallel()

	ddc := NewDetailedDataCollector("functions")
	ddc.SetAggregationMode(analyze.AggregationModeSummaryOnly)

	results := map[string]analyze.Report{
		"file1": {
			"functions": []map[string]any{
				{"name": "func1"},
				{"name": "func2"},
			},
		},
	}

	ddc.CollectFromReports(results)

	assert.Empty(t, ddc.collections["functions"])
}

func TestDetailedDataCollector_Full_CollectsNormally(t *testing.T) {
	t.Parallel()

	ddc := NewDetailedDataCollector("functions")
	ddc.SetAggregationMode(analyze.AggregationModeFull)

	results := map[string]analyze.Report{
		"file1": {
			"functions": []map[string]any{
				{"name": "func1"},
			},
		},
	}

	ddc.CollectFromReports(results)

	assert.Len(t, ddc.collections["functions"], 1)
}

func TestAggregator_SetAggregationMode_PropagatesToCollectors(t *testing.T) {
	t.Parallel()

	agg := NewAggregator(
		"test",
		[]string{"score"}, []string{"count"},
		"items", []string{"name"},
		nil, nil,
	)

	agg.SetAggregationMode(analyze.AggregationModeSummaryOnly)

	// Feed data — should not collect per-item data.
	report := analyze.Report{
		"score": 3.5,
		"count": 10,
		"items": []map[string]any{
			{"name": "item1", "value": 100},
			{"name": "item2", "value": 200},
		},
	}

	agg.Aggregate(map[string]analyze.Report{"test": report})

	// Metrics should still be processed.
	assert.Equal(t, 1, agg.GetMetricsProcessor().GetReportCount())

	// Per-item data should be empty.
	assert.Zero(t, agg.GetDataCollector().GetDataCount())
}

func TestAggregator_ImplementsAggregationModeAware(t *testing.T) {
	t.Parallel()

	agg := NewAggregator("test", nil, nil, "items", []string{"name"}, nil, nil)

	var aware analyze.AggregationModeAware = agg
	require.NotNil(t, aware)
}

// FRD: specs/frds/FRD-20260312-static-rss-logging.md.

func TestAggregator_EstimatedStateSize_Empty(t *testing.T) {
	t.Parallel()

	agg := NewAggregator("test", []string{"score"}, []string{"count"}, "items", []string{"name"}, nil, nil)

	assert.Equal(t, int64(0), agg.EstimatedStateSize())
}

func TestAggregator_EstimatedStateSize_WithData(t *testing.T) {
	t.Parallel()

	agg := NewAggregator("test", []string{"score"}, []string{"count"}, "items", []string{"name"}, nil, nil)

	report := analyze.Report{
		"score": 5.0,
		"count": 3,
		"items": []map[string]any{
			{"name": "a", "v": 1},
			{"name": "b", "v": 2},
		},
	}

	agg.Aggregate(map[string]analyze.Report{"test": report})

	estimated := agg.EstimatedStateSize()

	// Metrics: 1 numeric + 1 count = 2 entries × metricsEntryBytes.
	// Data: 2 items × estimatedItemBytes.
	expectedMetrics := int64(2) * metricsEntryBytes
	expectedData := int64(2) * estimatedItemBytes

	assert.Equal(t, expectedMetrics+expectedData, estimated)
}

func TestAggregator_ImplementsStateSizer(t *testing.T) {
	t.Parallel()

	agg := NewAggregator("test", nil, nil, "items", []string{"name"}, nil, nil)

	var sizer analyze.StateSizer = agg
	require.NotNil(t, sizer)
}

func TestAggregator_SummaryOnly_GetResult_EmptyCollection(t *testing.T) {
	t.Parallel()

	agg := NewAggregator(
		"test",
		[]string{"score"}, []string{"count"},
		"items", []string{"name"},
		func(_ float64) string { return "ok" }, nil,
	)

	agg.SetAggregationMode(analyze.AggregationModeSummaryOnly)

	report := analyze.Report{
		"score": 5.0,
		"count": 3,
		"items": []map[string]any{
			{"name": "a", "v": 1},
			{"name": "b", "v": 2},
		},
	}

	agg.Aggregate(map[string]analyze.Report{"test": report})

	result := agg.GetResult()

	// Metrics present.
	assert.Contains(t, result, "score")
	assert.Contains(t, result, "count")

	// Collection key present but empty.
	items, ok := result["items"].([]map[string]any)
	assert.True(t, ok)
	assert.Empty(t, items)
}
