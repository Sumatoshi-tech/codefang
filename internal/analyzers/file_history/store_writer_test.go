package filehistory

// FRD: specs/frds/FRD-20260301-burndown-filehistory-store-writer.md.

import (
	"context"
	"fmt"
	"sort"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

const testFileCount = 3

// newTestHash creates a deterministic Hash from an integer for testing.
func newTestHash(n int) gitlib.Hash {
	return gitlib.NewHash(fmt.Sprintf("%040x", n))
}

// buildTestFileHistoryAggregator creates an aggregator with known file history data.
func buildTestFileHistoryAggregator(tb testing.TB) *Aggregator {
	tb.Helper()

	agg := NewAggregator(analyze.AggregatorOptions{})

	// File a.go: 3 commits, 2 contributors.
	require.NoError(tb, agg.Add(analyze.TC{
		CommitHash: newTestHash(1),
		Data: &CommitData{
			PathActions: []PathAction{{Path: "a.go", Action: gitlib.Insert}},
			LineStatUpdates: []LineStatUpdate{
				{Path: "a.go", AuthorID: 0, Stats: plumbing.LineStats{Added: 50}},
			},
		},
	}))

	require.NoError(tb, agg.Add(analyze.TC{
		CommitHash: newTestHash(2),
		Data: &CommitData{
			PathActions: []PathAction{{Path: "a.go", Action: gitlib.Modify}},
			LineStatUpdates: []LineStatUpdate{
				{Path: "a.go", AuthorID: 0, Stats: plumbing.LineStats{Added: 10, Changed: 5}},
			},
		},
	}))

	require.NoError(tb, agg.Add(analyze.TC{
		CommitHash: newTestHash(3),
		Data: &CommitData{
			PathActions: []PathAction{{Path: "a.go", Action: gitlib.Modify}},
			LineStatUpdates: []LineStatUpdate{
				{Path: "a.go", AuthorID: 1, Stats: plumbing.LineStats{Added: 20, Removed: 5}},
			},
		},
	}))

	// File b.go: 2 commits, 1 contributor.
	require.NoError(tb, agg.Add(analyze.TC{
		CommitHash: newTestHash(4),
		Data: &CommitData{
			PathActions: []PathAction{{Path: "b.go", Action: gitlib.Insert}},
			LineStatUpdates: []LineStatUpdate{
				{Path: "b.go", AuthorID: 0, Stats: plumbing.LineStats{Added: 30}},
			},
		},
	}))

	require.NoError(tb, agg.Add(analyze.TC{
		CommitHash: newTestHash(5),
		Data: &CommitData{
			PathActions: []PathAction{{Path: "b.go", Action: gitlib.Modify}},
			LineStatUpdates: []LineStatUpdate{
				{Path: "b.go", AuthorID: 0, Stats: plumbing.LineStats{Added: 5}},
			},
		},
	}))

	// File c.go: 1 commit, 1 contributor.
	require.NoError(tb, agg.Add(analyze.TC{
		CommitHash: newTestHash(6),
		Data: &CommitData{
			PathActions: []PathAction{{Path: "c.go", Action: gitlib.Insert}},
			LineStatUpdates: []LineStatUpdate{
				{Path: "c.go", AuthorID: 1, Stats: plumbing.LineStats{Added: 100}},
			},
		},
	}))

	return agg
}

func TestWriteToStoreFromAggregator_RoundTrip(t *testing.T) {
	t.Parallel()

	agg := buildTestFileHistoryAggregator(t)
	defer agg.Close()

	storeDir := t.TempDir()
	store := analyze.NewFileReportStore(storeDir)

	analyzer := &HistoryAnalyzer{}

	meta := analyze.ReportMeta{AnalyzerID: "file-history"}
	w, beginErr := store.Begin("history/file-history", meta)
	require.NoError(t, beginErr)

	ctx := context.Background()
	writeErr := analyzer.WriteToStoreFromAggregator(ctx, agg, w)
	require.NoError(t, writeErr)
	require.NoError(t, w.Close())
	require.NoError(t, store.Close())

	// Read back.
	readStore := analyze.NewFileReportStore(storeDir)
	defer readStore.Close()

	reader, openErr := readStore.Open("history/file-history")
	require.NoError(t, openErr)

	defer reader.Close()

	// Verify file_churn records.
	churn, churnErr := readFileChurnIfPresent(reader, reader.Kinds())
	require.NoError(t, churnErr)
	require.Len(t, churn, testFileCount)

	// Churn data should be sorted by churn score descending.
	for i := 1; i < len(churn); i++ {
		assert.GreaterOrEqual(t, churn[i-1].ChurnScore, churn[i].ChurnScore)
	}

	// Verify summary record.
	var summary AggregateData

	summaryErr := reader.Iter(KindSummary, func(raw []byte) error {
		return analyze.GobDecode(raw, &summary)
	})
	require.NoError(t, summaryErr)
	assert.Equal(t, testFileCount, summary.TotalFiles)
	assert.Positive(t, summary.TotalCommits)
	assert.Positive(t, summary.TotalContributors)
}

func TestWriteToStoreFromAggregator_WrongType(t *testing.T) {
	t.Parallel()

	analyzer := &HistoryAnalyzer{}
	ctx := context.Background()
	err := analyzer.WriteToStoreFromAggregator(ctx, &mockAggregator{}, nil)
	require.ErrorIs(t, err, ErrUnexpectedAggregator)
}

func TestWriteToStoreFromAggregator_EmptyAggregator(t *testing.T) {
	t.Parallel()

	agg := NewAggregator(analyze.AggregatorOptions{})
	defer agg.Close()

	storeDir := t.TempDir()
	store := analyze.NewFileReportStore(storeDir)

	analyzer := &HistoryAnalyzer{}

	meta := analyze.ReportMeta{AnalyzerID: "file-history"}
	w, beginErr := store.Begin("history/file-history", meta)
	require.NoError(t, beginErr)

	ctx := context.Background()
	writeErr := analyzer.WriteToStoreFromAggregator(ctx, agg, w)
	require.NoError(t, writeErr)
	require.NoError(t, w.Close())
	require.NoError(t, store.Close())

	readStore := analyze.NewFileReportStore(storeDir)
	defer readStore.Close()

	reader, openErr := readStore.Open("history/file-history")
	require.NoError(t, openErr)

	defer reader.Close()

	churn, churnErr := readFileChurnIfPresent(reader, reader.Kinds())
	require.NoError(t, churnErr)
	assert.Empty(t, churn)
}

func TestWriteToStoreFromAggregator_EquivalenceReference(t *testing.T) {
	t.Parallel()

	agg := buildTestFileHistoryAggregator(t)
	defer agg.Close()

	// Reference path: FlushAllTicks → TicksToReport → ComputeAllMetrics.
	ticks, flushErr := agg.FlushAllTicks()
	require.NoError(t, flushErr)

	ctx := context.Background()
	refReport := TicksToReport(ctx, ticks, nil)
	refMetrics, metricsErr := ComputeAllMetrics(refReport)
	require.NoError(t, metricsErr)

	// Store path.
	agg2 := buildTestFileHistoryAggregator(t)
	defer agg2.Close()

	storeDir := t.TempDir()
	store := analyze.NewFileReportStore(storeDir)

	analyzer := &HistoryAnalyzer{}

	meta := analyze.ReportMeta{AnalyzerID: "file-history"}
	w, beginErr := store.Begin("history/file-history", meta)
	require.NoError(t, beginErr)

	writeErr := analyzer.WriteToStoreFromAggregator(ctx, agg2, w)
	require.NoError(t, writeErr)
	require.NoError(t, w.Close())
	require.NoError(t, store.Close())

	readStore := analyze.NewFileReportStore(storeDir)
	defer readStore.Close()

	reader, openErr := readStore.Open("history/file-history")
	require.NoError(t, openErr)

	defer reader.Close()

	storeChurn, churnErr := readFileChurnIfPresent(reader, reader.Kinds())
	require.NoError(t, churnErr)

	// Sort both for comparison.
	refChurn := refMetrics.FileChurn
	sort.Slice(refChurn, func(i, j int) bool {
		return refChurn[i].Path < refChurn[j].Path
	})

	sort.Slice(storeChurn, func(i, j int) bool {
		return storeChurn[i].Path < storeChurn[j].Path
	})

	require.Len(t, storeChurn, len(refChurn))

	for i := range refChurn {
		assert.Equal(t, refChurn[i].Path, storeChurn[i].Path)
		assert.Equal(t, refChurn[i].CommitCount, storeChurn[i].CommitCount)
		assert.Equal(t, refChurn[i].ContributorCount, storeChurn[i].ContributorCount)
		assert.InDelta(t, refChurn[i].ChurnScore, storeChurn[i].ChurnScore, 0.01)
	}

	// Compare aggregate.
	var storeAgg AggregateData

	summaryErr := reader.Iter(KindSummary, func(raw []byte) error {
		return analyze.GobDecode(raw, &storeAgg)
	})
	require.NoError(t, summaryErr)

	assert.Equal(t, refMetrics.Aggregate.TotalFiles, storeAgg.TotalFiles)
	assert.Equal(t, refMetrics.Aggregate.TotalContributors, storeAgg.TotalContributors)
}

func TestGenerateStoreSections_RoundTrip(t *testing.T) {
	t.Parallel()

	agg := buildTestFileHistoryAggregator(t)
	defer agg.Close()

	storeDir := t.TempDir()
	store := analyze.NewFileReportStore(storeDir)

	analyzer := &HistoryAnalyzer{}

	meta := analyze.ReportMeta{AnalyzerID: "file-history"}
	w, beginErr := store.Begin("history/file-history", meta)
	require.NoError(t, beginErr)

	ctx := context.Background()
	writeErr := analyzer.WriteToStoreFromAggregator(ctx, agg, w)
	require.NoError(t, writeErr)
	require.NoError(t, w.Close())
	require.NoError(t, store.Close())

	readStore := analyze.NewFileReportStore(storeDir)
	defer readStore.Close()

	reader, openErr := readStore.Open("history/file-history")
	require.NoError(t, openErr)

	defer reader.Close()

	sections, secErr := GenerateStoreSections(reader)
	require.NoError(t, secErr)
	require.Len(t, sections, 1)
	assert.Equal(t, "Most Modified Files", sections[0].Title)
}

// mockAggregator satisfies analyze.Aggregator but is not *filehistory.Aggregator.
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
