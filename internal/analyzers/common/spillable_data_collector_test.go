package common

import (
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
)

const (
	testSpillThreshold = 5
	testSpillZero      = 0
)

func TestSpillableDataCollector_NoSpill_CollectsLikeDataCollector(t *testing.T) {
	t.Parallel()

	sdc := NewSpillableDataCollector("items", "name", testSpillZero)

	report := analyze.Report{
		"items": []map[string]any{
			{"name": "alpha", "value": 1},
			{"name": "beta", "value": 2},
		},
	}

	sdc.CollectFromReport(report)

	assert.Equal(t, 2, sdc.GetDataCount())
	assert.Equal(t, "items", sdc.GetCollectionKey())
	assert.Equal(t, "name", sdc.GetIdentifierKey())

	data := sdc.GetSortedData()
	require.Len(t, data, 2)
	assert.Equal(t, "alpha", data[0]["name"])
	assert.Equal(t, "beta", data[1]["name"])
}

func TestSpillableDataCollector_Deduplication(t *testing.T) {
	t.Parallel()

	sdc := NewSpillableDataCollector("items", "name", testSpillZero)

	report1 := analyze.Report{
		"items": []map[string]any{
			{"name": "alpha", "value": 1},
		},
	}
	report2 := analyze.Report{
		"items": []map[string]any{
			{"name": "alpha", "value": 2},
		},
	}

	sdc.CollectFromReport(report1)
	sdc.CollectFromReport(report2)

	assert.Equal(t, 1, sdc.GetDataCount())

	data := sdc.GetSortedData()
	require.Len(t, data, 1)
	assert.Equal(t, 2, data[0]["value"])
}

func TestSpillableDataCollector_SummaryOnly_IsNoOp(t *testing.T) {
	t.Parallel()

	sdc := NewSpillableDataCollector("items", "name", testSpillZero)
	sdc.SetAggregationMode(analyze.AggregationModeSummaryOnly)

	report := analyze.Report{
		"items": []map[string]any{
			{"name": "alpha", "value": 1},
		},
	}

	sdc.CollectFromReport(report)

	assert.Equal(t, 0, sdc.GetDataCount())
	assert.Empty(t, sdc.GetSortedData())
}

func TestSpillableDataCollector_SpillsAtThreshold(t *testing.T) {
	t.Parallel()

	sdc := NewSpillableDataCollector("items", "name", testSpillThreshold)

	// Feed exactly threshold items in one report.
	items := make([]map[string]any, testSpillThreshold)
	for i := range testSpillThreshold {
		items[i] = map[string]any{
			"name":  makeItemName(i),
			"value": i,
		}
	}

	sdc.CollectFromReport(analyze.Report{"items": items})

	// Should have spilled: in-memory count is 0.
	assert.Equal(t, 0, sdc.GetDataCount())
	assert.Equal(t, 1, sdc.SpillCount())

	// GetSortedData merges spilled data.
	data := sdc.GetSortedData()
	require.Len(t, data, testSpillThreshold)
}

func TestSpillableDataCollector_MultipleSpills_CorrectMerge(t *testing.T) {
	t.Parallel()

	const threshold = 3

	sdc := NewSpillableDataCollector("items", "name", threshold)

	// Feed 7 items across multiple reports to trigger multiple spills.
	for i := range 7 {
		report := analyze.Report{
			"items": []map[string]any{
				{"name": makeItemName(i), "value": i},
			},
		}

		sdc.CollectFromReport(report)
	}

	// Should have spilled at least twice (7 items, threshold 3).
	assert.GreaterOrEqual(t, sdc.SpillCount(), 2)

	data := sdc.GetSortedData()
	require.Len(t, data, 7)

	// Verify sorted order.
	for i := 1; i < len(data); i++ {
		nameI, okI := data[i-1]["name"].(string)
		nameJ, okJ := data[i]["name"].(string)

		require.True(t, okI)
		require.True(t, okJ)
		assert.LessOrEqual(t, nameI, nameJ)
	}
}

func TestSpillableDataCollector_SpillDeduplication(t *testing.T) {
	t.Parallel()

	const threshold = 2

	sdc := NewSpillableDataCollector("items", "name", threshold)

	// First batch: 2 items → spill.
	sdc.CollectFromReport(analyze.Report{
		"items": []map[string]any{
			{"name": "alpha", "value": 1},
			{"name": "beta", "value": 2},
		},
	})

	assert.Equal(t, 1, sdc.SpillCount())

	// Second batch: same key "alpha" with new value.
	sdc.CollectFromReport(analyze.Report{
		"items": []map[string]any{
			{"name": "alpha", "value": 99},
		},
	})

	data := sdc.GetSortedData()
	require.Len(t, data, 2)

	// Last-write-wins: alpha should have value 99.
	assert.Equal(t, 99, data[0]["value"])
}

