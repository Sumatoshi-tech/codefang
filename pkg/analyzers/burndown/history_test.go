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

	b := &HistoryAnalyzer{}
	if b.Name() == "" {
		t.Error("Name empty")
	}
}

func TestHistoryAnalyzer_Flag(t *testing.T) {
	t.Parallel()

	b := &HistoryAnalyzer{}
	if b.Flag() == "" {
		t.Error("Flag empty")
	}
}

func TestHistoryAnalyzer_Description(t *testing.T) {
	t.Parallel()

	b := &HistoryAnalyzer{}
	if b.Description() == "" {
		t.Error("Description empty")
	}
}

func TestHistoryAnalyzer_ListConfigurationOptions(t *testing.T) {
	t.Parallel()

	b := &HistoryAnalyzer{}

	opts := b.ListConfigurationOptions()
	if len(opts) == 0 {
		t.Error("expected options")
	}
}

func TestHistoryAnalyzer_Configure(t *testing.T) {
	t.Parallel()

	b := &HistoryAnalyzer{}
	err := b.Configure(nil)
	require.NoError(t, err)
}

func TestHistoryAnalyzer_Initialize(t *testing.T) {
	t.Parallel()

	b := &HistoryAnalyzer{
		Granularity: 30,
		Sampling:    30,
		Goroutines:  4,
	}
	err := b.Initialize(nil)
	require.NoError(t, err)
}

func TestHistoryAnalyzer_Serialize_JSON_UsesComputedMetrics(t *testing.T) {
	t.Parallel()

	b := &HistoryAnalyzer{}

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

	b := &HistoryAnalyzer{}

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

	b := &HistoryAnalyzer{
		Granularity:  DefaultBurndownGranularity,
		Sampling:     DefaultBurndownSampling,
		Goroutines:   4,
		PeopleNumber: 2,
	}
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

	b := &HistoryAnalyzer{
		Granularity: DefaultBurndownGranularity,
		Sampling:    DefaultBurndownSampling,
		Goroutines:  2,
	}
	err := b.Initialize(nil)
	require.NoError(t, err)

	forks := b.Fork(2)

	fork1, ok := forks[0].(*HistoryAnalyzer)
	require.True(t, ok)

	fork2, ok := forks[1].(*HistoryAnalyzer)
	require.True(t, ok)

	// Modify fork1's shard.
	fork1.shards[0].globalHistory[1] = map[int]int64{0: 100}

	// fork2's shard should be unaffected.
	require.Empty(t, fork2.shards[0].globalHistory,
		"fork2 shard should be independent of fork1")

	// Parent's shard should be unaffected.
	require.Empty(t, b.shards[0].globalHistory,
		"parent shard should be independent of forks")
}

