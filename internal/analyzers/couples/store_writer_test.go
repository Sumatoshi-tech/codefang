package couples

// FRD: specs/frds/FRD-20260228-couples-store-writer.md.

import (
	"context"
	"sort"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
)

// testNames holds developer names for test fixtures.
var testNames = []string{"alice", "bob", "carol"}

// testPeopleCount is the number of developers in test fixtures.
const testPeopleCount = 3

// buildTestAggregator creates an aggregator with known coupling data for testing.
// Files: a.go, b.go, c.go with known coupling counts.
func buildTestAggregator(tb testing.TB) *Aggregator {
	tb.Helper()

	agg := newAggregator(
		analyze.AggregatorOptions{},
		testPeopleCount,
		testNames,
		nil, // no lastCommit for unit tests.
	)

	// Commit 1: alice touches a.go + b.go.
	require.NoError(tb, agg.Add(analyze.TC{
		AuthorID: 0,
		Data: &CommitData{
			CouplingFiles: []string{"a.go", "b.go"},
			AuthorFiles:   map[string]int{"a.go": 1, "b.go": 1},
			CommitCounted: true,
		},
	}))

	// Commit 2: alice touches a.go + b.go + c.go.
	require.NoError(tb, agg.Add(analyze.TC{
		AuthorID: 0,
		Data: &CommitData{
			CouplingFiles: []string{"a.go", "b.go", "c.go"},
			AuthorFiles:   map[string]int{"a.go": 1, "b.go": 1, "c.go": 1},
			CommitCounted: true,
		},
	}))

	// Commit 3: bob touches b.go + c.go.
	require.NoError(tb, agg.Add(analyze.TC{
		AuthorID: 1,
		Data: &CommitData{
			CouplingFiles: []string{"b.go", "c.go"},
			AuthorFiles:   map[string]int{"b.go": 1, "c.go": 1},
			CommitCounted: true,
		},
	}))

	return agg
}

func TestWriteToStoreFromAggregator_RoundTrip(t *testing.T) {
	t.Parallel()

	agg := buildTestAggregator(t)
	defer agg.Close()

	storeDir := t.TempDir()
	store := analyze.NewFileReportStore(storeDir)

	analyzer := &HistoryAnalyzer{}
	analyzer.TopKPerFile = DefaultTopKPerFile
	analyzer.MinEdgeWeight = 1 // Include all edges.

	meta := analyze.ReportMeta{AnalyzerID: "couples"}
	w, beginErr := store.Begin("history/couples", meta)
	require.NoError(t, beginErr)

	ctx := context.Background()
	writeErr := analyzer.WriteToStoreFromAggregator(ctx, agg, w)
	require.NoError(t, writeErr)
	require.NoError(t, w.Close())
	require.NoError(t, store.Close())

	// Read back.
	readStore := analyze.NewFileReportStore(storeDir)
	defer readStore.Close()

	reader, openErr := readStore.Open("history/couples")
	require.NoError(t, openErr)

	defer reader.Close()

	// Verify file_coupling records exist.
	fileCoupling, fcErr := readFileCoupling(reader)
	require.NoError(t, fcErr)
	assert.NotEmpty(t, fileCoupling, "expected file coupling records")

	// Verify dev_matrix record exists.
	devMatrix, dmErr := readDevMatrix(reader)
	require.NoError(t, dmErr)
	assert.NotNil(t, devMatrix)
	assert.NotEmpty(t, devMatrix.Names)

	// Verify ownership records exist.
	ownership, owErr := readOwnership(reader)
	require.NoError(t, owErr)
	assert.NotEmpty(t, ownership, "expected ownership records")

	// Verify aggregate record exists.
	var aggregate AggregateData

	aggErr := reader.Iter(KindAggregate, func(raw []byte) error {
		return analyze.GobDecode(raw, &aggregate)
	})
	require.NoError(t, aggErr)
	assert.Positive(t, aggregate.TotalFiles)
}

func TestWriteToStoreFromAggregator_FileCouplingTopK(t *testing.T) {
	t.Parallel()

	agg := buildTestAggregator(t)
	defer agg.Close()

	storeDir := t.TempDir()
	store := analyze.NewFileReportStore(storeDir)

	analyzer := &HistoryAnalyzer{}
	analyzer.TopKPerFile = 1 // Only top 1 pair.
	analyzer.MinEdgeWeight = 1

	meta := analyze.ReportMeta{AnalyzerID: "couples"}
	w, beginErr := store.Begin("history/couples", meta)
	require.NoError(t, beginErr)

	ctx := context.Background()
	writeErr := analyzer.WriteToStoreFromAggregator(ctx, agg, w)
	require.NoError(t, writeErr)
	require.NoError(t, w.Close())
	require.NoError(t, store.Close())

	readStore := analyze.NewFileReportStore(storeDir)
	defer readStore.Close()

	reader, openErr := readStore.Open("history/couples")
	require.NoError(t, openErr)

	defer reader.Close()

	fileCoupling, fcErr := readFileCoupling(reader)
	require.NoError(t, fcErr)
	assert.Len(t, fileCoupling, 1, "TopK=1 should emit exactly 1 pair")
}