func TestSpillableDataCollector_CleanupRemovesTempFiles(t *testing.T) {
	t.Parallel()

	sdc := NewSpillableDataCollector("items", "name", testSpillThreshold)

	items := make([]map[string]any, testSpillThreshold)
	for i := range testSpillThreshold {
		items[i] = map[string]any{
			"name":  makeItemName(i),
			"value": i,
		}
	}

	sdc.CollectFromReport(analyze.Report{"items": items})
	require.Equal(t, 1, sdc.SpillCount())

	dir := sdc.SpillDir()
	assert.DirExists(t, dir)

	sdc.Cleanup()
	assert.NoDirExists(t, dir)
}

func TestSpillableDataCollector_GetSortedData_CleansUp(t *testing.T) {
	t.Parallel()

	sdc := NewSpillableDataCollector("items", "name", testSpillThreshold)

	items := make([]map[string]any, testSpillThreshold)
	for i := range testSpillThreshold {
		items[i] = map[string]any{
			"name":  makeItemName(i),
			"value": i,
		}
	}

	sdc.CollectFromReport(analyze.Report{"items": items})

	dir := sdc.SpillDir()
	assert.DirExists(t, dir)

	_ = sdc.GetSortedData()

	// After GetSortedData, spill files should be cleaned up.
	assert.NoDirExists(t, dir)
	assert.Equal(t, 0, sdc.SpillCount())
}

func TestSpillableDataCollector_EmptyReport(t *testing.T) {
	t.Parallel()

	sdc := NewSpillableDataCollector("items", "name", testSpillThreshold)

	sdc.CollectFromReport(analyze.Report{"other_key": 42})

	assert.Equal(t, 0, sdc.GetDataCount())
	assert.Empty(t, sdc.GetSortedData())
}

func TestSpillableDataCollector_MissingIdentifier(t *testing.T) {
	t.Parallel()

	sdc := NewSpillableDataCollector("items", "name", testSpillZero)

	report := analyze.Report{
		"items": []map[string]any{
			{"no_name": "foo", "value": 1},
		},
	}

	sdc.CollectFromReport(report)

	// Items without identifier key are skipped.
	assert.Equal(t, 0, sdc.GetDataCount())
}

func TestSpillableDataCollector_NoSpillMatchesSpill(t *testing.T) {
	t.Parallel()

	// Feed identical data to both collectors (no-spill vs spill) and verify same output.
	noSpill := NewSpillableDataCollector("funcs", "name", testSpillZero)
	withSpill := NewSpillableDataCollector("funcs", "name", 2)

	reports := []analyze.Report{
		{
			"funcs": []map[string]any{
				{"name": "c_func", "score": 3},
				{"name": "a_func", "score": 1},
			},
		},
		{
			"funcs": []map[string]any{
				{"name": "b_func", "score": 2},
				{"name": "a_func", "score": 99},
			},
		},
	}

	for _, r := range reports {
		noSpill.CollectFromReport(r)
		withSpill.CollectFromReport(r)
	}

	noSpillData := noSpill.GetSortedData()
	withSpillData := withSpill.GetSortedData()

	assert.Equal(t, noSpillData, withSpillData)
}

func TestSpillableDataCollector_CompositeKeys_PreventsCrossFileOverwrite(t *testing.T) {
	t.Parallel()

	sdc := NewSpillableDataCollectorComposite("functions", []string{"_source_file", "name"}, testSpillZero)

	// Two files, each with a function named "init".
	report1 := analyze.Report{
		"functions": []map[string]any{
			{"name": "init", "_source_file": "pkg/foo.go", "volume": 100.0},
		},
	}
	report2 := analyze.Report{
		"functions": []map[string]any{
			{"name": "init", "_source_file": "pkg/bar.go", "volume": 200.0},
		},
	}

	sdc.CollectFromReport(report1)
	sdc.CollectFromReport(report2)

	data := sdc.GetSortedData()
	require.Len(t, data, 2, "composite key should prevent overwrite")
}