func TestHistoryAnalyzer_Fork_IndependentRenames(t *testing.T) {
	t.Parallel()

	b := &HistoryAnalyzer{
		Granularity: DefaultBurndownGranularity,
		Sampling:    DefaultBurndownSampling,
		Goroutines:  2,
	}
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

func TestHistoryAnalyzer_Merge_CombinesShardHistories(t *testing.T) {
	t.Parallel()

	b := &HistoryAnalyzer{
		Granularity: DefaultBurndownGranularity,
		Sampling:    DefaultBurndownSampling,
		Goroutines:  2,
	}
	err := b.Initialize(nil)
	require.NoError(t, err)

	// Create branches with different history data.
	branch1 := &HistoryAnalyzer{
		Granularity: DefaultBurndownGranularity,
		Sampling:    DefaultBurndownSampling,
		Goroutines:  2,
	}
	err = branch1.Initialize(nil)
	require.NoError(t, err)

	branch1.shards[0].globalHistory[1] = map[int]int64{0: 100}
	branch1.tick = 5

	branch2 := &HistoryAnalyzer{
		Granularity: DefaultBurndownGranularity,
		Sampling:    DefaultBurndownSampling,
		Goroutines:  2,
	}
	err = branch2.Initialize(nil)
	require.NoError(t, err)

	branch2.shards[0].globalHistory[1] = map[int]int64{0: 50}
	branch2.shards[1].globalHistory[2] = map[int]int64{1: 200}
	branch2.tick = 10

	b.Merge([]analyze.HistoryAnalyzer{branch1, branch2})

	// Histories should be combined.
	require.Equal(t, int64(150), b.shards[0].globalHistory[1][0],
		"shard 0 history should be summed")
	require.Equal(t, int64(200), b.shards[1].globalHistory[2][1],
		"shard 1 history should be merged")

	// tick should be max.
	require.Equal(t, 10, b.tick, "tick should be max of all branches")
}

func TestHistoryAnalyzer_Merge_CombinesRenames(t *testing.T) {
	t.Parallel()

	b := &HistoryAnalyzer{
		Granularity: DefaultBurndownGranularity,
		Sampling:    DefaultBurndownSampling,
		Goroutines:  2,
	}
	err := b.Initialize(nil)
	require.NoError(t, err)

	branch1 := &HistoryAnalyzer{
		Granularity: DefaultBurndownGranularity,
		Sampling:    DefaultBurndownSampling,
		Goroutines:  2,
		renames:     map[string]string{"a.go": "b.go"},
	}

	branch2 := &HistoryAnalyzer{
		Granularity: DefaultBurndownGranularity,
		Sampling:    DefaultBurndownSampling,
		Goroutines:  2,
		renames:     map[string]string{"c.go": "d.go"},
	}

	b.Merge([]analyze.HistoryAnalyzer{branch1, branch2})

	// Both renames should be present.
	require.Equal(t, "b.go", b.renames["a.go"])
	require.Equal(t, "d.go", b.renames["c.go"])
}

func TestHistoryAnalyzer_Merge_HandlesEmptyBranches(t *testing.T) {
	t.Parallel()

	b := &HistoryAnalyzer{
		Granularity: DefaultBurndownGranularity,
		Sampling:    DefaultBurndownSampling,
		Goroutines:  2,
	}
	err := b.Initialize(nil)
	require.NoError(t, err)

	b.shards[0].globalHistory[1] = map[int]int64{0: 100}

	emptyBranch := &HistoryAnalyzer{
		Granularity: DefaultBurndownGranularity,
		Sampling:    DefaultBurndownSampling,
		Goroutines:  2,
	}
	err = emptyBranch.Initialize(nil)
	require.NoError(t, err)

	b.Merge([]analyze.HistoryAnalyzer{emptyBranch})

	// Original history should be preserved.
	require.Equal(t, int64(100), b.shards[0].globalHistory[1][0])
}

func TestHistoryAnalyzer_Merge_HandlesNilBranches(t *testing.T) {
	t.Parallel()

	b := &HistoryAnalyzer{
		Granularity: DefaultBurndownGranularity,
		Sampling:    DefaultBurndownSampling,
		Goroutines:  2,
	}
	err := b.Initialize(nil)
	require.NoError(t, err)

	// Merge with nil slice should not panic.
	b.Merge(nil)
}

func TestHistoryAnalyzer_ForkMerge_RoundTrip(t *testing.T) {
	t.Parallel()

	b := &HistoryAnalyzer{
		Granularity:  DefaultBurndownGranularity,
		Sampling:     DefaultBurndownSampling,
		Goroutines:   2,
		PeopleNumber: 1,
	}
	err := b.Initialize(nil)
	require.NoError(t, err)

	// Fork.
	forks := b.Fork(2)

	fork1, ok := forks[0].(*HistoryAnalyzer)
	require.True(t, ok)

	fork2, ok := forks[1].(*HistoryAnalyzer)
	require.True(t, ok)

	// Simulate parallel processing - each fork accumulates different data.
	fork1.shards[0].globalHistory[1] = map[int]int64{0: 100}
	fork1.tick = 5

	fork2.shards[0].globalHistory[2] = map[int]int64{1: 200}
	fork2.tick = 10

	// Merge back.
	b.Merge(forks)

	// Should have data from both forks.
	require.Equal(t, int64(100), b.shards[0].globalHistory[1][0])
	require.Equal(t, int64(200), b.shards[0].globalHistory[2][1])
	require.Equal(t, 10, b.tick)
}
