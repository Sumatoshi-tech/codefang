package burndown

import (
	"bytes"
	"context"
	"testing"
	"time"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/burndown"
	"github.com/Sumatoshi-tech/codefang/internal/identity"
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

// --- extractFileOwnership Tests ---.

func TestExtractFileOwnership_Empty(t *testing.T) {
	t.Parallel()

	// File with zero lines: time=0, length=0.
	file := burndown.NewFile(0, 0)

	result := extractFileOwnership(file, func(v int) (int, int) {
		return v >> burndown.TreeMaxBinPower, v & int(burndown.TreeMergeMark)
	})

	assert.Empty(t, result)
}

func TestExtractFileOwnership_SingleAuthor(t *testing.T) {
	t.Parallel()

	// Pack author=1, tick=5 into the value.
	packed := 5 | (1 << burndown.TreeMaxBinPower)
	file := burndown.NewFile(packed, 100)

	result := extractFileOwnership(file, func(v int) (int, int) {
		return v >> burndown.TreeMaxBinPower, v & int(burndown.TreeMergeMark)
	})

	require.Len(t, result, 1)
	assert.Equal(t, 100, result[1])
}

func TestExtractFileOwnership_MultipleAuthors(t *testing.T) {
	t.Parallel()

	// Create a file with author=0, tick=1 for 50 lines.
	packed0 := 1 | (0 << burndown.TreeMaxBinPower)
	file := burndown.NewFile(packed0, 50)

	// Add 30 lines from author=1 at tick=2.
	packed1 := 2 | (1 << burndown.TreeMaxBinPower)
	file.Update(packed1, 50, 30, 0) // Insert 30 lines at position 50.

	unpack := func(v int) (int, int) {
		return v >> burndown.TreeMaxBinPower, v & int(burndown.TreeMergeMark)
	}

	result := extractFileOwnership(file, unpack)

	assert.Equal(t, 50, result[0])
	assert.Equal(t, 30, result[1])
}

func TestExtractFileOwnership_IgnoresAuthorMissing(t *testing.T) {
	t.Parallel()

	// When PeopleNumber=0, unpack returns AuthorMissing.
	file := burndown.NewFile(5, 100)

	result := extractFileOwnership(file, func(v int) (int, int) {
		return identity.AuthorMissing, v
	})

	assert.Empty(t, result)
}

// --- mergeFileOwnership Tests ---.

func TestAggregator_Add_FileOwnership(t *testing.T) {
	t.Parallel()

	pi := NewPathInterner()
	mainID := pi.Intern("main.go")

	agg := newTestAggregator(withFiles(pi))

	cr := &CommitResult{
		GlobalDeltas: sparseHistory{1: {0: 100}},
		FileDeltas:   map[PathID]sparseHistory{mainID: {1: {0: 100}}},
		FileOwnership: map[PathID]map[int]int{
			mainID: {0: 70, 1: 30},
		},
	}

	err := agg.Add(analyze.TC{Data: cr, Tick: 1})
	require.NoError(t, err)

	require.NotNil(t, agg.fileOwnership)
	assert.Equal(t, 70, agg.fileOwnership[mainID][0])
	assert.Equal(t, 30, agg.fileOwnership[mainID][1])
}

func TestAggregator_Add_FileOwnershipReplaces(t *testing.T) {
	t.Parallel()

	pi := NewPathInterner()
	mainID := pi.Intern("main.go")

	agg := newTestAggregator(withFiles(pi))

	// First commit: author 0 owns 100 lines.
	err := agg.Add(analyze.TC{
		Data: &CommitResult{
			GlobalDeltas:  sparseHistory{1: {0: 100}},
			FileOwnership: map[PathID]map[int]int{mainID: {0: 100}},
		},
		Tick: 1,
	})
	require.NoError(t, err)

	// Second commit: author 0 now owns 60, author 1 owns 40.
	err = agg.Add(analyze.TC{
		Data: &CommitResult{
			GlobalDeltas:  sparseHistory{2: {0: 10}},
			FileOwnership: map[PathID]map[int]int{mainID: {0: 60, 1: 40}},
		},
		Tick: 2,
	})
	require.NoError(t, err)

	// Ownership should be replaced, not accumulated.
	assert.Equal(t, 60, agg.fileOwnership[mainID][0])
	assert.Equal(t, 40, agg.fileOwnership[mainID][1])
}

func TestAggregator_FlushTick_IncludesFileOwnership(t *testing.T) {
	t.Parallel()

	pi := NewPathInterner()
	mainID := pi.Intern("main.go")

	agg := newTestAggregator(withFiles(pi))

	err := agg.Add(analyze.TC{
		Data: &CommitResult{
			GlobalDeltas:  sparseHistory{1: {0: 100}},
			FileOwnership: map[PathID]map[int]int{mainID: {0: 100}},
		},
		Tick: 1,
	})
	require.NoError(t, err)

	tick, err := agg.FlushTick(1)
	require.NoError(t, err)

	tr, ok := tick.Data.(*TickResult)
	require.True(t, ok)
	require.NotNil(t, tr.FileOwnership)
	assert.Equal(t, 100, tr.FileOwnership[mainID][0])
}

func TestAggregator_FlushTick_FileOwnershipCloned(t *testing.T) {
	t.Parallel()

	pi := NewPathInterner()
	mainID := pi.Intern("main.go")

	agg := newTestAggregator(withFiles(pi))

	err := agg.Add(analyze.TC{
		Data: &CommitResult{
			GlobalDeltas:  sparseHistory{1: {0: 100}},
			FileOwnership: map[PathID]map[int]int{mainID: {0: 100}},
		},
		Tick: 1,
	})
	require.NoError(t, err)

	tick, err := agg.FlushTick(1)
	require.NoError(t, err)

	tr, ok := tick.Data.(*TickResult)
	require.True(t, ok)
	// Mutate the clone.
	tr.FileOwnership[mainID][0] = 999

	// Original should be unchanged.
	assert.Equal(t, 100, agg.fileOwnership[mainID][0])
}

// --- FlushAllTicks Tests ---.

func TestAggregator_FlushAllTicks_Empty(t *testing.T) {
	t.Parallel()

	agg := newTestAggregator()

	ticks, err := agg.FlushAllTicks()
	require.NoError(t, err)
	assert.Nil(t, ticks)
}

func TestAggregator_FlushAllTicks_WithData(t *testing.T) {
	t.Parallel()

	agg := newTestAggregator()

	err := agg.Add(analyze.TC{
		Data: &CommitResult{GlobalDeltas: sparseHistory{1: {0: 100}}},
		Tick: 1,
	})
	require.NoError(t, err)

	ticks, err := agg.FlushAllTicks()
	require.NoError(t, err)
	require.Len(t, ticks, 1)
	assert.Equal(t, 1, ticks[0].Tick)

	tr, ok := ticks[0].Data.(*TickResult)
	require.True(t, ok)
	assert.Equal(t, int64(100), tr.GlobalHistory[1][0])
}

// --- SpillCollect with FileOwnership ---.

func TestAggregator_SpillCollect_FileOwnership(t *testing.T) {
	t.Parallel()

	pi := NewPathInterner()
	mainID := pi.Intern("main.go")

	agg := newTestAggregator(withFiles(pi))
	agg.opts.SpillDir = t.TempDir()

	err := agg.Add(analyze.TC{
		Data: &CommitResult{
			GlobalDeltas:  sparseHistory{1: {0: 100}},
			FileOwnership: map[PathID]map[int]int{mainID: {0: 100}},
		},
		Tick: 1,
	})
	require.NoError(t, err)

	freed, err := agg.Spill()
	require.NoError(t, err)
	assert.Positive(t, freed)
	assert.Nil(t, agg.fileOwnership)

	err = agg.Collect()
	require.NoError(t, err)

	assert.Equal(t, 100, agg.fileOwnership[mainID][0])
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

// --- Additional Coverage Tests ---.

func TestTicksToReport_FileOwnership(t *testing.T) {
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
				FileOwnership: map[PathID]map[int]int{
					mainID: {0: 70, 1: 30},
				},
			},
		},
	}

	report := ticksToReport(context.Background(), ticks, 30, 30, 2, true, 24*time.Hour, []string{"Alice", "Bob"}, pi)

	fo, ok := report["FileOwnership"].(map[string]map[int]int)
	require.True(t, ok)
	require.Contains(t, fo, "main.go")
	assert.Equal(t, 70, fo["main.go"][0])
	assert.Equal(t, 30, fo["main.go"][1])
}

