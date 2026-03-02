package shotness

// FRD: specs/frds/FRD-20260301-all-analyzers-store-based.md.

import (
	"context"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
)

const testAnalyzerID = "history/shotness"

// readAggregateForTest reads the single aggregate record for test verification.
func readAggregateForTest(t *testing.T, reader analyze.ReportReader) AggregateData {
	t.Helper()

	var agg AggregateData

	iterErr := reader.Iter(KindAggregate, func(raw []byte) error {
		return analyze.GobDecode(raw, &agg)
	})

	require.NoError(t, iterErr)

	return agg
}

func buildTestShotnessTicks() []analyze.TICK {
	return []analyze.TICK{
		{
			Tick: 0,
			Data: &TickData{
				Nodes: map[string]*nodeShotnessData{
					"Function_foo_main.go": {
						Summary: NodeSummary{Type: "Function", Name: "foo", File: "main.go"},
						Count:   5,
						Couples: map[string]int{
							"Function_bar_main.go": 3,
						},
					},
					"Function_bar_main.go": {
						Summary: NodeSummary{Type: "Function", Name: "bar", File: "main.go"},
						Count:   4,
						Couples: map[string]int{
							"Function_foo_main.go": 3,
						},
					},
				},
				CommitStats: map[string]*CommitSummary{},
			},
		},
		{
			Tick: 1,
			Data: &TickData{
				Nodes: map[string]*nodeShotnessData{
					"Function_foo_main.go": {
						Summary: NodeSummary{Type: "Function", Name: "foo", File: "main.go"},
						Count:   2,
						Couples: map[string]int{
							"Function_baz_util.go": 1,
						},
					},
					"Function_baz_util.go": {
						Summary: NodeSummary{Type: "Function", Name: "baz", File: "util.go"},
						Count:   3,
						Couples: map[string]int{
							"Function_foo_main.go": 1,
						},
					},
				},
				CommitStats: map[string]*CommitSummary{},
			},
		},
	}
}

func TestWriteToStore_RoundTrip(t *testing.T) {
	t.Parallel()

	ticks := buildTestShotnessTicks()

	storeDir := t.TempDir()
	store := analyze.NewFileReportStore(storeDir)

	analyzer := &Analyzer{}

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

	// Verify node_data records.
	records, nodeErr := readNodeDataIfPresent(reader, reader.Kinds())
	require.NoError(t, nodeErr)

	// 3 distinct nodes: foo, bar, baz.
	const expectedNodeCount = 3

	require.Len(t, records, expectedNodeCount)

	// Build name→record map for verification.
	byName := make(map[string]NodeStoreRecord, len(records))
	for _, rec := range records {
		byName[rec.Summary.Name] = rec
	}

	// foo: count 5+2=7, coupled to bar(3) and baz(1).
	fooRec, hasFoo := byName["foo"]
	require.True(t, hasFoo)
	assert.Equal(t, "main.go", fooRec.Summary.File)

	// bar: count 4.
	barRec, hasBar := byName["bar"]
	require.True(t, hasBar)
	assert.Equal(t, "main.go", barRec.Summary.File)

	// baz: count 3.
	bazRec, hasBaz := byName["baz"]
	require.True(t, hasBaz)
	assert.Equal(t, "util.go", bazRec.Summary.File)

	// Self counts via counter diagonal.
	for _, rec := range records {
		// Find this node's index.
		for i, r := range records {
			if r.Summary.Name == rec.Summary.Name {
				assert.Positive(t, rec.Counter[i], "self count for %s", rec.Summary.Name)

				break
			}
		}
	}

	// Verify aggregate record.
	agg := readAggregateForTest(t, reader)
	assert.Equal(t, expectedNodeCount, agg.TotalNodes)
	assert.Positive(t, agg.TotalChanges)

	_ = fooRec
	_ = barRec
	_ = bazRec
}

func TestWriteToStore_EmptyTicks(t *testing.T) {
	t.Parallel()

	storeDir := t.TempDir()
	store := analyze.NewFileReportStore(storeDir)

	analyzer := &Analyzer{}

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

	records, nodeErr := readNodeDataIfPresent(reader, reader.Kinds())
	require.NoError(t, nodeErr)
	assert.Empty(t, records)
}

func TestWriteToStore_EquivalenceReference(t *testing.T) {
	t.Parallel()

	ticks := buildTestShotnessTicks()

	// Reference path: ticksToReport → extractShotnessData.
	ctx := context.Background()
	refReport := ticksToReport(ctx, ticks)
	refNodes, refCounters, extractErr := extractShotnessData(refReport)
	require.NoError(t, extractErr)

	// Store path.
	storeDir := t.TempDir()
	store := analyze.NewFileReportStore(storeDir)

	analyzer := &Analyzer{}

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

	records, nodeErr := readNodeDataIfPresent(reader, reader.Kinds())
	require.NoError(t, nodeErr)

	require.Len(t, records, len(refNodes))

	// Reconstruct nodes and counters from store.
	storeNodes := make([]NodeSummary, len(records))
	storeCounters := make([]map[int]int, len(records))

	for i, rec := range records {
		storeNodes[i] = rec.Summary
		storeCounters[i] = rec.Counter
	}

	// Compare nodes.
	for i := range refNodes {
		assert.Equal(t, refNodes[i].Name, storeNodes[i].Name)
		assert.Equal(t, refNodes[i].Type, storeNodes[i].Type)
		assert.Equal(t, refNodes[i].File, storeNodes[i].File)
	}

	// Compare counters (self counts and coupling).
	for i := range refCounters {
		for j, val := range refCounters[i] {
			assert.Equal(t, val, storeCounters[i][j],
				"counter mismatch at [%d][%d]", i, j)
		}
	}
}

func TestGenerateStoreSections_RoundTrip(t *testing.T) {
	t.Parallel()

	ticks := buildTestShotnessTicks()

	storeDir := t.TempDir()
	store := analyze.NewFileReportStore(storeDir)

	analyzer := &Analyzer{}

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

	// Expects treemap + heatmap + bar chart.
	const expectedSectionCount = 3

	require.Len(t, sections, expectedSectionCount)
	assert.Equal(t, "Code Hotness TreeMap", sections[0].Title)
	assert.Equal(t, "Function Coupling Matrix", sections[1].Title)
	assert.Equal(t, "Top Hot Functions", sections[2].Title)
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
