package burndown

import (
	"bytes"
	"context"
	"testing"
	"time"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
)

func newTestAggregator(opts ...func(*Aggregator)) *Aggregator {
	pi := NewPathInterner()

	agg := newAggregator(
		analyze.AggregatorOptions{},
		DefaultBurndownGranularity, DefaultBurndownSampling, 0,
		false, 24*time.Hour,
		nil, pi,
	)

	for _, opt := range opts {
		opt(agg)
	}

	return agg
}

func withPeople(n int, names []string) func(*Aggregator) {
	return func(a *Aggregator) {
		a.peopleNumber = n
		a.reversedPeopleDict = names
	}
}

func withFiles(pi *PathInterner) func(*Aggregator) {
	return func(a *Aggregator) {
		a.trackFiles = true
		a.pathInterner = pi
	}
}

// --- Basic Add Tests ---.

func TestAggregator_Add_NilData(t *testing.T) {
	t.Parallel()

	agg := newTestAggregator()

	err := agg.Add(analyze.TC{Data: nil})
	require.NoError(t, err)
	assert.Empty(t, agg.globalHistory)
}

func TestAggregator_Add_WrongDataType(t *testing.T) {
	t.Parallel()

	agg := newTestAggregator()

	err := agg.Add(analyze.TC{Data: "not a CommitResult"})
	require.NoError(t, err)
	assert.Empty(t, agg.globalHistory)
}

func TestAggregator_Add_GlobalDeltas(t *testing.T) {
	t.Parallel()

	agg := newTestAggregator()

	cr := &CommitResult{
		GlobalDeltas: sparseHistory{1: {0: 100}},
	}

	err := agg.Add(analyze.TC{Data: cr, Tick: 1})
	require.NoError(t, err)
	assert.Equal(t, int64(100), agg.globalHistory[1][0])
	assert.Equal(t, 1, agg.lastTick)
}

func TestAggregator_Add_AccumulatesMultipleCommits(t *testing.T) {
	t.Parallel()

	agg := newTestAggregator()

	err := agg.Add(analyze.TC{
		Data: &CommitResult{GlobalDeltas: sparseHistory{1: {0: 100}}},
		Tick: 1,
	})
	require.NoError(t, err)

	err = agg.Add(analyze.TC{
		Data: &CommitResult{GlobalDeltas: sparseHistory{1: {0: 50}}},
		Tick: 1,
	})
	require.NoError(t, err)

	assert.Equal(t, int64(150), agg.globalHistory[1][0])
}

func TestAggregator_Add_PeopleDeltas(t *testing.T) {
	t.Parallel()

	agg := newTestAggregator(withPeople(2, []string{"Alice", "Bob"}))

	cr := &CommitResult{
		GlobalDeltas: sparseHistory{1: {0: 100}},
		PeopleDeltas: map[int]sparseHistory{
			0: {1: {0: 60}},
			1: {1: {0: 40}},
		},
		MatrixDeltas: []map[int]int64{
			{authorSelf: 50},
			{0: 30},
		},
	}

	err := agg.Add(analyze.TC{Data: cr, Tick: 1})
	require.NoError(t, err)

	assert.Equal(t, int64(60), agg.peopleHistories[0][1][0])
	assert.Equal(t, int64(40), agg.peopleHistories[1][1][0])
	assert.Equal(t, int64(50), agg.matrix[0][authorSelf])
	assert.Equal(t, int64(30), agg.matrix[1][0])
}

func TestAggregator_Add_FileDeltas(t *testing.T) {
	t.Parallel()

	pi := NewPathInterner()
	mainID := pi.Intern("main.go")

	agg := newTestAggregator(withFiles(pi))

	cr := &CommitResult{
		GlobalDeltas: sparseHistory{1: {0: 100}},
		FileDeltas:   map[PathID]sparseHistory{mainID: {1: {0: 100}}},
	}

	err := agg.Add(analyze.TC{Data: cr, Tick: 1})
	require.NoError(t, err)

	assert.Equal(t, int64(100), agg.fileHistories[mainID][1][0])
}

