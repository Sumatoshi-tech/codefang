package burndown

import (
	"bytes"
	"encoding/json"
	"testing"
	"time"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
)

func TestHistoryAnalyzer_Name(t *testing.T) {
	t.Parallel()

	b := NewHistoryAnalyzer()
	if b.Name() == "" {
		t.Error("Name empty")
	}
}

func TestHistoryAnalyzer_Flag(t *testing.T) {
	t.Parallel()

	b := NewHistoryAnalyzer()
	if b.Flag() == "" {
		t.Error("Flag empty")
	}
}

func TestHistoryAnalyzer_Description(t *testing.T) {
	t.Parallel()

	b := NewHistoryAnalyzer()
	if b.Description() == "" {
		t.Error("Description empty")
	}
}

func TestHistoryAnalyzer_ListConfigurationOptions(t *testing.T) {
	t.Parallel()

	b := NewHistoryAnalyzer()

	opts := b.ListConfigurationOptions()
	if len(opts) == 0 {
		t.Error("expected options")
	}
}

func TestHistoryAnalyzer_Configure(t *testing.T) {
	t.Parallel()

	b := NewHistoryAnalyzer()
	err := b.Configure(nil)
	require.NoError(t, err)
}

func TestHistoryAnalyzer_Initialize(t *testing.T) {
	t.Parallel()

	b := NewHistoryAnalyzer()
	b.Granularity = 30
	b.Sampling = 30
	b.Goroutines = 4

	err := b.Initialize(nil)
	require.NoError(t, err)
}

func TestHistoryAnalyzer_Serialize_JSON_UsesComputedMetrics(t *testing.T) {
	t.Parallel()

	b := NewHistoryAnalyzer()

	report := analyze.Report{
		"GlobalHistory":      DenseHistory{{100, 200}, {150, 180}},
		"FileHistories":      map[string]DenseHistory{"main.go": {{50, 100}}},
		"FileOwnership":      map[string]map[int]int{"main.go": {0: 100}},
		"PeopleHistories":    []DenseHistory{{{100, 200}}},
		"ReversedPeopleDict": []string{"Alice"},
		"TickSize":           24 * time.Hour,
		"Sampling":           30,
	}

	var buf bytes.Buffer

	err := b.Serialize(report, analyze.FormatJSON, &buf)
	require.NoError(t, err)

	var result map[string]any

	err = json.Unmarshal(buf.Bytes(), &result)
	require.NoError(t, err)

	// Should have computed metrics structure.
	assert.Contains(t, result, "aggregate")
	assert.Contains(t, result, "global_survival")
	assert.Contains(t, result, "file_survival")
	assert.Contains(t, result, "developer_survival")
	assert.Contains(t, result, "interactions")
}

func TestHistoryAnalyzer_Serialize_YAML_UsesComputedMetrics(t *testing.T) {
	t.Parallel()

	b := NewHistoryAnalyzer()

	report := analyze.Report{
		"GlobalHistory":      DenseHistory{{100, 200}, {150, 180}},
		"FileHistories":      map[string]DenseHistory{"main.go": {{50, 100}}},
		"FileOwnership":      map[string]map[int]int{"main.go": {0: 100}},
		"PeopleHistories":    []DenseHistory{{{100, 200}}},
		"ReversedPeopleDict": []string{"Alice"},
		"TickSize":           24 * time.Hour,
		"Sampling":           30,
	}

	var buf bytes.Buffer

	err := b.Serialize(report, analyze.FormatYAML, &buf)
	require.NoError(t, err)

	output := buf.String()
	// Should have computed metrics structure (YAML keys).
	assert.Contains(t, output, "aggregate:")
	assert.Contains(t, output, "global_survival:")
	assert.Contains(t, output, "file_survival:")
	assert.Contains(t, output, "developer_survival:")
	assert.Contains(t, output, "interactions:")
}

// --- Fork/Merge Tests ---.

func TestHistoryAnalyzer_Fork_CreatesIndependentCopies(t *testing.T) {
	t.Parallel()

	b := NewHistoryAnalyzer()
	b.Granularity = DefaultBurndownGranularity
	b.Sampling = DefaultBurndownSampling
	b.Goroutines = 4
	b.PeopleNumber = 2

	err := b.Initialize(nil)
	require.NoError(t, err)

	const forkCount = 3

	forks := b.Fork(forkCount)
	require.Len(t, forks, forkCount)

	// Each fork should be independent.
	for i, fork := range forks {
		analyzer, ok := fork.(*HistoryAnalyzer)
		require.True(t, ok, "fork %d should be *HistoryAnalyzer", i)

		// Should be different pointer.
		require.NotSame(t, b, analyzer, "fork %d should not be same pointer as parent", i)

		// Config should be copied.
		require.Equal(t, b.Granularity, analyzer.Granularity)
		require.Equal(t, b.Sampling, analyzer.Sampling)
		require.Equal(t, b.Goroutines, analyzer.Goroutines)
		require.Equal(t, b.PeopleNumber, analyzer.PeopleNumber)

		// Should have shards.
		require.NotNil(t, analyzer.shards, "fork %d should have shards", i)
		require.Len(t, analyzer.shards, b.Goroutines)

		// PathInterner should be shared (same instance).
		require.Same(t, b.pathInterner, analyzer.pathInterner,
			"fork %d should share pathInterner with parent", i)
	}
}

