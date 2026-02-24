package couples

import (
	"context"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
)

func TestAggregator_Add_StoresData(t *testing.T) {
	t.Parallel()

	agg := newAggregator(analyze.AggregatorOptions{}, 2, []string{"alice", "bob"}, nil)

	tc := analyze.TC{
		AuthorID: 0,
		Data: &CommitData{
			CouplingFiles: []string{"a.go", "b.go"},
			AuthorFiles:   map[string]int{"a.go": 1, "b.go": 1},
			CommitCounted: true,
		},
	}

	require.NoError(t, agg.Add(tc))

	// Verify file couplings.
	aLane, ok := agg.files.Get("a.go")
	require.True(t, ok)
	assert.Equal(t, 1, aLane["b.go"])
	assert.Equal(t, 1, aLane["a.go"])

	bLane, ok := agg.files.Get("b.go")
	require.True(t, ok)
	assert.Equal(t, 1, bLane["a.go"])

	// Verify people.
	assert.Equal(t, 1, agg.people[0]["a.go"])
	assert.Equal(t, 1, agg.people[0]["b.go"])

	// Verify commit count.
	assert.Equal(t, 1, agg.peopleCommits[0])
}

func TestAggregator_Add_NilData(t *testing.T) {
	t.Parallel()

	agg := newAggregator(analyze.AggregatorOptions{}, 1, nil, nil)

	require.NoError(t, agg.Add(analyze.TC{Data: nil}))

	assert.Equal(t, 0, agg.files.Len())
}

func TestAggregator_Add_WrongType(t *testing.T) {
	t.Parallel()

	agg := newAggregator(analyze.AggregatorOptions{}, 1, nil, nil)

	require.NoError(t, agg.Add(analyze.TC{Data: "wrong"}))

	assert.Equal(t, 0, agg.files.Len())
}

func TestAggregator_Add_EmptyCouplingFiles(t *testing.T) {
	t.Parallel()

	agg := newAggregator(analyze.AggregatorOptions{}, 1, nil, nil)

	tc := analyze.TC{
		AuthorID: 0,
		Data: &CommitData{
			CouplingFiles: nil,
			AuthorFiles:   map[string]int{"deleted.go": 1},
			CommitCounted: true,
		},
	}

	require.NoError(t, agg.Add(tc))

	// No coupling entries.
	assert.Equal(t, 0, agg.files.Len())

	// But people entry should exist.
	assert.Equal(t, 1, agg.people[0]["deleted.go"])
	assert.Equal(t, 1, agg.peopleCommits[0])
}

func TestAggregator_Add_PeopleGrowth(t *testing.T) {
	t.Parallel()

	agg := newAggregator(analyze.AggregatorOptions{}, 1, nil, nil)

	tc := analyze.TC{
		AuthorID: 5, // Exceeds initial capacity of 2 (PeopleNumber+1).
		Data: &CommitData{
			AuthorFiles:   map[string]int{"x.go": 1},
			CommitCounted: true,
		},
	}

	require.NoError(t, agg.Add(tc))

	assert.Equal(t, 1, agg.people[5]["x.go"])
	assert.Equal(t, 1, agg.peopleCommits[5])
}

func TestAggregator_Add_CommitNotCounted(t *testing.T) {
	t.Parallel()

	agg := newAggregator(analyze.AggregatorOptions{}, 1, nil, nil)

	tc := analyze.TC{
		AuthorID: 0,
		Data: &CommitData{
			CouplingFiles: []string{"a.go"},
			AuthorFiles:   map[string]int{"a.go": 1},
			CommitCounted: false,
		},
	}

	require.NoError(t, agg.Add(tc))

	assert.Equal(t, 0, agg.peopleCommits[0])
	assert.Equal(t, 1, agg.people[0]["a.go"])
}

func TestAggregator_FlushTick_Empty(t *testing.T) {
	t.Parallel()

	agg := newAggregator(analyze.AggregatorOptions{}, 1, nil, nil)

	tick, err := agg.FlushTick(0)
	require.NoError(t, err)

	assert.Equal(t, 0, tick.Tick)
	assert.Nil(t, tick.Data)
}