func TestAggregator_Add_TracksEndTime(t *testing.T) {
	t.Parallel()

	agg := newTestAggregator()

	t1 := time.Date(2025, 1, 1, 0, 0, 0, 0, time.UTC)
	t2 := time.Date(2025, 6, 1, 0, 0, 0, 0, time.UTC)

	err := agg.Add(analyze.TC{
		Data:      &CommitResult{GlobalDeltas: sparseHistory{1: {0: 10}}},
		Tick:      1,
		Timestamp: t1,
	})
	require.NoError(t, err)

	err = agg.Add(analyze.TC{
		Data:      &CommitResult{GlobalDeltas: sparseHistory{2: {0: 20}}},
		Tick:      2,
		Timestamp: t2,
	})
	require.NoError(t, err)

	assert.Equal(t, t2, agg.endTime)
	assert.Equal(t, 2, agg.lastTick)
}

// --- FlushTick Tests ---.

func TestAggregator_FlushTick_Empty(t *testing.T) {
	t.Parallel()

	agg := newTestAggregator()

	tick, err := agg.FlushTick(0)
	require.NoError(t, err)
	assert.Equal(t, 0, tick.Tick)
	assert.Nil(t, tick.Data)
}

func TestAggregator_FlushTick_WithData(t *testing.T) {
	t.Parallel()

	agg := newTestAggregator()

	err := agg.Add(analyze.TC{
		Data: &CommitResult{GlobalDeltas: sparseHistory{1: {0: 100}}},
		Tick: 1,
	})
	require.NoError(t, err)

	tick, err := agg.FlushTick(1)
	require.NoError(t, err)

	tr, ok := tick.Data.(*TickResult)
	require.True(t, ok)
	assert.Equal(t, int64(100), tr.GlobalHistory[1][0])
}

func TestAggregator_FlushTick_ReturnsClone(t *testing.T) {
	t.Parallel()

	agg := newTestAggregator()

	err := agg.Add(analyze.TC{
		Data: &CommitResult{GlobalDeltas: sparseHistory{1: {0: 100}}},
		Tick: 1,
	})
	require.NoError(t, err)

	tick, err := agg.FlushTick(1)
	require.NoError(t, err)

	tr, ok := tick.Data.(*TickResult)
	require.True(t, ok)
	// Mutate the returned data.
	tr.GlobalHistory[1][0] = 999

	// Original should be unchanged.
	assert.Equal(t, int64(100), agg.globalHistory[1][0])
}

// --- Spill/Collect Tests ---.

func TestAggregator_SpillCollect_RoundTrip(t *testing.T) {
	t.Parallel()

	agg := newTestAggregator(withPeople(2, []string{"Alice", "Bob"}))
	agg.opts.SpillDir = t.TempDir()

	err := agg.Add(analyze.TC{
		Data: &CommitResult{
			GlobalDeltas: sparseHistory{1: {0: 100}},
			PeopleDeltas: map[int]sparseHistory{0: {1: {0: 60}}},
			MatrixDeltas: []map[int]int64{{authorSelf: 50}},
		},
		Tick: 1,
	})
	require.NoError(t, err)

	freed, err := agg.Spill()
	require.NoError(t, err)
	assert.Positive(t, freed)

	// After spill, in-memory state should be empty.
	assert.Empty(t, agg.globalHistory)
	assert.Empty(t, agg.peopleHistories)
	assert.Nil(t, agg.matrix)

	err = agg.Collect()
	require.NoError(t, err)

	// After collect, data should be back.
	assert.Equal(t, int64(100), agg.globalHistory[1][0])
	assert.Equal(t, int64(60), agg.peopleHistories[0][1][0])
	assert.Equal(t, int64(50), agg.matrix[0][authorSelf])
}

