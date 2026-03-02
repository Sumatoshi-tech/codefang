package imports

// FRD: specs/frds/FRD-20260301-all-analyzers-store-based.md.

import (
	"context"
	"sort"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
)

func buildTestImportTicks() []analyze.TICK {
	return []analyze.TICK{
		{
			Tick: 0,
			Data: &TickData{
				Imports: Map{
					0: { // author 0.
						"go": {
							"fmt":     {0: 3},
							"os":      {0: 2},
							"strings": {0: 1},
						},
					},
				},
			},
		},
		{
			Tick: 1,
			Data: &TickData{
				Imports: Map{
					0: {
						"go": {
							"fmt": {1: 5},
							"io":  {1: 1},
						},
					},
					1: { // author 1.
						"go": {
							"fmt": {1: 2},
							"os":  {1: 3},
						},
					},
				},
			},
		},
	}
}

const testAnalyzerID = "history/imports"

func TestWriteToStore_RoundTrip(t *testing.T) {
	t.Parallel()

	ticks := buildTestImportTicks()

	storeDir := t.TempDir()
	store := analyze.NewFileReportStore(storeDir)

	analyzer := &HistoryAnalyzer{}

	meta := analyze.ReportMeta{AnalyzerID: testAnalyzerID}
	w, beginErr := store.Begin(testAnalyzerID, meta)
	require.NoError(t, beginErr)

	ctx := context.Background()
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

	records, readRecErr := readImportUsageIfPresent(reader, reader.Kinds())
	require.NoError(t, readRecErr)
	require.NotEmpty(t, records)

	// "fmt" should have the highest count: 3 + 5 + 2 = 10.
	assert.Equal(t, "fmt", records[0].Import)

	const expectedFmtCount = 10

	assert.Equal(t, int64(expectedFmtCount), records[0].Count)

	// "os" = 2 + 3 = 5.
	const expectedOsCount = 5

	foundOS := false

	for _, rec := range records {
		if rec.Import == "os" {
			foundOS = true

			assert.Equal(t, int64(expectedOsCount), rec.Count)
		}
	}

	assert.True(t, foundOS, "expected 'os' import record")
}

func TestWriteToStore_EmptyTicks(t *testing.T) {
	t.Parallel()

	storeDir := t.TempDir()
	store := analyze.NewFileReportStore(storeDir)

	analyzer := &HistoryAnalyzer{}

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

	records, readErr := readImportUsageIfPresent(reader, reader.Kinds())
	require.NoError(t, readErr)
	assert.Empty(t, records)
}

func TestWriteToStore_EquivalenceReference(t *testing.T) {
	t.Parallel()

	ticks := buildTestImportTicks()

	// Reference path: build merged Map, aggregate counts, topImports.
	merged := Map{}

	for _, tick := range ticks {
		td, ok := tick.Data.(*TickData)
		if !ok || td == nil {
			continue
		}

		mergeImportMaps(merged, td.Imports)
	}

	refCounts := aggregateImportCounts(merged)
	refLabels, refData := topImports(refCounts)

	// Store path.
	storeDir := t.TempDir()
	store := analyze.NewFileReportStore(storeDir)

	analyzer := &HistoryAnalyzer{}

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

	storeRecords, readErr := readImportUsageIfPresent(reader, reader.Kinds())
	require.NoError(t, readErr)

	require.Len(t, storeRecords, len(refLabels))

	// Sort both by import name for deterministic comparison.
	type labelCount struct {
		label string
		count int64
	}

	refPairs := make([]labelCount, len(refLabels))
	for i := range refLabels {
		refPairs[i] = labelCount{refLabels[i], int64(refData[i])}
	}

	sort.Slice(refPairs, func(i, j int) bool {
		return refPairs[i].label < refPairs[j].label
	})

	storePairs := make([]labelCount, len(storeRecords))
	for i, rec := range storeRecords {
		storePairs[i] = labelCount{rec.Import, rec.Count}
	}

	sort.Slice(storePairs, func(i, j int) bool {
		return storePairs[i].label < storePairs[j].label
	})

	for i := range refPairs {
		assert.Equal(t, refPairs[i].label, storePairs[i].label)
		assert.Equal(t, refPairs[i].count, storePairs[i].count)
	}
}

func TestGenerateStoreSections_RoundTrip(t *testing.T) {
	t.Parallel()

	ticks := buildTestImportTicks()

	storeDir := t.TempDir()
	store := analyze.NewFileReportStore(storeDir)

	analyzer := &HistoryAnalyzer{}

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
	require.Len(t, sections, 1)
	assert.Equal(t, "Top Imports Usage", sections[0].Title)
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