func TestSpillableDataCollector_CompositeKeys_DedupsWithinSameFile(t *testing.T) {
	t.Parallel()

	sdc := NewSpillableDataCollectorComposite("functions", []string{"_source_file", "name"}, testSpillZero)

	// Same file, same function name → should dedup (last-write-wins).
	report1 := analyze.Report{
		"functions": []map[string]any{
			{"name": "init", "_source_file": "pkg/foo.go", "volume": 100.0},
		},
	}
	report2 := analyze.Report{
		"functions": []map[string]any{
			{"name": "init", "_source_file": "pkg/foo.go", "volume": 999.0},
		},
	}

	sdc.CollectFromReport(report1)
	sdc.CollectFromReport(report2)

	data := sdc.GetSortedData()
	require.Len(t, data, 1, "same composite key should dedup")
	assert.InDelta(t, 999.0, data[0]["volume"], 0.001)
}

func TestSpillableDataCollector_CompositeKeys_MissingOptionalKey_StillCollects(t *testing.T) {
	t.Parallel()

	sdc := NewSpillableDataCollectorComposite("functions", []string{"_source_file", "name"}, testSpillZero)

	// Item missing "_source_file" (optional prefix key) → still collected using just "name".
	report := analyze.Report{
		"functions": []map[string]any{
			{"name": "init", "volume": 100.0},
		},
	}

	sdc.CollectFromReport(report)

	assert.Equal(t, 1, sdc.GetDataCount())
}

func TestSpillableDataCollector_CompositeKeys_MissingRequiredKey_Skipped(t *testing.T) {
	t.Parallel()

	sdc := NewSpillableDataCollectorComposite("functions", []string{"_source_file", "name"}, testSpillZero)

	// Item missing "name" (last/required key) → should be skipped.
	report := analyze.Report{
		"functions": []map[string]any{
			{"_source_file": "pkg/foo.go", "volume": 100.0},
		},
	}

	sdc.CollectFromReport(report)

	assert.Equal(t, 0, sdc.GetDataCount())
}

func TestSpillableDataCollector_CompositeKeys_WithSpill(t *testing.T) {
	t.Parallel()

	const threshold = 2

	sdc := NewSpillableDataCollectorComposite("functions", []string{"_source_file", "name"}, threshold)

	// 4 items across 4 files → should trigger spill and preserve all.
	for i := range 4 {
		sdc.CollectFromReport(analyze.Report{
			"functions": []map[string]any{
				{"name": "init", "_source_file": makeItemName(i), "value": i},
			},
		})
	}

	data := sdc.GetSortedData()
	require.Len(t, data, 4)
}

func TestSpillableDataCollector_CompositeKeys_GetIdentifierKey(t *testing.T) {
	t.Parallel()

	sdc := NewSpillableDataCollectorComposite("functions", []string{"_source_file", "name"}, testSpillZero)

	// GetIdentifierKey returns the last key (the primary sort key).
	assert.Equal(t, "name", sdc.GetIdentifierKey())
}

func TestSpillableDataCollector_EstimatedBufferBytes_Empty(t *testing.T) {
	t.Parallel()

	sdc := NewSpillableDataCollector("items", "name", testSpillZero)

	assert.Equal(t, int64(0), sdc.EstimatedBufferBytes())
}

func TestSpillableDataCollector_EstimatedBufferBytes_WithItems(t *testing.T) {
	t.Parallel()

	sdc := NewSpillableDataCollector("items", "name", testSpillZero)

	report := analyze.Report{
		"items": []map[string]any{
			{"name": "alpha", "value": 1},
			{"name": "beta", "value": 2},
		},
	}

	sdc.CollectFromReport(report)

	estimated := sdc.EstimatedBufferBytes()
	assert.Equal(t, int64(2)*estimatedItemBytes, estimated)
}

func TestSpillableDataCollector_EstimatedBufferBytes_AfterSpill(t *testing.T) {
	t.Parallel()

	sdc := NewSpillableDataCollector("items", "name", testSpillThreshold)

	items := make([]map[string]any, testSpillThreshold)
	for i := range testSpillThreshold {
		items[i] = map[string]any{
			"name":  makeItemName(i),
			"value": i,
		}
	}

	sdc.CollectFromReport(analyze.Report{"items": items})

	// After spill, in-memory buffer is empty.
	assert.Equal(t, int64(0), sdc.EstimatedBufferBytes())
}

// makeItemName generates a sortable item name from an index.
func makeItemName(index int) string {
	return "item_" + string(rune('A'+index))
}