func TestHistoryAnalyzer_Fork_IndependentShards(t *testing.T) {
	t.Parallel()

	b := NewHistoryAnalyzer()
	b.Granularity = DefaultBurndownGranularity
	b.Sampling = DefaultBurndownSampling
	b.Goroutines = 2

	err := b.Initialize(nil)
	require.NoError(t, err)

	forks := b.Fork(2)

	fork1, ok := forks[0].(*HistoryAnalyzer)
	require.True(t, ok)

	fork2, ok := forks[1].(*HistoryAnalyzer)
	require.True(t, ok)

	// Modify fork1's shard tracking maps — they should be independent.
	fork1.shards[0].mergedByID[PathID(42)] = true

	// fork2's shard should be unaffected.
	require.Empty(t, fork2.shards[0].mergedByID,
		"fork2 shard should be independent of fork1")

	// Parent's shard should be unaffected.
	require.NotContains(t, b.shards[0].mergedByID, PathID(42),
		"parent shard should be independent of forks")
}

func TestHistoryAnalyzer_Fork_IndependentRenames(t *testing.T) {
	t.Parallel()

	b := NewHistoryAnalyzer()
	b.Granularity = DefaultBurndownGranularity
	b.Sampling = DefaultBurndownSampling
	b.Goroutines = 2

	err := b.Initialize(nil)
	require.NoError(t, err)

	// Set up some renames in parent.
	b.renames["old.go"] = "new.go"

	forks := b.Fork(2)

	fork1, ok := forks[0].(*HistoryAnalyzer)
	require.True(t, ok)

	// Fork should have empty renames (fresh start).
	require.Empty(t, fork1.renames, "fork should start with empty renames")
}

func TestHistoryAnalyzer_Merge_CombinesRenames(t *testing.T) {
	t.Parallel()

	b := NewHistoryAnalyzer()
	b.Granularity = DefaultBurndownGranularity
	b.Sampling = DefaultBurndownSampling
	b.Goroutines = 2

	err := b.Initialize(nil)
	require.NoError(t, err)

	branch1 := NewHistoryAnalyzer()
	branch1.Granularity = DefaultBurndownGranularity
	branch1.Sampling = DefaultBurndownSampling
	branch1.Goroutines = 2
	branch1.renames = map[string]string{"a.go": "b.go"}

	branch2 := NewHistoryAnalyzer()
	branch2.Granularity = DefaultBurndownGranularity
	branch2.Sampling = DefaultBurndownSampling
	branch2.Goroutines = 2
	branch2.renames = map[string]string{"c.go": "d.go"}

	b.Merge([]analyze.HistoryAnalyzer{branch1, branch2})

	// Both renames should be present.
	require.Equal(t, "b.go", b.renames["a.go"])
	require.Equal(t, "d.go", b.renames["c.go"])
}

func TestHistoryAnalyzer_Merge_HandlesNilBranches(t *testing.T) {
	t.Parallel()

	b := NewHistoryAnalyzer()
	b.Granularity = DefaultBurndownGranularity
	b.Sampling = DefaultBurndownSampling
	b.Goroutines = 2

	err := b.Initialize(nil)
	require.NoError(t, err)

	// Merge with nil slice should not panic.
	b.Merge(nil)
}

func TestHistoryAnalyzer_Merge_TakeMaxTick(t *testing.T) {
	t.Parallel()

	b := NewHistoryAnalyzer()
	b.Granularity = DefaultBurndownGranularity
	b.Sampling = DefaultBurndownSampling
	b.Goroutines = 2

	err := b.Initialize(nil)
	require.NoError(t, err)

	branch1 := NewHistoryAnalyzer()
	branch1.Granularity = DefaultBurndownGranularity
	branch1.Sampling = DefaultBurndownSampling
	branch1.Goroutines = 2

	err = branch1.Initialize(nil)
	require.NoError(t, err)

	branch1.tick = 5

	branch2 := NewHistoryAnalyzer()
	branch2.Granularity = DefaultBurndownGranularity
	branch2.Sampling = DefaultBurndownSampling
	branch2.Goroutines = 2

	err = branch2.Initialize(nil)
	require.NoError(t, err)

	branch2.tick = 10

	b.Merge([]analyze.HistoryAnalyzer{branch1, branch2})

	require.Equal(t, 10, b.tick, "tick should be max of all branches")
}