func TestFindEndTime_Empty(t *testing.T) {
	t.Parallel()

	result := findEndTime(nil)
	assert.True(t, result.IsZero())
}

func TestFindEndTime_WithTicks(t *testing.T) {
	t.Parallel()

	t1 := time.Date(2024, 1, 1, 0, 0, 0, 0, time.UTC)
	t2 := time.Date(2025, 6, 1, 0, 0, 0, 0, time.UTC)

	ticks := []analyze.TICK{
		{Tick: 0, EndTime: t1, Data: &TickResult{GlobalHistory: sparseHistory{0: {0: 10}}}},
		{Tick: 1, EndTime: t2, Data: &TickResult{GlobalHistory: sparseHistory{1: {0: 20}}}},
	}

	result := findEndTime(ticks)
	assert.Equal(t, t2, result)
}

func TestMergeAllTicks_NilData(t *testing.T) {
	t.Parallel()

	ticks := []analyze.TICK{
		{Tick: 0, Data: nil},
		{Tick: 1, Data: "not a TickResult"},
	}
	result := mergeAllTicks(ticks)
	assert.Nil(t, result)
}

func TestMergeAllTicks_FileOwnershipLastTickWins(t *testing.T) {
	t.Parallel()

	ticks := []analyze.TICK{
		{
			Tick: 0,
			Data: &TickResult{
				GlobalHistory: sparseHistory{0: {0: 100}},
				FileOwnership: map[PathID]map[int]int{PathID(0): {0: 100}},
			},
		},
		{
			Tick: 1,
			Data: &TickResult{
				GlobalHistory: sparseHistory{0: {0: 50}},
				FileOwnership: map[PathID]map[int]int{PathID(0): {0: 60, 1: 40}},
			},
		},
	}
	result := mergeAllTicks(ticks)
	require.NotNil(t, result)
	// Last tick wins for ownership.
	assert.Equal(t, 60, result.FileOwnership[PathID(0)][0])
	assert.Equal(t, 40, result.FileOwnership[PathID(0)][1])
}