func TestAggregator_FlushTick_WithData(t *testing.T) {
	t.Parallel()

	agg := newAggregator(analyze.AggregatorOptions{}, 1, nil, nil)

	require.NoError(t, agg.Add(analyze.TC{
		AuthorID: 0,
		Data: &CommitData{
			CouplingFiles: []string{"a.go", "b.go"},
			AuthorFiles:   map[string]int{"a.go": 1, "b.go": 1},
			CommitCounted: true,
			Renames:       []RenamePair{{FromName: "old.go", ToName: "a.go"}},
		},
	}))

	tick, err := agg.FlushTick(0)
	require.NoError(t, err)

	td, ok := tick.Data.(*TickData)
	require.True(t, ok)
	assert.NotEmpty(t, td.Files)
	assert.NotEmpty(t, td.People)
	assert.NotEmpty(t, td.Renames)
	assert.Equal(t, 1, td.PeopleCommits[0])
}

func TestAggregator_SpillCollect(t *testing.T) {
	t.Parallel()

	agg := newAggregator(analyze.AggregatorOptions{}, 1, nil, nil)

	require.NoError(t, agg.Add(analyze.TC{
		AuthorID: 0,
		Data: &CommitData{
			CouplingFiles: []string{"a.go", "b.go"},
			AuthorFiles:   map[string]int{"a.go": 1, "b.go": 1},
			CommitCounted: true,
		},
	}))

	// Spill.
	freed, err := agg.Spill()
	require.NoError(t, err)
	assert.Positive(t, freed)
	assert.Equal(t, 0, agg.files.Len())

	// Add more data.
	require.NoError(t, agg.Add(analyze.TC{
		AuthorID: 0,
		Data: &CommitData{
			CouplingFiles: []string{"a.go", "c.go"},
			AuthorFiles:   map[string]int{"a.go": 1, "c.go": 1},
			CommitCounted: true,
		},
	}))

	// Collect.
	require.NoError(t, agg.Collect())

	// Verify merged data.
	aLane, ok := agg.files.Get("a.go")
	require.True(t, ok)
	assert.Equal(t, 2, aLane["a.go"])
	assert.Equal(t, 1, aLane["b.go"])
	assert.Equal(t, 1, aLane["c.go"])
}

func TestAggregator_AutoSpill(t *testing.T) {
	t.Parallel()

	agg := newAggregator(analyze.AggregatorOptions{SpillBudget: 1}, 1, nil, nil)

	require.NoError(t, agg.Add(analyze.TC{
		AuthorID: 0,
		Data: &CommitData{
			CouplingFiles: []string{"a.go", "b.go"},
			AuthorFiles:   map[string]int{"a.go": 1, "b.go": 1},
			CommitCounted: true,
		},
	}))

	// With SpillBudget=1, auto-spill should have triggered.
	assert.Equal(t, 0, agg.files.Len())
	assert.Equal(t, 1, agg.files.SpillCount())
}

func TestAggregator_Close_Idempotent(t *testing.T) {
	t.Parallel()

	agg := newAggregator(analyze.AggregatorOptions{}, 1, nil, nil)

	require.NoError(t, agg.Close())
	require.NoError(t, agg.Close())
}

func TestAggregator_EstimatedStateSize(t *testing.T) {
	t.Parallel()

	agg := newAggregator(analyze.AggregatorOptions{}, 1, nil, nil)

	// Initial size accounts for the pre-allocated peopleCommits slice.
	initialSize := agg.EstimatedStateSize()

	require.NoError(t, agg.Add(analyze.TC{
		AuthorID: 0,
		Data: &CommitData{
			CouplingFiles: []string{"a.go", "b.go"},
			AuthorFiles:   map[string]int{"a.go": 1},
			CommitCounted: true,
		},
	}))

	assert.Greater(t, agg.EstimatedStateSize(), initialSize)
}

