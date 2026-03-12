package couples

import (
	"context"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
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

// --- Tests for ticksToReport people sizing (PeopleNumber=0 regression) ---.

func TestTicksToReport_PeopleNumberZero_IncrementalAuthors(t *testing.T) {
	t.Parallel()

	// Simulates the real scenario: PeopleNumber=0 at Configure time (no --people-dict),
	// but the Aggregator grows people slices as authors are discovered incrementally.
	// ticksToReport must use the actual people count from tick data, not the stale peopleNumber.
	ticks := []analyze.TICK{
		{
			Tick: 0,
			Data: &TickData{
				Files: map[string]map[string]int{
					"a.go": {"a.go": 3, "b.go": 2},
					"b.go": {"a.go": 2, "b.go": 3},
				},
				// 4 authors discovered incrementally (indices 0..3).
				People: []map[string]int{
					{"a.go": 5, "b.go": 3},
					{"a.go": 2},
					{"b.go": 4},
					{"a.go": 1, "b.go": 1},
				},
				PeopleCommits: []int{10, 5, 8, 2},
			},
		},
	}

	names := []string{"alice", "bob", "charlie"}
	report := ticksToReport(context.Background(), ticks, names, 0, nil)

	// Verify all 4 authors made it into the PeopleMatrix (not truncated to 1).
	matrix, ok := report["PeopleMatrix"].([]map[int]int64)
	require.True(t, ok, "PeopleMatrix should be []map[int]int64")
	require.Len(t, matrix, 4, "PeopleMatrix should have 4 entries (one per author)")

	// Verify PeopleFiles has entries for all authors.
	pf, ok := report["PeopleFiles"].([][]int)
	require.True(t, ok, "PeopleFiles should be [][]int")
	require.Len(t, pf, 4, "PeopleFiles should have 4 entries")

	// Alice (idx 0) touched both a.go and b.go → should appear in PeopleFiles[0].
	assert.Len(t, pf[0], 2, "alice should have touched 2 files")

	// Verify cross-author coupling: alice and bob both touched a.go.
	// PeopleMatrix[0][1] should be > 0 (shared file touches).
	assert.Positive(t, matrix[0][1], "alice-bob coupling should be positive (shared a.go)")

	// Verify diagonal: alice self-coupling should reflect her total touches.
	assert.Positive(t, matrix[0][0], "alice self-coupling should be positive")
}

func TestTicksToReport_MultipleTicks_PeopleGrowth(t *testing.T) {
	t.Parallel()

	// Tick 0: 2 authors.
	// Tick 1: 4 authors (grew during processing).
	ticks := []analyze.TICK{
		{
			Tick: 0,
			Data: &TickData{
				Files: map[string]map[string]int{
					"a.go": {"a.go": 2},
				},
				People:        []map[string]int{{"a.go": 3}, {"a.go": 1}},
				PeopleCommits: []int{5, 2},
			},
		},
		{
			Tick: 1,
			Data: &TickData{
				Files: map[string]map[string]int{
					"a.go": {"a.go": 1},
				},
				People:        []map[string]int{{"a.go": 1}, {}, {"a.go": 2}, {"a.go": 1}},
				PeopleCommits: []int{1, 0, 3, 1},
			},
		},
	}

	report := ticksToReport(context.Background(), ticks, []string{"a", "b", "c"}, 1, nil)

	matrix, ok := report["PeopleMatrix"].([]map[int]int64)
	require.True(t, ok)
	// Should have 4 entries (max across ticks), not 2 (from peopleNumber=1 → 1+1=2).
	require.Len(t, matrix, 4)
}

func TestMergeTickPeople_DstSmallerThanSrc(t *testing.T) {
	t.Parallel()

	// dst has 2 slots, src has 4.
	// Should merge the first 2 without panic and silently skip the rest.
	dst := []map[string]int{
		{"a.go": 1},
		{"b.go": 2},
	}
	src := []map[string]int{
		{"a.go": 3},
		{"b.go": 1},
		{"c.go": 5},
		{"d.go": 7},
	}

	mergeTickPeople(dst, src)

	assert.Equal(t, 4, dst[0]["a.go"]) // 1 + 3
	assert.Equal(t, 3, dst[1]["b.go"]) // 2 + 1
}

func TestMergeTickPeople_SrcSmallerThanDst(t *testing.T) {
	t.Parallel()

	dst := []map[string]int{
		{"a.go": 1},
		{"b.go": 2},
		{"c.go": 3},
	}
	src := []map[string]int{
		{"a.go": 10},
	}

	mergeTickPeople(dst, src)

	assert.Equal(t, 11, dst[0]["a.go"]) // 1 + 10
	assert.Equal(t, 2, dst[1]["b.go"])  // Unchanged.
	assert.Equal(t, 3, dst[2]["c.go"])  // Unchanged.
}

func TestTicksToReport_VerifyPeopleMatrixValues(t *testing.T) {
	t.Parallel()

	// Two authors share file a.go. The PeopleMatrix should reflect their coupling.
	ticks := []analyze.TICK{
		{
			Tick: 0,
			Data: &TickData{
				Files: map[string]map[string]int{
					"a.go": {"a.go": 5, "b.go": 2},
					"b.go": {"a.go": 2, "b.go": 3},
				},
				People: []map[string]int{
					{"a.go": 4, "b.go": 2}, // alice: 4 touches on a.go, 2 on b.go.
					{"a.go": 3},            // bob: 3 touches on a.go.
				},
				PeopleCommits: []int{10, 5},
			},
		},
	}

	report := ticksToReport(context.Background(), ticks, []string{"alice", "bob"}, 1, nil)

	matrix, ok := report["PeopleMatrix"].([]map[int]int64)
	require.True(t, ok)
	require.Len(t, matrix, 2)

	// Alice-Bob coupling on a.go: min(4, 3) = 3.
	assert.Equal(t, int64(3), matrix[0][1], "alice-bob coupling = min(4,3) on shared file a.go")
	assert.Equal(t, int64(3), matrix[1][0], "bob-alice coupling should be symmetric")

	// Alice self-coupling: min(4,4) on a.go + min(2,2) on b.go = 4+2 = 6.
	assert.Equal(t, int64(6), matrix[0][0], "alice self-coupling = 4 (a.go) + 2 (b.go)")

	// Bob self-coupling: min(3,3) on a.go = 3.
	assert.Equal(t, int64(3), matrix[1][1], "bob self-coupling = 3 (a.go)")

	// Verify FilesMatrix values.
	fm, ok := report["FilesMatrix"].([]map[int]int64)
	require.True(t, ok)
	require.Len(t, fm, 2)

	// Files should be ["a.go", "b.go"] sorted.
	files, ok := report["Files"].([]string)
	require.True(t, ok)
	assert.Equal(t, []string{"a.go", "b.go"}, files)

	// FilesMatrix[0] (a.go): self=5, coupled with b.go=2.
	assert.Equal(t, int64(5), fm[0][0], "a.go self-coupling")
	assert.Equal(t, int64(2), fm[0][1], "a.go-b.go coupling")
}

func TestComputePeopleMatrix_ThreeAuthorsOverlapping(t *testing.T) {
	t.Parallel()

	// 3 authors with overlapping file touches.
	filesIndex := map[string]int{"a.go": 0, "b.go": 1, "c.go": 2}
	people := []map[string]int{
		{"a.go": 5, "b.go": 3},            // alice: 5 on a, 3 on b.
		{"a.go": 2, "b.go": 4, "c.go": 1}, // bob: 2 on a, 4 on b, 1 on c.
		{"b.go": 6, "c.go": 3},            // charlie: 6 on b, 3 on c.
	}

	matrix, pf := computePeopleMatrix(people, filesIndex, 2)

	require.Len(t, matrix, 3)
	require.Len(t, pf, 3)

	// alice-bob coupling: min(5,2) on a.go + min(3,4) on b.go = 2+3 = 5.
	assert.Equal(t, int64(5), matrix[0][1], "alice-bob coupling")
	assert.Equal(t, int64(5), matrix[1][0], "bob-alice coupling (symmetric)")

	// alice-charlie coupling: min(3,6) on b.go = 3.
	assert.Equal(t, int64(3), matrix[0][2], "alice-charlie coupling (shared b.go)")
	assert.Equal(t, int64(3), matrix[2][0], "charlie-alice coupling (symmetric)")

	// bob-charlie coupling: min(4,6) on b.go + min(1,3) on c.go = 4+1 = 5.
	assert.Equal(t, int64(5), matrix[1][2], "bob-charlie coupling")
	assert.Equal(t, int64(5), matrix[2][1], "charlie-bob coupling (symmetric)")

	// Self couplings.
	// alice: min(5,5) on a + min(3,3) on b = 8.
	assert.Equal(t, int64(8), matrix[0][0], "alice self-coupling")
	// bob: min(2,2) on a + min(4,4) on b + min(1,1) on c = 7.
	assert.Equal(t, int64(7), matrix[1][1], "bob self-coupling")
	// charlie: min(6,6) on b + min(3,3) on c = 9.
	assert.Equal(t, int64(9), matrix[2][2], "charlie self-coupling")

	// PeopleFiles: alice has a.go(0), b.go(1). Sorted.
	assert.Equal(t, []int{0, 1}, pf[0])
	// bob has a.go(0), b.go(1), c.go(2). Sorted.
	assert.Equal(t, []int{0, 1, 2}, pf[1])
	// charlie has b.go(1), c.go(2). Sorted.
	assert.Equal(t, []int{1, 2}, pf[2])
}

func TestEndToEnd_Consume_Aggregator_TicksToReport(t *testing.T) {
	t.Parallel()

	// Simulate the full pipeline: multiple commits → Aggregator → FlushAllTicks → ticksToReport.
	// This is the closest we can get to an integration test without a real git repo.

	// Step 1: Build commit data as Consume would produce.
	commits := []analyze.TC{
		{
			AuthorID: 0,
			Data: &CommitData{
				CouplingFiles: []string{"a.go", "b.go"},
				AuthorFiles:   map[string]int{"a.go": 1, "b.go": 1},
				CommitCounted: true,
			},
		},
		{
			AuthorID: 1,
			Data: &CommitData{
				CouplingFiles: []string{"a.go", "c.go"},
				AuthorFiles:   map[string]int{"a.go": 1, "c.go": 1},
				CommitCounted: true,
			},
		},
		{
			AuthorID: 2,
			Data: &CommitData{
				CouplingFiles: []string{"b.go", "c.go"},
				AuthorFiles:   map[string]int{"b.go": 1, "c.go": 1},
				CommitCounted: true,
			},
		},
		{
			AuthorID: 0,
			Data: &CommitData{
				CouplingFiles: []string{"a.go"},
				AuthorFiles:   map[string]int{"a.go": 1},
				CommitCounted: true,
			},
		},
	}

	// Step 2: Feed through Aggregator (PeopleNumber=0 — no --people-dict).
	agg := newAggregator(analyze.AggregatorOptions{}, 0, nil, nil)
	defer agg.Close()

	for _, tc := range commits {
		require.NoError(t, agg.Add(tc))
	}

	// Step 3: FlushAllTicks.
	ticks, err := agg.FlushAllTicks()
	require.NoError(t, err)
	require.Len(t, ticks, 1)

	// Step 4: ticksToReport (PeopleNumber=0, simulating no --people-dict).
	names := []string{"alice", "bob", "charlie"}
	report := ticksToReport(context.Background(), ticks, names, 0, nil)

	// Verify report structure.
	matrix, ok := report["PeopleMatrix"].([]map[int]int64)
	require.True(t, ok, "PeopleMatrix type assertion")
	require.Len(t, matrix, 3, "should have 3 authors")

	pf, ok := report["PeopleFiles"].([][]int)
	require.True(t, ok, "PeopleFiles type assertion")
	require.Len(t, pf, 3, "should have 3 author entries in PeopleFiles")

	files, ok := report["Files"].([]string)
	require.True(t, ok, "Files type assertion")
	assert.ElementsMatch(t, []string{"a.go", "b.go", "c.go"}, files)

	// Verify coupling values.
	// Alice touched a.go twice, b.go once. Bob touched a.go once, c.go once.
	// Alice-Bob coupling: shared a.go → min(2,1)=1.
	assert.Equal(t, int64(1), matrix[0][1], "alice-bob coupling via shared a.go")

	// Alice self-coupling: min(2,2) on a.go + min(1,1) on b.go = 3.
	assert.Equal(t, int64(3), matrix[0][0], "alice self-coupling")

	// Bob self: min(1,1) on a + min(1,1) on c = 2.
	assert.Equal(t, int64(2), matrix[1][1], "bob self-coupling")

	// Charlie self: min(1,1) on b + min(1,1) on c = 2.
	assert.Equal(t, int64(2), matrix[2][2], "charlie self-coupling")

	// Bob-Charlie: shared c.go → min(1,1) = 1.
	assert.Equal(t, int64(1), matrix[1][2], "bob-charlie coupling via shared c.go")

	// Verify the report can be consumed by ComputeAllMetrics.
	metrics, err := ComputeAllMetrics(report)
	require.NoError(t, err)
	assert.Equal(t, 3, metrics.Aggregate.TotalFiles)
	assert.Len(t, metrics.DeveloperCoupling, 3, "should have 3 developer coupling pairs")
}