func TestNewAggregator_SetsFields(t *testing.T) {
	t.Parallel()

	pi := NewPathInterner()
	opts := analyze.AggregatorOptions{SpillBudget: 1024}

	agg := newAggregator(opts, 30, 30, 2, true, 24*time.Hour, []string{"Alice", "Bob"}, pi)

	assert.Equal(t, 30, agg.granularity)
	assert.Equal(t, 30, agg.sampling)
	assert.Equal(t, 2, agg.peopleNumber)
	assert.True(t, agg.trackFiles)
	assert.Equal(t, 24*time.Hour, agg.tickSize)
	assert.Equal(t, []string{"Alice", "Bob"}, agg.reversedPeopleDict)
	assert.Same(t, pi, agg.pathInterner)
	assert.NotNil(t, agg.globalHistory)
	assert.NotNil(t, agg.peopleHistories)
	assert.NotNil(t, agg.fileHistories)
}

func TestAggregator_CloneFileHistories_Empty(t *testing.T) {
	t.Parallel()

	agg := newTestAggregator()
	clone := agg.cloneFileHistories()
	assert.Nil(t, clone)
}

func TestAggregator_CloneFileHistories_WithData(t *testing.T) {
	t.Parallel()

	pi := NewPathInterner()
	mainID := pi.Intern("main.go")

	agg := newTestAggregator(withFiles(pi))
	agg.fileHistories[mainID] = sparseHistory{0: {0: 100}}

	clone := agg.cloneFileHistories()
	require.NotNil(t, clone)
	assert.Equal(t, int64(100), clone[mainID][0][0])

	// Mutate clone, original should be unchanged.
	clone[mainID][0][0] = 999
	assert.Equal(t, int64(100), agg.fileHistories[mainID][0][0])
}

func TestAddFilesToReport_EmptyHistories(t *testing.T) {
	t.Parallel()

	pi := NewPathInterner()

	merged := &TickResult{
		FileHistories: map[PathID]sparseHistory{},
	}

	report := analyze.Report{}
	converter := &HistoryAnalyzer{Granularity: 30, Sampling: 30}
	addFilesToReport(report, merged, converter, 0, pi)

	fh, ok := report["FileHistories"].(map[string]DenseHistory)
	require.True(t, ok)
	assert.Empty(t, fh)
}

func TestEstimateSparseHistorySize(t *testing.T) {
	t.Parallel()

	empty := sparseHistory{}
	assert.Equal(t, int64(0), estimateSparseHistorySize(empty))

	h := sparseHistory{
		0: {0: 100, 1: 200},
		1: {0: 50},
	}
	size := estimateSparseHistorySize(h)
	assert.Positive(t, size)
	// 3 entries * sparseEntryBytes (56).
	assert.Equal(t, int64(3)*sparseEntryBytes, size)
}

func TestCloneMatrix_Nil(t *testing.T) {
	t.Parallel()

	agg := newTestAggregator()
	clone := agg.cloneMatrix()
	assert.Nil(t, clone)
}

func TestCloneMatrix_WithData(t *testing.T) {
	t.Parallel()

	agg := newTestAggregator()
	agg.matrix = []map[int]int64{
		{authorSelf: 50, 1: 30},
	}

	clone := agg.cloneMatrix()
	require.Len(t, clone, 1)
	assert.Equal(t, int64(50), clone[0][authorSelf])

	// Mutate clone.
	clone[0][authorSelf] = 999
	assert.Equal(t, int64(50), agg.matrix[0][authorSelf])
}

func TestClonePeopleHistories_Empty(t *testing.T) {
	t.Parallel()

	agg := newTestAggregator()
	clone := agg.clonePeopleHistories()
	assert.Nil(t, clone)
}