func TestTicksToReport_MergesTicks(t *testing.T) {
	t.Parallel()

	ticks := []analyze.TICK{
		{
			Tick: 0,
			Data: &TickData{
				Files: map[string]map[string]int{
					"a.go": {"b.go": 2, "a.go": 3},
					"b.go": {"a.go": 2, "b.go": 3},
				},
				People:        []map[string]int{{"a.go": 5, "b.go": 3}, {"a.go": 2}},
				PeopleCommits: []int{10, 5},
				Renames:       []RenamePair{{FromName: "old.go", ToName: "a.go"}},
			},
		},
		{
			Tick: 1,
			Data: &TickData{
				Files: map[string]map[string]int{
					"a.go": {"c.go": 1, "a.go": 1},
					"c.go": {"a.go": 1, "c.go": 1},
				},
				People:        []map[string]int{{"c.go": 2}, {}},
				PeopleCommits: []int{3, 1},
			},
		},
	}

	report := ticksToReport(context.Background(), ticks, []string{"alice", "bob"}, 1, nil)

	// Verify report contains expected keys.
	assert.Contains(t, report, "PeopleMatrix")
	assert.Contains(t, report, "PeopleFiles")
	assert.Contains(t, report, "Files")
	assert.Contains(t, report, "FilesLines")
	assert.Contains(t, report, "FilesMatrix")
	assert.Contains(t, report, "ReversedPeopleDict")

	files, ok := report["Files"].([]string)
	require.True(t, ok)
	assert.NotEmpty(t, files)

	names, ok := report["ReversedPeopleDict"].([]string)
	require.True(t, ok)
	assert.Equal(t, []string{"alice", "bob"}, names)
}

func TestTicksToReport_NilData(t *testing.T) {
	t.Parallel()

	ticks := []analyze.TICK{
		{Tick: 0, Data: nil},
		{Tick: 1, Data: "wrong type"},
	}

	report := ticksToReport(context.Background(), ticks, []string{"dev"}, 0, nil)
	assert.NotNil(t, report)
}

func TestTicksToReport_Empty(t *testing.T) {
	t.Parallel()

	report := ticksToReport(context.Background(), nil, []string{"dev"}, 0, nil)
	assert.NotNil(t, report)
}

func TestMergeFileCouplings(t *testing.T) {
	t.Parallel()

	existing := map[string]int{"a.go": 3, "b.go": 1}
	incoming := map[string]int{"a.go": 2, "c.go": 5}

	result := mergeFileCouplings(existing, incoming)
	assert.Equal(t, 5, result["a.go"])
	assert.Equal(t, 1, result["b.go"])
	assert.Equal(t, 5, result["c.go"])
}

func TestAggregator_MultipleCommits(t *testing.T) {
	t.Parallel()

	agg := newAggregator(analyze.AggregatorOptions{}, 2, []string{"alice", "bob"}, nil)

	// Commit 1: alice touches a.go, b.go.
	require.NoError(t, agg.Add(analyze.TC{
		AuthorID: 0,
		Data: &CommitData{
			CouplingFiles: []string{"a.go", "b.go"},
			AuthorFiles:   map[string]int{"a.go": 1, "b.go": 1},
			CommitCounted: true,
		},
	}))

	// Commit 2: bob touches a.go.
	require.NoError(t, agg.Add(analyze.TC{
		AuthorID: 1,
		Data: &CommitData{
			CouplingFiles: []string{"a.go"},
			AuthorFiles:   map[string]int{"a.go": 1},
			CommitCounted: true,
		},
	}))

	// Verify cumulative state.
	assert.Equal(t, 1, agg.peopleCommits[0])
	assert.Equal(t, 1, agg.peopleCommits[1])
	assert.Equal(t, 1, agg.people[0]["a.go"])
	assert.Equal(t, 1, agg.people[1]["a.go"])

	// a.go self-coupling: 1 (from first commit) + 1 (from second) = 2.
	aLane, _ := agg.files.Get("a.go")
	assert.Equal(t, 2, aLane["a.go"])
}

func TestAggregator_Renames(t *testing.T) {
	t.Parallel()

	agg := newAggregator(analyze.AggregatorOptions{}, 1, nil, nil)

	require.NoError(t, agg.Add(analyze.TC{
		AuthorID: 0,
		Data: &CommitData{
			CouplingFiles: []string{"new.go"},
			AuthorFiles:   map[string]int{"new.go": 1},
			CommitCounted: true,
			Renames:       []RenamePair{{FromName: "old.go", ToName: "new.go"}},
		},
	}))

	require.NoError(t, agg.Add(analyze.TC{
		AuthorID: 0,
		Data: &CommitData{
			Renames: []RenamePair{{FromName: "a.go", ToName: "b.go"}},
		},
	}))

	assert.Len(t, agg.renames, 2)
}

func TestAggregator_Spill_Empty(t *testing.T) {
	t.Parallel()

	agg := newAggregator(analyze.AggregatorOptions{}, 1, nil, nil)

	freed, err := agg.Spill()
	require.NoError(t, err)
	assert.Zero(t, freed)
}
