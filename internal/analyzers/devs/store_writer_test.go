package devs

// FRD: specs/frds/FRD-20260301-all-analyzers-store-based.md.

import (
	"context"
	"testing"
	"time"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	pkgplumbing "github.com/Sumatoshi-tech/codefang/internal/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

const testAnalyzerID = "history/devs"

const (
	storeTestHashC = "cccccccccccccccccccccccccccccccccccccccc"
	storeTestHashD = "dddddddddddddddddddddddddddddddddddddd"
)

func buildTestDevTicks() (ticks []analyze.TICK, commitsByTick map[int][]gitlib.Hash) {
	ticks = []analyze.TICK{
		{
			Tick: 0,
			Data: &TickDevData{
				DevData: map[string]*CommitDevData{
					testHashA: {
						Commits: 1, Added: 10, Removed: 5, Changed: 3, AuthorID: 0,
						Languages: map[string]pkgplumbing.LineStats{
							"Go": {Added: 10, Removed: 5, Changed: 3},
						},
					},
					testHashB: {
						Commits: 1, Added: 20, Removed: 3, Changed: 2, AuthorID: 1,
						Languages: map[string]pkgplumbing.LineStats{
							"Python": {Added: 20, Removed: 3, Changed: 2},
						},
					},
				},
			},
		},
		{
			Tick: 1,
			Data: &TickDevData{
				DevData: map[string]*CommitDevData{
					storeTestHashC: {
						Commits: 1, Added: 15, Removed: 8, Changed: 4, AuthorID: 0,
						Languages: map[string]pkgplumbing.LineStats{
							"Go": {Added: 15, Removed: 8, Changed: 4},
						},
					},
					storeTestHashD: {
						Commits: 1, Added: 5, Removed: 2, Changed: 1, AuthorID: 1,
						Languages: map[string]pkgplumbing.LineStats{
							"Go": {Added: 5, Removed: 2, Changed: 1},
						},
					},
				},
			},
		},
	}

	commitsByTick = map[int][]gitlib.Hash{
		0: {gitlib.NewHash(testHashA), gitlib.NewHash(testHashB)},
		1: {gitlib.NewHash(storeTestHashC), gitlib.NewHash(storeTestHashD)},
	}

	return ticks, commitsByTick
}

func newTestAnalyzer(commitsByTick map[int][]gitlib.Hash) *Analyzer {
	return &Analyzer{
		reversedPeopleDict: []string{"Alice", "Bob"},
		tickSize:           defaultHoursPerDay * time.Hour,
		commitsByTick:      commitsByTick,
	}
}

func TestWriteToStore_RoundTrip(t *testing.T) {
	t.Parallel()

	ticks, commitsByTick := buildTestDevTicks()

	// Compute expected values via the in-memory path to avoid hash round-trip assumptions.
	ctx := context.Background()
	names := []string{"Alice", "Bob"}
	tickSize := time.Duration(defaultHoursPerDay) * time.Hour
	refReport := ticksToReport(ctx, ticks, commitsByTick, names, tickSize, false)
	refMetrics, refErr := ComputeAllMetrics(refReport)
	require.NoError(t, refErr)

	storeDir := t.TempDir()
	store := analyze.NewFileReportStore(storeDir)

	analyzer := newTestAnalyzer(commitsByTick)

	meta := analyze.ReportMeta{AnalyzerID: testAnalyzerID}
	w, beginErr := store.Begin(testAnalyzerID, meta)
	require.NoError(t, beginErr)

	writeErr := analyzer.WriteToStore(ctx, ticks, w)
	require.NoError(t, writeErr)
	require.NoError(t, w.Close())
	require.NoError(t, store.Close())

	// Read back.
	readStore := analyze.NewFileReportStore(storeDir)
	defer readStore.Close()

	reader, openErr := readStore.Open(testAnalyzerID)
	require.NoError(t, openErr)

	defer reader.Close()

	// Verify developer records.
	developers, devErr := readDevelopersIfPresent(reader, reader.Kinds())
	require.NoError(t, devErr)
	require.Len(t, developers, len(refMetrics.Developers))

	// Developers should have at least 1 commit each.
	devNames := make(map[string]bool, len(developers))
	for _, d := range developers {
		devNames[d.Name] = true
		assert.Positive(t, d.Commits)
	}

	assert.True(t, devNames["Alice"])
	assert.True(t, devNames["Bob"])

	// Verify activity records.
	activity, actErr := readActivityIfPresent(reader, reader.Kinds())
	require.NoError(t, actErr)
	require.Len(t, activity, len(refMetrics.Activity))

	assert.Equal(t, 0, activity[0].Tick)
	assert.Equal(t, 1, activity[1].Tick)

	// Verify aggregate record matches reference.
	agg, aggErr := readAggregateIfPresent(reader, reader.Kinds())
	require.NoError(t, aggErr)

	assert.Equal(t, refMetrics.Aggregate.TotalCommits, agg.TotalCommits)
	assert.Equal(t, refMetrics.Aggregate.TotalDevelopers, agg.TotalDevelopers)
	assert.Positive(t, agg.TotalCommits)
	assert.Positive(t, agg.TotalDevelopers)
}

func TestWriteToStore_EmptyTicks(t *testing.T) {
	t.Parallel()

	storeDir := t.TempDir()
	store := analyze.NewFileReportStore(storeDir)

	analyzer := newTestAnalyzer(nil)

	meta := analyze.ReportMeta{AnalyzerID: testAnalyzerID}
	w, beginErr := store.Begin(testAnalyzerID, meta)
	require.NoError(t, beginErr)

	ctx := context.Background()
	writeErr := analyzer.WriteToStore(ctx, nil, w)
	require.NoError(t, writeErr)
	require.NoError(t, w.Close())
	require.NoError(t, store.Close())

	readStore := analyze.NewFileReportStore(storeDir)
	defer readStore.Close()

	reader, openErr := readStore.Open(testAnalyzerID)
	require.NoError(t, openErr)

	defer reader.Close()

	developers, devErr := readDevelopersIfPresent(reader, reader.Kinds())
	require.NoError(t, devErr)
	assert.Empty(t, developers)
}

func TestWriteToStore_EquivalenceReference(t *testing.T) {
	t.Parallel()

	ticks, commitsByTick := buildTestDevTicks()

	// Reference path: ticksToReport → ComputeAllMetrics.
	ctx := context.Background()
	names := []string{"Alice", "Bob"}
	tickSize := time.Duration(defaultHoursPerDay) * time.Hour
	refReport := ticksToReport(ctx, ticks, commitsByTick, names, tickSize, false)
	refMetrics, metricsErr := ComputeAllMetrics(refReport)
	require.NoError(t, metricsErr)

	// Store path.
	storeDir := t.TempDir()
	store := analyze.NewFileReportStore(storeDir)

	analyzer := newTestAnalyzer(commitsByTick)

	meta := analyze.ReportMeta{AnalyzerID: testAnalyzerID}
	w, beginErr := store.Begin(testAnalyzerID, meta)
	require.NoError(t, beginErr)

	writeErr := analyzer.WriteToStore(ctx, ticks, w)
	require.NoError(t, writeErr)
	require.NoError(t, w.Close())
	require.NoError(t, store.Close())

	readStore := analyze.NewFileReportStore(storeDir)
	defer readStore.Close()

	reader, openErr := readStore.Open(testAnalyzerID)
	require.NoError(t, openErr)

	defer reader.Close()

	// Compare developers.
	storeDevelopers, devErr := readDevelopersIfPresent(reader, reader.Kinds())
	require.NoError(t, devErr)
	require.Len(t, storeDevelopers, len(refMetrics.Developers))

	for i := range refMetrics.Developers {
		assert.Equal(t, refMetrics.Developers[i].Name, storeDevelopers[i].Name)
		assert.Equal(t, refMetrics.Developers[i].Commits, storeDevelopers[i].Commits)
		assert.Equal(t, refMetrics.Developers[i].Added, storeDevelopers[i].Added)
		assert.Equal(t, refMetrics.Developers[i].Removed, storeDevelopers[i].Removed)
	}

	// Compare aggregate.
	storeAgg, aggErr := readAggregateIfPresent(reader, reader.Kinds())
	require.NoError(t, aggErr)
	assert.Equal(t, refMetrics.Aggregate.TotalCommits, storeAgg.TotalCommits)
	assert.Equal(t, refMetrics.Aggregate.TotalDevelopers, storeAgg.TotalDevelopers)
	assert.Equal(t, refMetrics.Aggregate.TotalLinesAdded, storeAgg.TotalLinesAdded)
	assert.Equal(t, refMetrics.Aggregate.TotalLinesRemoved, storeAgg.TotalLinesRemoved)

	// Compare activity.
	storeActivity, actErr := readActivityIfPresent(reader, reader.Kinds())
	require.NoError(t, actErr)
	require.Len(t, storeActivity, len(refMetrics.Activity))

	for i := range refMetrics.Activity {
		assert.Equal(t, refMetrics.Activity[i].Tick, storeActivity[i].Tick)
		assert.Equal(t, refMetrics.Activity[i].TotalCommits, storeActivity[i].TotalCommits)
	}

	// Compare churn.
	storeChurn, churnErr := readChurnIfPresent(reader, reader.Kinds())
	require.NoError(t, churnErr)
	require.Len(t, storeChurn, len(refMetrics.Churn))

	for i := range refMetrics.Churn {
		assert.Equal(t, refMetrics.Churn[i].Tick, storeChurn[i].Tick)
		assert.Equal(t, refMetrics.Churn[i].Added, storeChurn[i].Added)
		assert.Equal(t, refMetrics.Churn[i].Removed, storeChurn[i].Removed)
	}
}

func TestGenerateStoreSections_RoundTrip(t *testing.T) {
	t.Parallel()

	ticks, commitsByTick := buildTestDevTicks()

	storeDir := t.TempDir()
	store := analyze.NewFileReportStore(storeDir)

	analyzer := newTestAnalyzer(commitsByTick)

	meta := analyze.ReportMeta{AnalyzerID: testAnalyzerID}
	w, beginErr := store.Begin(testAnalyzerID, meta)
	require.NoError(t, beginErr)

	ctx := context.Background()
	writeErr := analyzer.WriteToStore(ctx, ticks, w)
	require.NoError(t, writeErr)
	require.NoError(t, w.Close())
	require.NoError(t, store.Close())

	readStore := analyze.NewFileReportStore(storeDir)
	defer readStore.Close()

	reader, openErr := readStore.Open(testAnalyzerID)
	require.NoError(t, openErr)

	defer reader.Close()

	sections, secErr := GenerateStoreSections(reader)
	require.NoError(t, secErr)

	// Expects single section with tabbed dashboard.
	const expectedSectionCount = 1

	require.Len(t, sections, expectedSectionCount)
	assert.Equal(t, "Developer Analytics", sections[0].Title)
}

func TestGenerateStoreSections_EmptyStore(t *testing.T) {
	t.Parallel()

	storeDir := t.TempDir()
	store := analyze.NewFileReportStore(storeDir)

	meta := analyze.ReportMeta{AnalyzerID: testAnalyzerID}
	w, beginErr := store.Begin(testAnalyzerID, meta)
	require.NoError(t, beginErr)
	require.NoError(t, w.Close())
	require.NoError(t, store.Close())

	readStore := analyze.NewFileReportStore(storeDir)
	defer readStore.Close()

	reader, openErr := readStore.Open(testAnalyzerID)
	require.NoError(t, openErr)

	defer reader.Close()

	sections, secErr := GenerateStoreSections(reader)
	require.NoError(t, secErr)
	assert.Empty(t, sections)
}
