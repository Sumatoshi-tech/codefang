package burndown

import (
	"bytes"
	"encoding/json"
	"testing"
	"time"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/burndown"
	"github.com/Sumatoshi-tech/codefang/internal/identity"
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

// --- collectFileOwnership Tests ---.

func TestCollectFileOwnership_EmptyShards(t *testing.T) {
	t.Parallel()

	b := NewHistoryAnalyzer()
	b.Granularity = DefaultBurndownGranularity
	b.Sampling = DefaultBurndownSampling
	b.Goroutines = 2
	b.PeopleNumber = 1
	b.TrackFiles = true

	err := b.Initialize(nil)
	require.NoError(t, err)

	ownership := b.collectFileOwnership()
	assert.Empty(t, ownership)
}

func TestCollectFileOwnership_WithFiles(t *testing.T) {
	t.Parallel()

	b := NewHistoryAnalyzer()
	b.Granularity = DefaultBurndownGranularity
	b.Sampling = DefaultBurndownSampling
	b.Goroutines = 1
	b.PeopleNumber = 2
	b.TrackFiles = true

	err := b.Initialize(nil)
	require.NoError(t, err)

	// Create a file in shard 0 with author=1, tick=5.
	packed := 5 | (1 << burndown.TreeMaxBinPower)
	file := burndown.NewFile(packed, 80)

	b.shards[0].filesByID = append(b.shards[0].filesByID, file)

	ownership := b.collectFileOwnership()

	require.Contains(t, ownership, PathID(0))
	assert.Equal(t, 80, ownership[PathID(0)][1])
}

func TestCollectDeltas_IncludesFileOwnership(t *testing.T) {
	t.Parallel()

	b := NewHistoryAnalyzer()
	b.Granularity = DefaultBurndownGranularity
	b.Sampling = DefaultBurndownSampling
	b.Goroutines = 1
	b.PeopleNumber = 2
	b.TrackFiles = true

	err := b.Initialize(nil)
	require.NoError(t, err)

	b.resetDeltaBuffers()

	// Create a file in shard 0 with author=0, tick=3.
	packed := 3 | (0 << burndown.TreeMaxBinPower)
	file := burndown.NewFile(packed, 50)

	b.shards[0].filesByID = append(b.shards[0].filesByID, file)

	result := b.collectDeltas()

	require.NotNil(t, result.FileOwnership)
	require.Contains(t, result.FileOwnership, PathID(0))
	assert.Equal(t, 50, result.FileOwnership[PathID(0)][0])
}

func TestCollectDeltas_NoOwnershipWithoutPeople(t *testing.T) {
	t.Parallel()

	b := NewHistoryAnalyzer()
	b.Granularity = DefaultBurndownGranularity
	b.Sampling = DefaultBurndownSampling
	b.Goroutines = 1
	b.PeopleNumber = 0
	b.TrackFiles = true

	err := b.Initialize(nil)
	require.NoError(t, err)

	b.resetDeltaBuffers()

	result := b.collectDeltas()

	assert.Nil(t, result.FileOwnership)
}

// --- groupSparseHistory Tests ---.

func TestGroupSparseHistory_Empty(t *testing.T) {
	t.Parallel()

	b := NewHistoryAnalyzer()
	b.Granularity = DefaultBurndownGranularity
	b.Sampling = DefaultBurndownSampling

	result := b.groupSparseHistory(sparseHistory{}, 30)

	assert.Equal(t, DenseHistory{}, result)
}

func TestGroupSparseHistory_SingleTick(t *testing.T) {
	t.Parallel()

	b := NewHistoryAnalyzer()
	b.Granularity = 30
	b.Sampling = 30

	history := sparseHistory{
		0: {0: 100},
	}

	result := b.groupSparseHistory(history, 0)

	require.Len(t, result, 1) // 1 sample (tick 0 / sampling 30 = 0, +1).
	require.Len(t, result[0], 1)
	assert.Equal(t, int64(100), result[0][0])
}

func TestGroupSparseHistory_MultiTickForwardFill(t *testing.T) {
	t.Parallel()

	b := NewHistoryAnalyzer()
	b.Granularity = 30
	b.Sampling = 30

	// Tick 0: 100 lines in band 0.
	// Tick 60: 50 new lines in band 1.
	// Tick 30 is a gap — should forward-fill from tick 0.
	history := sparseHistory{
		0:  {0: 100},
		60: {30: 50},
	}

	result := b.groupSparseHistory(history, 60)

	// samples = 60/30 + 1 = 3.
	// bands = 60/30 + 1 = 3.
	require.Len(t, result, 3)

	// Sample 0: band 0 = 100.
	assert.Equal(t, int64(100), result[0][0])

	// Sample 1: forward-filled from sample 0.
	assert.Equal(t, int64(100), result[1][0])

	// Sample 2: forward-fill + new data.
	assert.Equal(t, int64(100), result[2][0])
	assert.Equal(t, int64(50), result[2][1])
}

func TestGroupSparseHistory_SamplingEqualsGranularity(t *testing.T) {
	t.Parallel()

	b := NewHistoryAnalyzer()
	b.Granularity = 30
	b.Sampling = 30

	history := sparseHistory{
		0:  {0: 100},
		30: {0: -20, 30: 80},
	}

	result := b.groupSparseHistory(history, 30)

	require.Len(t, result, 2)
	assert.Equal(t, int64(100), result[0][0])
	// Sample 1: carried 100, then delta -20 + 80 in band 1.
	assert.Equal(t, int64(80), result[1][0])
	assert.Equal(t, int64(80), result[1][1])
}

func TestNormalizeTicks_EmptyHistory(t *testing.T) {
	t.Parallel()

	b := NewHistoryAnalyzer()
	b.Granularity = 30
	b.Sampling = 30

	ticks, lastTick := b.normalizeTicks(sparseHistory{}, 60)

	assert.Empty(t, ticks)
	assert.Equal(t, 60, lastTick)
}

func TestNormalizeTicks_NegativeLastTick(t *testing.T) {
	t.Parallel()

	b := NewHistoryAnalyzer()
	b.Granularity = 30
	b.Sampling = 30

	ticks, lastTick := b.normalizeTicks(sparseHistory{}, -1)

	assert.Empty(t, ticks)
	assert.Equal(t, 0, lastTick)
}

// --- Configure Tests ---.

func TestConfigure_WithPeopleTracking(t *testing.T) {
	t.Parallel()

	b := NewHistoryAnalyzer()

	facts := map[string]any{
		ConfigBurndownGranularity:                       15,
		ConfigBurndownSampling:                          10,
		ConfigBurndownTrackFiles:                        true,
		ConfigBurndownTrackPeople:                       true,
		identity.FactIdentityDetectorPeopleCount:        3,
		identity.FactIdentityDetectorReversedPeopleDict: []string{"Alice", "Bob", "Charlie"},
		ConfigBurndownHibernationThreshold:              500,
		ConfigBurndownHibernationToDisk:                 true,
		ConfigBurndownDebug:                             true,
		ConfigBurndownGoroutines:                        8,
	}

	err := b.Configure(facts)
	require.NoError(t, err)

	assert.Equal(t, 15, b.Granularity)
	assert.Equal(t, 10, b.Sampling)
	assert.True(t, b.TrackFiles)
	assert.Equal(t, 3, b.PeopleNumber)
	assert.Equal(t, []string{"Alice", "Bob", "Charlie"}, b.ReversedPeopleDict)
	assert.Equal(t, 500, b.HibernationThreshold)
	assert.True(t, b.HibernationToDisk)
	assert.True(t, b.Debug)
	assert.Equal(t, 8, b.Goroutines)
}

func TestConfigure_NegativePeopleCount(t *testing.T) {
	t.Parallel()

	b := NewHistoryAnalyzer()

	facts := map[string]any{
		ConfigBurndownTrackPeople:                true,
		identity.FactIdentityDetectorPeopleCount: -1,
	}

	err := b.Configure(facts)
	require.Error(t, err)
}

func TestConfigure_PeopleTrackingWithBadDict(t *testing.T) {
	t.Parallel()

	b := NewHistoryAnalyzer()

	facts := map[string]any{
		ConfigBurndownTrackPeople:                       true,
		identity.FactIdentityDetectorPeopleCount:        2,
		identity.FactIdentityDetectorReversedPeopleDict: "not a slice",
	}

	err := b.Configure(facts)
	require.Error(t, err)
}

func TestInitialize_NegativePeopleNumber(t *testing.T) {
	t.Parallel()

	b := NewHistoryAnalyzer()
	b.PeopleNumber = -1

	err := b.Initialize(nil)
	require.Error(t, err)
}

func TestInitialize_SamplingAdjustment(t *testing.T) {
	t.Parallel()

	b := NewHistoryAnalyzer()
	b.Granularity = 15
	b.Sampling = 30 // Larger than granularity, should be clamped.

	err := b.Initialize(nil)
	require.NoError(t, err)

	assert.Equal(t, 15, b.Sampling)
}

// --- packPersonWithTick / unpackPersonWithTick Tests ---.

func TestPackPersonWithTick_NoPeople(t *testing.T) {
	t.Parallel()

	b := NewHistoryAnalyzer()
	b.PeopleNumber = 0
	result := b.packPersonWithTick(5, 42)
	assert.Equal(t, 42, result) // when no people, returns tick directly.
}

func TestPackPersonWithTick_WithPeople(t *testing.T) {
	t.Parallel()

	b := NewHistoryAnalyzer()
	b.PeopleNumber = 2
	result := b.packPersonWithTick(1, 5)
	// tick = 5, person = 1 shifted by TreeMaxBinPower.
	assert.Equal(t, 5|(1<<burndown.TreeMaxBinPower), result)
}

func TestUnpackPersonWithTick_NoPeople(t *testing.T) {
	t.Parallel()

	b := NewHistoryAnalyzer()
	b.PeopleNumber = 0
	person, tick := b.unpackPersonWithTick(42)
	assert.Equal(t, identity.AuthorMissing, person)
	assert.Equal(t, 42, tick)
}

func TestUnpackPersonWithTick_WithPeople(t *testing.T) {
	t.Parallel()

	b := NewHistoryAnalyzer()
	b.PeopleNumber = 3
	packed := b.packPersonWithTick(2, 7)
	person, tick := b.unpackPersonWithTick(packed)
	assert.Equal(t, 2, person)
	assert.Equal(t, 7, tick)
}

// --- Simple Getter Tests ---.

func TestWorkingStateSize(t *testing.T) {
	t.Parallel()

	b := NewHistoryAnalyzer()
	assert.Positive(t, b.WorkingStateSize())
}

func TestAvgTCSize(t *testing.T) {
	t.Parallel()

	b := NewHistoryAnalyzer()
	assert.Positive(t, b.AvgTCSize())
}

// --- Serialize Plot Tests ---.

func TestSerialize_Plot(t *testing.T) {
	t.Parallel()

	b := NewHistoryAnalyzer()

	report := analyze.Report{
		"GlobalHistory": DenseHistory{{100, 50}, {120, 60}},
		"Sampling":      30,
		"Granularity":   30,
		"TickSize":      24 * time.Hour,
	}

	var buf bytes.Buffer

	err := b.Serialize(report, analyze.FormatPlot, &buf)
	require.NoError(t, err)
	assert.Positive(t, buf.Len())
}

func TestSerialize_PlotEmpty(t *testing.T) {
	t.Parallel()

	b := NewHistoryAnalyzer()

	report := analyze.Report{
		"GlobalHistory": DenseHistory{},
		"Sampling":      30,
		"Granularity":   30,
		"TickSize":      24 * time.Hour,
	}

	var buf bytes.Buffer

	err := b.Serialize(report, analyze.FormatPlot, &buf)
	require.NoError(t, err)
	assert.Positive(t, buf.Len())
}

func TestSerializeTICKs_Plot(t *testing.T) {
	t.Parallel()

	b := NewHistoryAnalyzer()
	b.Granularity = 30
	b.Sampling = 30
	b.TickSize = 24 * time.Hour

	ticks := []analyze.TICK{
		{
			Tick: 0,
			Data: &TickResult{
				GlobalHistory: sparseHistory{0: {0: 100}},
			},
		},
	}

	var buf bytes.Buffer

	err := b.SerializeTICKs(ticks, analyze.FormatPlot, &buf)
	require.NoError(t, err)
	assert.Positive(t, buf.Len())
}

// --- onNewTick Tests ---.

func TestOnNewTick(t *testing.T) {
	t.Parallel()

	b := NewHistoryAnalyzer()
	b.tick = 5
	b.previousTick = 3
	b.mergedAuthor = 1

	b.onNewTick()

	assert.Equal(t, 5, b.previousTick)
	assert.Equal(t, identity.AuthorMissing, b.mergedAuthor)
}

func TestOnNewTick_NoPreviousAdvance(t *testing.T) {
	t.Parallel()

	b := NewHistoryAnalyzer()
	b.tick = 3
	b.previousTick = 5 // Already ahead.
	b.mergedAuthor = 1

	b.onNewTick()

	assert.Equal(t, 5, b.previousTick)                      // Unchanged.
	assert.Equal(t, identity.AuthorMissing, b.mergedAuthor) // Always reset.
}

// --- CleanupSpills Tests ---.

func TestCleanupSpills_NoSpills(t *testing.T) {
	t.Parallel()

	b := NewHistoryAnalyzer()
	b.Goroutines = 2
	err := b.Initialize(nil)
	require.NoError(t, err)
	// Should not panic.
	b.CleanupSpills()
}

// --- fillDenseHistory / groupSparseHistory Tests ---.

func TestFillDenseHistory(t *testing.T) {
	t.Parallel()

	b := NewHistoryAnalyzer()
	b.Granularity = 30
	b.Sampling = 30

	history := sparseHistory{
		0: {0: 100},
	}
	result := b.groupSparseHistory(history, 30)
	require.Len(t, result, 2)
	// First sample: band 0 = 100.
	assert.Equal(t, int64(100), result[0][0])
	// Second sample: forward-filled from first.
	assert.Equal(t, int64(100), result[1][0])
}