func TestClonePeopleHistories_WithData(t *testing.T) {
	t.Parallel()

	agg := newTestAggregator(withPeople(2, []string{"Alice", "Bob"}))
	agg.peopleHistories = map[int]sparseHistory{
		0: {1: {0: 100}},
	}

	clone := agg.clonePeopleHistories()
	require.NotNil(t, clone)
	require.Len(t, clone, 1) // maxAuthor=0 -> slice of length 1.
	assert.Equal(t, int64(100), clone[0][1][0])

	// Mutate clone.
	clone[0][1][0] = 999

	assert.Equal(t, int64(100), agg.peopleHistories[0][1][0])
}

func TestCloneFileOwnership_Nil(t *testing.T) {
	t.Parallel()

	agg := newTestAggregator()
	clone := agg.cloneFileOwnership()
	assert.Nil(t, clone)
}

func TestCloneFileOwnership_WithData(t *testing.T) {
	t.Parallel()

	agg := newTestAggregator()
	agg.fileOwnership = map[PathID]map[int]int{
		PathID(0): {0: 100, 1: 50},
	}

	clone := agg.cloneFileOwnership()
	require.NotNil(t, clone)
	assert.Equal(t, 100, clone[PathID(0)][0])

	// Mutate clone.
	clone[PathID(0)][0] = 999

	assert.Equal(t, 100, agg.fileOwnership[PathID(0)][0])
}

// --- SpillState / RestoreSpillState Tests ---.

func TestAggregator_SpillState(t *testing.T) {
	t.Parallel()

	agg := newTestAggregator()
	agg.opts.SpillDir = t.TempDir()

	err := agg.Add(analyze.TC{
		Data: &CommitResult{GlobalDeltas: sparseHistory{1: {0: 100}}},
		Tick: 1,
	})
	require.NoError(t, err)

	_, err = agg.Spill()
	require.NoError(t, err)

	info := agg.SpillState()
	assert.NotEmpty(t, info.Dir)
	assert.Equal(t, 1, info.Count)
}

func TestAggregator_RestoreSpillState(t *testing.T) {
	t.Parallel()

	// First aggregator: add data and spill.
	agg1 := newTestAggregator()
	agg1.opts.SpillDir = t.TempDir()

	err := agg1.Add(analyze.TC{
		Data: &CommitResult{GlobalDeltas: sparseHistory{1: {0: 100}}},
		Tick: 1,
	})
	require.NoError(t, err)

	_, err = agg1.Spill()
	require.NoError(t, err)

	info := agg1.SpillState()

	// Second aggregator: restore spill state and collect.
	agg2 := newTestAggregator()
	agg2.RestoreSpillState(info)

	err = agg2.Collect()
	require.NoError(t, err)

	assert.Equal(t, int64(100), agg2.globalHistory[1][0])
}

// --- ticksToReport with file ownership through addFilesToReport ---.

func TestTicksToReport_WithFileOwnershipAndHistory(t *testing.T) {
	t.Parallel()

	pi := NewPathInterner()
	mainID := pi.Intern("main.go")
	utilID := pi.Intern("util.go")

	ticks := []analyze.TICK{
		{
			Tick:    0,
			EndTime: time.Date(2025, 6, 1, 0, 0, 0, 0, time.UTC),
			Data: &TickResult{
				GlobalHistory: sparseHistory{0: {0: 200}},
				FileHistories: map[PathID]sparseHistory{
					mainID: {0: {0: 120}},
					utilID: {0: {0: 80}},
				},
				FileOwnership: map[PathID]map[int]int{
					mainID: {0: 80, 1: 40},
					utilID: {0: 50, 1: 30},
				},
			},
		},
	}

	report := ticksToReport(
		context.Background(), ticks,
		30, 30, 2, true, 24*time.Hour,
		[]string{"Alice", "Bob"}, pi,
	)

	fh, ok := report["FileHistories"].(map[string]DenseHistory)
	require.True(t, ok)
	assert.Len(t, fh, 2)

	fo, ok := report["FileOwnership"].(map[string]map[int]int)
	require.True(t, ok)
	assert.Len(t, fo, 2)
	assert.Equal(t, 80, fo["main.go"][0])
	assert.Equal(t, 30, fo["util.go"][1])
}

// --- mergeTickPeopleHistories edge case ---.

func TestMergeTickPeopleHistories_Growth(t *testing.T) {
	t.Parallel()

	merged := &TickResult{
		GlobalHistory: sparseHistory{0: {0: 100}},
	}

	// Merge from author 3 — should grow the PeopleHistories slice.
	src := []sparseHistory{
		nil, nil, nil,
		{0: {0: 50}}, // Author 3.
	}

	mergeTickPeopleHistories(merged, src)

	require.Len(t, merged.PeopleHistories, 4)
	assert.Equal(t, int64(50), merged.PeopleHistories[3][0][0])
}