func TestWriteToStoreFromAggregator_MinEdgeWeight(t *testing.T) {
	t.Parallel()

	agg := buildTestAggregator(t)
	defer agg.Close()

	storeDir := t.TempDir()
	store := analyze.NewFileReportStore(storeDir)

	analyzer := &HistoryAnalyzer{}
	analyzer.TopKPerFile = DefaultTopKPerFile
	analyzer.MinEdgeWeight = 100 // Very high threshold — no edges should pass.

	meta := analyze.ReportMeta{AnalyzerID: "couples"}
	w, beginErr := store.Begin("history/couples", meta)
	require.NoError(t, beginErr)

	ctx := context.Background()
	writeErr := analyzer.WriteToStoreFromAggregator(ctx, agg, w)
	require.NoError(t, writeErr)
	require.NoError(t, w.Close())
	require.NoError(t, store.Close())

	readStore := analyze.NewFileReportStore(storeDir)
	defer readStore.Close()

	reader, openErr := readStore.Open("history/couples")
	require.NoError(t, openErr)

	defer reader.Close()

	kinds := reader.Kinds()
	fileCoupling, fcErr := readFileCouplingIfPresent(reader, kinds)
	require.NoError(t, fcErr)
	assert.Empty(t, fileCoupling, "high min weight should filter all edges")
}

func TestWriteToStoreFromAggregator_WrongAggregatorType(t *testing.T) {
	t.Parallel()

	analyzer := &HistoryAnalyzer{}

	// Use a mock aggregator that isn't *couples.Aggregator.
	ctx := context.Background()
	err := analyzer.WriteToStoreFromAggregator(ctx, &mockAggregator{}, nil)
	require.ErrorIs(t, err, ErrUnexpectedAggregator)
}

func TestWriteToStoreFromAggregator_EmptyAggregator(t *testing.T) {
	t.Parallel()

	agg := newAggregator(analyze.AggregatorOptions{}, 0, nil, nil)
	defer agg.Close()

	storeDir := t.TempDir()
	store := analyze.NewFileReportStore(storeDir)

	analyzer := &HistoryAnalyzer{}

	meta := analyze.ReportMeta{AnalyzerID: "couples"}
	w, beginErr := store.Begin("history/couples", meta)
	require.NoError(t, beginErr)

	ctx := context.Background()
	writeErr := analyzer.WriteToStoreFromAggregator(ctx, agg, w)
	require.NoError(t, writeErr)
	require.NoError(t, w.Close())
	require.NoError(t, store.Close())

	readStore := analyze.NewFileReportStore(storeDir)
	defer readStore.Close()

	reader, openErr := readStore.Open("history/couples")
	require.NoError(t, openErr)

	defer reader.Close()

	// Should have zero file coupling but still have aggregate and dev_matrix.
	kinds := reader.Kinds()
	fileCoupling, fcErr := readFileCouplingIfPresent(reader, kinds)
	require.NoError(t, fcErr)
	assert.Empty(t, fileCoupling)
}

