package typos

// FRD: specs/frds/FRD-20260301-all-analyzers-store-based.md.

import (
	"context"
	"sort"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

func buildTestTypoTicks() []analyze.TICK {
	return []analyze.TICK{
		{
			Tick: 0,
			Data: &TickData{
				Typos: []Typo{
					{Wrong: "tets", Correct: "test", File: "main.go", Commit: gitlib.NewHash("aaaa"), Line: 5},
					{Wrong: "functon", Correct: "function", File: "main.go", Commit: gitlib.NewHash("aaaa"), Line: 10},
					{Wrong: "retrun", Correct: "return", File: "util.go", Commit: gitlib.NewHash("bbbb"), Line: 3},
				},
			},
		},
		{
			Tick: 1,
			Data: &TickData{
				Typos: []Typo{
					{Wrong: "tets", Correct: "test", File: "lib.go", Commit: gitlib.NewHash("cccc"), Line: 7},
					{Wrong: "lengthh", Correct: "length", File: "lib.go", Commit: gitlib.NewHash("cccc"), Line: 12},
				},
			},
		},
	}
}

func TestWriteToStore_RoundTrip(t *testing.T) {
	t.Parallel()

	ticks := buildTestTypoTicks()

	storeDir := t.TempDir()
	store := analyze.NewFileReportStore(storeDir)

	analyzer := &Analyzer{}

	const analyzerID = "history/typos"

	meta := analyze.ReportMeta{AnalyzerID: analyzerID}
	w, beginErr := store.Begin(analyzerID, meta)
	require.NoError(t, beginErr)

	ctx := context.Background()
	writeErr := analyzer.WriteToStore(ctx, ticks, w)
	require.NoError(t, writeErr)
	require.NoError(t, w.Close())
	require.NoError(t, store.Close())

	// Read back.
	readStore := analyze.NewFileReportStore(storeDir)
	defer readStore.Close()

	reader, openErr := readStore.Open(analyzerID)
	require.NoError(t, openErr)

	defer reader.Close()

	// Verify file_typos records.
	fileTypos, readTypoErr := readFileTyposIfPresent(reader, reader.Kinds())
	require.NoError(t, readTypoErr)

	// 3 unique files: main.go, util.go, lib.go.
	const expectedFileCount = 3

	require.Len(t, fileTypos, expectedFileCount)

	// Should be sorted by typo count descending.
	for i := 1; i < len(fileTypos); i++ {
		assert.GreaterOrEqual(t, fileTypos[i-1].TypoCount, fileTypos[i].TypoCount)
	}

	// main.go has 2 typos, should be first.
	assert.Equal(t, "main.go", fileTypos[0].File)
	assert.Equal(t, 2, fileTypos[0].TypoCount)

	// Verify aggregate record.
	var agg AggregateData

	aggErr := reader.Iter(KindAggregate, func(raw []byte) error {
		return analyze.GobDecode(raw, &agg)
	})
	require.NoError(t, aggErr)

	// "tets|test" is deduplicated cross-tick, so 4 unique typos.
	const expectedTotalTypos = 4

	assert.Equal(t, expectedTotalTypos, agg.TotalTypos)
	assert.Equal(t, expectedFileCount, agg.AffectedFiles)
	assert.Positive(t, agg.UniquePatterns)
}

func TestWriteToStore_EmptyTicks(t *testing.T) {
	t.Parallel()

	storeDir := t.TempDir()
	store := analyze.NewFileReportStore(storeDir)

	analyzer := &Analyzer{}

	const analyzerID = "history/typos"

	meta := analyze.ReportMeta{AnalyzerID: analyzerID}
	w, beginErr := store.Begin(analyzerID, meta)
	require.NoError(t, beginErr)

	ctx := context.Background()
	writeErr := analyzer.WriteToStore(ctx, nil, w)
	require.NoError(t, writeErr)
	require.NoError(t, w.Close())
	require.NoError(t, store.Close())

	readStore := analyze.NewFileReportStore(storeDir)
	defer readStore.Close()

	reader, openErr := readStore.Open(analyzerID)
	require.NoError(t, openErr)

	defer reader.Close()

	fileTypos, readErr := readFileTyposIfPresent(reader, reader.Kinds())
	require.NoError(t, readErr)
	assert.Empty(t, fileTypos)
}

func TestWriteToStore_EquivalenceReference(t *testing.T) {
	t.Parallel()

	ticks := buildTestTypoTicks()

	// Reference path: ticksToReport → ComputeAllMetrics.
	ctx := context.Background()
	refReport := ticksToReport(ctx, ticks)
	refMetrics, metricsErr := ComputeAllMetrics(refReport)
	require.NoError(t, metricsErr)

	// Store path.
	storeDir := t.TempDir()
	store := analyze.NewFileReportStore(storeDir)

	analyzer := &Analyzer{}

	const analyzerID = "history/typos"

	meta := analyze.ReportMeta{AnalyzerID: analyzerID}
	w, beginErr := store.Begin(analyzerID, meta)
	require.NoError(t, beginErr)

	writeErr := analyzer.WriteToStore(ctx, ticks, w)
	require.NoError(t, writeErr)
	require.NoError(t, w.Close())
	require.NoError(t, store.Close())

	readStore := analyze.NewFileReportStore(storeDir)
	defer readStore.Close()

	reader, openErr := readStore.Open(analyzerID)
	require.NoError(t, openErr)

	defer reader.Close()

	storeFileTypos, readErr := readFileTyposIfPresent(reader, reader.Kinds())
	require.NoError(t, readErr)

	require.Len(t, storeFileTypos, len(refMetrics.FileTypos))

	// Sort both by file name for deterministic comparison.
	refFileTypos := refMetrics.FileTypos
	sort.Slice(refFileTypos, func(i, j int) bool {
		return refFileTypos[i].File < refFileTypos[j].File
	})

	sort.Slice(storeFileTypos, func(i, j int) bool {
		return storeFileTypos[i].File < storeFileTypos[j].File
	})

	for i := range refFileTypos {
		assert.Equal(t, refFileTypos[i].File, storeFileTypos[i].File)
		assert.Equal(t, refFileTypos[i].TypoCount, storeFileTypos[i].TypoCount)
	}

	// Compare aggregate.
	var storeAgg AggregateData

	aggErr := reader.Iter(KindAggregate, func(raw []byte) error {
		return analyze.GobDecode(raw, &storeAgg)
	})
	require.NoError(t, aggErr)

	assert.Equal(t, refMetrics.Aggregate.TotalTypos, storeAgg.TotalTypos)
	assert.Equal(t, refMetrics.Aggregate.AffectedFiles, storeAgg.AffectedFiles)
	assert.Equal(t, refMetrics.Aggregate.UniquePatterns, storeAgg.UniquePatterns)
}

func TestGenerateStoreSections_RoundTrip(t *testing.T) {
	t.Parallel()

	ticks := buildTestTypoTicks()

	storeDir := t.TempDir()
	store := analyze.NewFileReportStore(storeDir)

	analyzer := &Analyzer{}

	const analyzerID = "history/typos"

	meta := analyze.ReportMeta{AnalyzerID: analyzerID}
	w, beginErr := store.Begin(analyzerID, meta)
	require.NoError(t, beginErr)

	ctx := context.Background()
	writeErr := analyzer.WriteToStore(ctx, ticks, w)
	require.NoError(t, writeErr)
	require.NoError(t, w.Close())
	require.NoError(t, store.Close())

	readStore := analyze.NewFileReportStore(storeDir)
	defer readStore.Close()

	reader, openErr := readStore.Open(analyzerID)
	require.NoError(t, openErr)

	defer reader.Close()

	sections, secErr := GenerateStoreSections(reader)
	require.NoError(t, secErr)
	require.Len(t, sections, 1)
	assert.Equal(t, "Typo-Prone Files", sections[0].Title)
}

func TestGenerateStoreSections_EmptyStore(t *testing.T) {
	t.Parallel()

	storeDir := t.TempDir()
	store := analyze.NewFileReportStore(storeDir)

	const analyzerID = "history/typos"

	meta := analyze.ReportMeta{AnalyzerID: analyzerID}
	w, beginErr := store.Begin(analyzerID, meta)
	require.NoError(t, beginErr)
	require.NoError(t, w.Close())
	require.NoError(t, store.Close())

	readStore := analyze.NewFileReportStore(storeDir)
	defer readStore.Close()

	reader, openErr := readStore.Open(analyzerID)
	require.NoError(t, openErr)

	defer reader.Close()

	sections, secErr := GenerateStoreSections(reader)
	require.NoError(t, secErr)
	assert.Empty(t, sections)
}