func TestHistoryAnalyzer_NewAggregator_ReturnsAggregator(t *testing.T) {
	t.Parallel()

	b := NewHistoryAnalyzer()
	b.Granularity = DefaultBurndownGranularity
	b.Sampling = DefaultBurndownSampling
	b.PeopleNumber = 2

	opts := analyze.AggregatorOptions{
		SpillBudget: 1024 * 1024,
		SpillDir:    t.TempDir(),
		Sampling:    b.Sampling,
		Granularity: b.Granularity,
	}

	agg := b.NewAggregator(opts)
	assert.NotNil(t, agg)
}

// --- Delta Buffer Tests ---.

func TestResetDeltaBuffers_ClearsState(t *testing.T) {
	t.Parallel()

	b := NewHistoryAnalyzer()
	b.Granularity = DefaultBurndownGranularity
	b.Sampling = DefaultBurndownSampling
	b.Goroutines = 2
	b.PeopleNumber = 1
	b.TrackFiles = true

	err := b.Initialize(nil)
	require.NoError(t, err)

	// Populate delta buffers manually.
	b.shards[0].deltas.globalDeltas = sparseHistory{1: {0: 100}}
	b.shards[0].deltas.peopleDeltas = map[int]sparseHistory{0: {1: {0: 50}}}
	b.shards[0].deltas.matrixDeltas = []map[int]int64{{1: 10}}
	b.shards[0].deltas.fileDeltas = map[PathID]sparseHistory{0: {1: {0: 20}}}

	b.resetDeltaBuffers()

	// All delta buffers should be empty.
	for i, shard := range b.shards {
		assert.Empty(t, shard.deltas.globalDeltas, "shard %d globalDeltas", i)
		assert.Empty(t, shard.deltas.peopleDeltas, "shard %d peopleDeltas", i)
		assert.Nil(t, shard.deltas.matrixDeltas, "shard %d matrixDeltas", i)
		assert.Empty(t, shard.deltas.fileDeltas, "shard %d fileDeltas", i)
	}
}

func TestCollectDeltas_EmptyShards(t *testing.T) {
	t.Parallel()

	b := NewHistoryAnalyzer()
	b.Granularity = DefaultBurndownGranularity
	b.Sampling = DefaultBurndownSampling
	b.Goroutines = 2

	err := b.Initialize(nil)
	require.NoError(t, err)

	b.resetDeltaBuffers()

	result := b.collectDeltas()

	assert.NotNil(t, result)
	assert.Empty(t, result.GlobalDeltas)
	assert.Nil(t, result.PeopleDeltas)
	assert.Nil(t, result.MatrixDeltas)
	assert.Nil(t, result.FileDeltas)
}

func TestCollectDeltas_MergesAllShards(t *testing.T) {
	t.Parallel()

	b := NewHistoryAnalyzer()
	b.Granularity = DefaultBurndownGranularity
	b.Sampling = DefaultBurndownSampling
	b.Goroutines = 2
	b.PeopleNumber = 1
	b.TrackFiles = true

	err := b.Initialize(nil)
	require.NoError(t, err)

	b.resetDeltaBuffers()

	// Add deltas to shard 0.
	b.shards[0].deltas.globalDeltas[1] = map[int]int64{0: 100}
	b.shards[0].deltas.peopleDeltas[0] = sparseHistory{1: {0: 50}}
	b.shards[0].deltas.matrixDeltas = []map[int]int64{{1: 10}}
	b.shards[0].deltas.fileDeltas[PathID(0)] = sparseHistory{1: {0: 20}}

	// Add deltas to shard 1.
	b.shards[1].deltas.globalDeltas[1] = map[int]int64{0: 200}
	b.shards[1].deltas.peopleDeltas[0] = sparseHistory{1: {0: 30}}
	b.shards[1].deltas.matrixDeltas = []map[int]int64{{1: 5}}
	b.shards[1].deltas.fileDeltas[PathID(1)] = sparseHistory{1: {0: 15}}

	result := b.collectDeltas()

	// Global deltas should be merged (100 + 200).
	assert.Equal(t, int64(300), result.GlobalDeltas[1][0])

	// People deltas should be merged (50 + 30).
	assert.Equal(t, int64(80), result.PeopleDeltas[0][1][0])

	// Matrix deltas should be merged (10 + 5).
	assert.Equal(t, int64(15), result.MatrixDeltas[0][1])

	// File deltas should have both files.
	assert.Equal(t, int64(20), result.FileDeltas[PathID(0)][1][0])
	assert.Equal(t, int64(15), result.FileDeltas[PathID(1)][1][0])
}