func TestAggregator_SpillCollect_MultipleSpills(t *testing.T) {
	t.Parallel()

	agg := newTestAggregator()
	agg.opts.SpillDir = t.TempDir()

	// Spill #1.
	err := agg.Add(analyze.TC{
		Data: &CommitResult{GlobalDeltas: sparseHistory{1: {0: 100}}},
		Tick: 1,
	})
	require.NoError(t, err)

	_, err = agg.Spill()
	require.NoError(t, err)

	// Spill #2.
	err = agg.Add(analyze.TC{
		Data: &CommitResult{GlobalDeltas: sparseHistory{1: {0: 200}}},
		Tick: 1,
	})
	require.NoError(t, err)

	_, err = agg.Spill()
	require.NoError(t, err)

	// Add more in-memory.
	err = agg.Add(analyze.TC{
		Data: &CommitResult{GlobalDeltas: sparseHistory{2: {0: 50}}},
		Tick: 2,
	})
	require.NoError(t, err)

	err = agg.Collect()
	require.NoError(t, err)

	// Should be 100 + 200 = 300 for tick 1.
	assert.Equal(t, int64(300), agg.globalHistory[1][0])
	// Should be 50 for tick 2.
	assert.Equal(t, int64(50), agg.globalHistory[2][0])
}

func TestAggregator_Spill_Empty(t *testing.T) {
	t.Parallel()

	agg := newTestAggregator()

	freed, err := agg.Spill()
	require.NoError(t, err)
	assert.Equal(t, int64(0), freed)
}

func TestAggregator_AutoSpill(t *testing.T) {
	t.Parallel()

	agg := newTestAggregator()
	agg.opts.SpillBudget = 1 // Very small budget to trigger auto-spill.
	agg.opts.SpillDir = t.TempDir()

	err := agg.Add(analyze.TC{
		Data: &CommitResult{GlobalDeltas: sparseHistory{1: {0: 100}}},
		Tick: 1,
	})
	require.NoError(t, err)

	// Data should have been auto-spilled.
	assert.Equal(t, 1, agg.spillN)
	assert.Empty(t, agg.globalHistory)
}

// --- EstimatedStateSize Tests ---.

func TestAggregator_EstimatedStateSize_Empty(t *testing.T) {
	t.Parallel()

	agg := newTestAggregator()
	assert.Equal(t, int64(0), agg.EstimatedStateSize())
}

func TestAggregator_EstimatedStateSize_GrowsWithData(t *testing.T) {
	t.Parallel()

	agg := newTestAggregator()

	err := agg.Add(analyze.TC{
		Data: &CommitResult{GlobalDeltas: sparseHistory{1: {0: 100}}},
		Tick: 1,
	})
	require.NoError(t, err)

	assert.Positive(t, agg.EstimatedStateSize())
}

// --- Close Tests ---.

func TestAggregator_Close_Idempotent(t *testing.T) {
	t.Parallel()

	agg := newTestAggregator()

	err := agg.Close()
	require.NoError(t, err)

	err = agg.Close()
	require.NoError(t, err)
}

// --- ticksToReport Tests ---.

func TestTicksToReport_Empty(t *testing.T) {
	t.Parallel()

	report := ticksToReport(context.Background(), nil, 30, 30, 0, false, 24*time.Hour, nil, nil)
	assert.Empty(t, report)
}

func TestTicksToReport_GlobalHistory(t *testing.T) {
	t.Parallel()

	ticks := []analyze.TICK{
		{
			Tick: 0,
			Data: &TickResult{
				GlobalHistory: sparseHistory{
					0: {0: 100},
					1: {0: 50, 1: 30},
				},
			},
		},
	}

	report := ticksToReport(context.Background(), ticks, 30, 30, 0, false, 24*time.Hour, nil, nil)

	gh, ok := report["GlobalHistory"].(DenseHistory)
	require.True(t, ok)
	require.NotEmpty(t, gh)
}