func TestWriteToStoreFromAggregator_EquivalenceReferencePath(t *testing.T) {
	t.Parallel()

	agg := buildTestAggregator(t)
	defer agg.Close()

	// Reference path: FlushAllTicks then ticksToReport then ComputeAllMetrics.
	ticks, flushErr := agg.FlushAllTicks()
	require.NoError(t, flushErr)

	ctx := context.Background()
	refReport := ticksToReport(ctx, ticks, testNames, testPeopleCount, nil)
	refMetrics, metricsErr := ComputeAllMetrics(refReport)
	require.NoError(t, metricsErr)

	// Store path: WriteToStoreFromAggregator then read back.
	// Re-build aggregator (FlushAllTicks consumed it by deep copy, original is still intact).
	agg2 := buildTestAggregator(t)
	defer agg2.Close()

	storeDir := t.TempDir()
	store := analyze.NewFileReportStore(storeDir)

	analyzer := &HistoryAnalyzer{}
	analyzer.TopKPerFile = DefaultTopKPerFile
	analyzer.MinEdgeWeight = 1

	meta := analyze.ReportMeta{AnalyzerID: "couples"}
	w, beginErr := store.Begin("history/couples", meta)
	require.NoError(t, beginErr)

	writeErr := analyzer.WriteToStoreFromAggregator(ctx, agg2, w)
	require.NoError(t, writeErr)
	require.NoError(t, w.Close())
	require.NoError(t, store.Close())

	readStore := analyze.NewFileReportStore(storeDir)
	defer readStore.Close()

	reader, openErr := readStore.Open("history/couples")
	require.NoError(t, openErr)

	defer reader.Close()

	storeFileCoupling, fcErr := readFileCoupling(reader)
	require.NoError(t, fcErr)

	// Compare top file coupling pairs. Sort both by File1+File2 for deterministic comparison.
	refCoupling := refMetrics.FileCoupling
	sort.Slice(refCoupling, func(i, j int) bool {
		if refCoupling[i].File1 == refCoupling[j].File1 {
			return refCoupling[i].File2 < refCoupling[j].File2
		}

		return refCoupling[i].File1 < refCoupling[j].File1
	})

	sort.Slice(storeFileCoupling, func(i, j int) bool {
		if storeFileCoupling[i].File1 == storeFileCoupling[j].File1 {
			return storeFileCoupling[i].File2 < storeFileCoupling[j].File2
		}

		return storeFileCoupling[i].File1 < storeFileCoupling[j].File1
	})

	require.Len(t, storeFileCoupling, len(refCoupling),
		"store path should produce same number of coupling pairs as reference")

	for i := range refCoupling {
		assert.Equal(t, refCoupling[i].File1, storeFileCoupling[i].File1)
		assert.Equal(t, refCoupling[i].File2, storeFileCoupling[i].File2)
		assert.Equal(t, refCoupling[i].CoChanges, storeFileCoupling[i].CoChanges)
		assert.InDelta(t, refCoupling[i].Strength, storeFileCoupling[i].Strength, 0.01)
	}

	// Compare aggregate.
	var storeAgg AggregateData

	aggErr := reader.Iter(KindAggregate, func(raw []byte) error {
		return analyze.GobDecode(raw, &storeAgg)
	})
	require.NoError(t, aggErr)

	assert.Equal(t, refMetrics.Aggregate.TotalFiles, storeAgg.TotalFiles)
	assert.Equal(t, refMetrics.Aggregate.TotalCoChanges, storeAgg.TotalCoChanges)
	assert.Equal(t, refMetrics.Aggregate.HighlyCoupledPairs, storeAgg.HighlyCoupledPairs)
	assert.InDelta(t, refMetrics.Aggregate.AvgCouplingStrength, storeAgg.AvgCouplingStrength, 0.01)
}

func TestGenerateStoreSections_RoundTrip(t *testing.T) {
	t.Parallel()

	agg := buildTestAggregator(t)
	defer agg.Close()

	storeDir := t.TempDir()
	store := analyze.NewFileReportStore(storeDir)

	analyzer := &HistoryAnalyzer{}
	analyzer.TopKPerFile = DefaultTopKPerFile
	analyzer.MinEdgeWeight = 1

	meta := analyze.ReportMeta{AnalyzerID: "couples"}
	w, beginErr := store.Begin("history/couples", meta)
	require.NoError(t, beginErr)

	ctx := context.Background()
	writeErr := analyzer.WriteToStoreFromAggregator(ctx, agg, w)
	require.NoError(t, writeErr)
	require.NoError(t, w.Close())
	require.NoError(t, store.Close())

	// Read back and generate sections.
	readStore := analyze.NewFileReportStore(storeDir)
	defer readStore.Close()

	reader, openErr := readStore.Open("history/couples")
	require.NoError(t, openErr)

	defer reader.Close()

	sections, secErr := GenerateStoreSections(reader)
	require.NoError(t, secErr)

	// Should have at least 2 sections (file coupling bar + dev heatmap).
	// Ownership pie might be empty if no line count data.
	assert.GreaterOrEqual(t, len(sections), 2,
		"expected at least file coupling and dev heatmap sections")

	// Verify section titles.
	titles := make([]string, len(sections))
	for i, s := range sections {
		titles[i] = s.Title
	}

	assert.Contains(t, titles, "Top File Couples")
	assert.Contains(t, titles, "Developer Coupling Heatmap")
}