func TestTicksToReport_PeopleHistories(t *testing.T) {
	t.Parallel()

	ticks := []analyze.TICK{
		{
			Tick: 0,
			Data: &TickResult{
				GlobalHistory: sparseHistory{0: {0: 100}},
				PeopleHistories: []sparseHistory{
					{0: {0: 60}},
					{0: {0: 40}},
				},
				Matrix: []map[int]int64{
					{authorSelf: 50},
					{0: 30},
				},
			},
		},
	}

	report := ticksToReport(context.Background(), ticks, 30, 30, 2, false, 24*time.Hour, []string{"Alice", "Bob"}, nil)

	ph, ok := report["PeopleHistories"].([]DenseHistory)
	require.True(t, ok)
	require.Len(t, ph, 2)

	pm, ok := report["PeopleMatrix"].(DenseHistory)
	require.True(t, ok)
	require.NotEmpty(t, pm)

	names, ok := report["ReversedPeopleDict"].([]string)
	require.True(t, ok)
	assert.Equal(t, []string{"Alice", "Bob"}, names)
}

func TestTicksToReport_FileHistories(t *testing.T) {
	t.Parallel()

	pi := NewPathInterner()
	mainID := pi.Intern("main.go")

	ticks := []analyze.TICK{
		{
			Tick: 0,
			Data: &TickResult{
				GlobalHistory: sparseHistory{0: {0: 100}},
				FileHistories: map[PathID]sparseHistory{
					mainID: {0: {0: 100}},
				},
			},
		},
	}

	report := ticksToReport(context.Background(), ticks, 30, 30, 0, true, 24*time.Hour, nil, pi)

	fh, ok := report["FileHistories"].(map[string]DenseHistory)
	require.True(t, ok)
	assert.Contains(t, fh, "main.go")
}

func TestTicksToReport_MergesMultipleTicks(t *testing.T) {
	t.Parallel()

	ticks := []analyze.TICK{
		{
			Tick: 0,
			Data: &TickResult{
				GlobalHistory: sparseHistory{0: {0: 100}},
			},
		},
		{
			Tick: 1,
			Data: &TickResult{
				GlobalHistory: sparseHistory{0: {0: 50}},
			},
		},
	}

	report := ticksToReport(context.Background(), ticks, 30, 30, 0, false, 24*time.Hour, nil, nil)

	gh, ok := report["GlobalHistory"].(DenseHistory)
	require.True(t, ok)
	require.NotEmpty(t, gh)
	// First sample should have merged value 150.
	assert.Equal(t, int64(150), gh[0][0])
}

func TestTicksToReport_ReportMetadata(t *testing.T) {
	t.Parallel()

	endTime := time.Date(2025, 6, 15, 0, 0, 0, 0, time.UTC)

	ticks := []analyze.TICK{
		{
			Tick:    0,
			EndTime: endTime,
			Data:    &TickResult{GlobalHistory: sparseHistory{0: {0: 100}}},
		},
	}

	report := ticksToReport(context.Background(), ticks, 30, 30, 0, false, 24*time.Hour, nil, nil)

	assert.Equal(t, 30, report["Sampling"])
	assert.Equal(t, 30, report["Granularity"])
	assert.Equal(t, 24*time.Hour, report["TickSize"])
	assert.Equal(t, endTime, report["EndTime"])
}

// --- buildDenseMatrix Tests ---.

func TestBuildDenseMatrix_Empty(t *testing.T) {
	t.Parallel()

	result := buildDenseMatrix(nil, 2)
	assert.Nil(t, result)
}

func TestBuildDenseMatrix_SelfAndOther(t *testing.T) {
	t.Parallel()

	sparse := []map[int]int64{
		{authorSelf: 50, 1: 30},
		{0: 20, authorSelf: 40},
	}

	dense := buildDenseMatrix(sparse, 2)
	require.Len(t, dense, 2)

	// Author 0: self=50 at col 0, author 1 → col 3.
	assert.Equal(t, int64(50), dense[0][0])
	assert.Equal(t, int64(30), dense[0][3])

	// Author 1: author 0 → col 2, self=40 at col 0.
	assert.Equal(t, int64(20), dense[1][2])
	assert.Equal(t, int64(40), dense[1][0])
}

// --- mapMatrixColumn Tests ---.

func TestMapMatrixColumn_Self(t *testing.T) {
	t.Parallel()

	assert.Equal(t, 0, mapMatrixColumn(authorSelf))
}

func TestMapMatrixColumn_Regular(t *testing.T) {
	t.Parallel()

	assert.Equal(t, 2, mapMatrixColumn(0))
	assert.Equal(t, 3, mapMatrixColumn(1))
	assert.Equal(t, 4, mapMatrixColumn(2))
}

// --- computeFileOwnership Tests ---.

func TestComputeFileOwnership_Empty(t *testing.T) {
	t.Parallel()

	result := computeFileOwnership(sparseHistory{})
	assert.Nil(t, result)
}

func TestComputeFileOwnership_LastTick(t *testing.T) {
	t.Parallel()

	history := sparseHistory{
		0: {0: 100, 1: 50},
		1: {0: 80, 1: 70},
	}

	ownership := computeFileOwnership(history)
	// Should use tick 1 (the highest).
	assert.Equal(t, 80, ownership[0])
	assert.Equal(t, 70, ownership[1])
}

// --- Integration: NewAggregator from HistoryAnalyzer ---.

func TestHistoryAnalyzer_NewAggregator_CreatesWorkingAggregator(t *testing.T) {
	t.Parallel()

	b := NewHistoryAnalyzer()
	b.Granularity = DefaultBurndownGranularity
	b.Sampling = DefaultBurndownSampling
	b.PeopleNumber = 2
	b.TickSize = 24 * time.Hour
	b.reversedPeopleDict = []string{"Alice", "Bob"}
	b.pathInterner = NewPathInterner()

	opts := analyze.AggregatorOptions{
		SpillBudget: 1024 * 1024,
		SpillDir:    t.TempDir(),
	}

	agg := b.NewAggregator(opts)
	require.NotNil(t, agg)

	// Should be able to add a TC.
	err := agg.Add(analyze.TC{
		Data: &CommitResult{GlobalDeltas: sparseHistory{1: {0: 100}}},
		Tick: 1,
	})
	require.NoError(t, err)

	// Should be able to flush.
	tick, err := agg.FlushTick(1)
	require.NoError(t, err)
	assert.NotNil(t, tick.Data)

	err = agg.Close()
	require.NoError(t, err)
}

// --- Integration: SerializeTICKs ---.

func TestHistoryAnalyzer_SerializeTICKs_ProducesJSON(t *testing.T) {
	t.Parallel()

	b := NewHistoryAnalyzer()
	b.Granularity = DefaultBurndownGranularity
	b.Sampling = DefaultBurndownSampling
	b.PeopleNumber = 2
	b.TickSize = 24 * time.Hour
	b.reversedPeopleDict = []string{"Alice", "Bob"}
	b.pathInterner = NewPathInterner()

	ticks := []analyze.TICK{
		{
			Tick: 0,
			Data: &TickResult{
				GlobalHistory: sparseHistory{0: {0: 100}},
				PeopleHistories: []sparseHistory{
					{0: {0: 60}},
					{0: {0: 40}},
				},
				Matrix: []map[int]int64{
					{authorSelf: 50},
					{0: 30},
				},
			},
		},
	}

	var buf = new(bytes.Buffer)

	err := b.SerializeTICKs(ticks, analyze.FormatJSON, buf)
	require.NoError(t, err)
	assert.Positive(t, buf.Len())
}