func TestComputeSparseCoupling_MatchesDenseMatrix(t *testing.T) {
	t.Parallel()

	// Build a known sparse file coupling map.
	rawFiles := map[string]map[string]int{
		"a.go": {"a.go": 3, "b.go": 2, "c.go": 1},
		"b.go": {"a.go": 2, "b.go": 3, "c.go": 2},
		"c.go": {"a.go": 1, "b.go": 2, "c.go": 2},
	}

	filesSequence := []string{"a.go", "b.go", "c.go"}
	filesIndex := map[string]int{"a.go": 0, "b.go": 1, "c.go": 2}

	// Sparse coupling with minWeight=1.
	sparse := computeSparseCoupling(rawFiles, filesSequence, filesIndex, 1)

	// Dense path: computeFilesMatrix then FileCouplingMetric.Compute.
	filesMatrix := computeFilesMatrix(rawFiles, filesSequence, filesIndex)
	input := &ReportData{
		Files:       filesSequence,
		FilesMatrix: filesMatrix,
	}
	metric := NewFileCouplingMetric()
	dense := metric.Compute(input)

	// Sort both for comparison.
	sort.Slice(sparse, func(i, j int) bool {
		if sparse[i].File1 == sparse[j].File1 {
			return sparse[i].File2 < sparse[j].File2
		}

		return sparse[i].File1 < sparse[j].File1
	})

	sort.Slice(dense, func(i, j int) bool {
		if dense[i].File1 == dense[j].File1 {
			return dense[i].File2 < dense[j].File2
		}

		return dense[i].File1 < dense[j].File1
	})

	require.Len(t, sparse, len(dense), "sparse and dense should produce same count")

	for i := range dense {
		assert.Equal(t, dense[i].File1, sparse[i].File1)
		assert.Equal(t, dense[i].File2, sparse[i].File2)
		assert.Equal(t, dense[i].CoChanges, sparse[i].CoChanges)
		assert.InDelta(t, dense[i].Strength, sparse[i].Strength, 0.001)
	}
}

func TestComputeSparseAggregate_MatchesDenseAggregate(t *testing.T) {
	t.Parallel()

	rawFiles := map[string]map[string]int{
		"a.go": {"a.go": 5, "b.go": 3, "c.go": 1},
		"b.go": {"a.go": 3, "b.go": 4, "c.go": 2},
		"c.go": {"a.go": 1, "b.go": 2, "c.go": 3},
	}

	filesSequence := []string{"a.go", "b.go", "c.go"}
	filesIndex := map[string]int{"a.go": 0, "b.go": 1, "c.go": 2}
	names := []string{"alice", "bob"}

	// Sparse aggregate.
	sparseAgg := computeSparseAggregate(rawFiles, filesSequence, filesIndex, names)

	// Dense aggregate.
	filesMatrix := computeFilesMatrix(rawFiles, filesSequence, filesIndex)
	input := &ReportData{
		Files:              filesSequence,
		FilesMatrix:        filesMatrix,
		ReversedPeopleDict: names,
	}
	aggMetric := NewAggregateMetric()
	denseAgg := aggMetric.Compute(input)

	assert.Equal(t, denseAgg.TotalFiles, sparseAgg.TotalFiles)
	assert.Equal(t, denseAgg.TotalDevelopers, sparseAgg.TotalDevelopers)
	assert.Equal(t, denseAgg.TotalCoChanges, sparseAgg.TotalCoChanges)
	assert.Equal(t, denseAgg.HighlyCoupledPairs, sparseAgg.HighlyCoupledPairs)
	assert.InDelta(t, denseAgg.AvgCouplingStrength, sparseAgg.AvgCouplingStrength, 0.001)
}

// mockAggregator satisfies analyze.Aggregator but is not *couples.Aggregator.
type mockAggregator struct{}

func (m *mockAggregator) Add(_ analyze.TC) error { return nil }

func (m *mockAggregator) FlushTick(_ int) (analyze.TICK, error) { return analyze.TICK{}, nil }

func (m *mockAggregator) FlushAllTicks() ([]analyze.TICK, error) { return nil, nil }

func (m *mockAggregator) DiscardState() {}

func (m *mockAggregator) Spill() (int64, error) { return 0, nil }

func (m *mockAggregator) Collect() error { return nil }

func (m *mockAggregator) EstimatedStateSize() int64 { return 0 }

func (m *mockAggregator) SpillState() analyze.AggregatorSpillInfo {
	return analyze.AggregatorSpillInfo{}
}

func (m *mockAggregator) RestoreSpillState(_ analyze.AggregatorSpillInfo) {}

func (m *mockAggregator) Close() error { return nil }
