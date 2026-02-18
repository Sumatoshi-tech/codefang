package quality

import (
	"bytes"
	"context"
	"encoding/json"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
	pkgplumbing "github.com/Sumatoshi-tech/codefang/pkg/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/uast"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

// Test hash constants to avoid magic strings.
const (
	testHashA = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
	testHashB = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
)

func TestHistoryAnalyzer_Descriptor(t *testing.T) {
	t.Parallel()

	ha := &HistoryAnalyzer{}
	desc := ha.Descriptor()

	assert.Equal(t, "history/quality", desc.ID)
	assert.Equal(t, analyze.ModeHistory, desc.Mode)
	assert.NotEmpty(t, desc.Description)
}

func TestHistoryAnalyzer_NameAndFlag(t *testing.T) {
	t.Parallel()

	ha := &HistoryAnalyzer{}

	assert.Equal(t, "CodeQuality", ha.Name())
	assert.Equal(t, "quality", ha.Flag())
}

func TestHistoryAnalyzer_Configure(t *testing.T) {
	t.Parallel()

	ha := &HistoryAnalyzer{}
	facts := map[string]any{
		pkgplumbing.FactCommitsByTick: map[int][]gitlib.Hash{},
	}

	err := ha.Configure(facts)
	require.NoError(t, err)
	assert.NotNil(t, ha.commitsByTick)
}

func TestHistoryAnalyzer_Initialize(t *testing.T) {
	t.Parallel()

	ha := &HistoryAnalyzer{}

	err := ha.Initialize(nil)
	require.NoError(t, err)

	assert.NotNil(t, ha.commitQuality)
	assert.NotNil(t, ha.complexityAnalyzer)
	assert.NotNil(t, ha.halsteadAnalyzer)
	assert.NotNil(t, ha.commentsAnalyzer)
	assert.NotNil(t, ha.cohesionAnalyzer)
}

func TestHistoryAnalyzer_Consume_EmptyChanges(t *testing.T) {
	t.Parallel()

	ha := newTestAnalyzer()

	ha.UAST.SetChangesForTest(nil)
	ha.Ticks.Tick = 0

	err := ha.Consume(context.Background(), &analyze.Context{})
	require.NoError(t, err)

	// No commit was provided, so no per-commit data stored.
	assert.Empty(t, ha.commitQuality)
}

func TestHistoryAnalyzer_Consume_BasicNode(t *testing.T) {
	t.Parallel()

	ha := newTestAnalyzer()
	funcNode := buildTestFunctionNode()
	hash := gitlib.NewHash(testHashA)

	ha.UAST.SetChangesForTest([]uast.Change{
		{After: funcNode},
	})
	ha.Ticks.Tick = 0

	err := ha.Consume(context.Background(), &analyze.Context{
		Commit: gitlib.NewCommitForTest(hash),
	})
	require.NoError(t, err)

	cq := ha.commitQuality[hash.String()]
	require.NotNil(t, cq)
	assert.GreaterOrEqual(t, cq.filesAnalyzed(), 1)
}

func TestHistoryAnalyzer_Consume_StoresPerCommitQuality(t *testing.T) {
	t.Parallel()

	ha := newTestAnalyzer()
	funcNode := buildTestFunctionNode()

	hash1 := gitlib.NewHash(testHashA)
	hash2 := gitlib.NewHash(testHashB)

	// Commit 1.
	ha.UAST.SetChangesForTest([]uast.Change{{After: funcNode}})
	ha.Ticks.Tick = 0

	err := ha.Consume(context.Background(), &analyze.Context{
		Commit: gitlib.NewCommitForTest(hash1),
	})
	require.NoError(t, err)

	// Commit 2 (same tick).
	ha.UAST.SetChangesForTest([]uast.Change{{After: funcNode}})
	ha.Ticks.Tick = 0

	err = ha.Consume(context.Background(), &analyze.Context{
		Commit: gitlib.NewCommitForTest(hash2),
	})
	require.NoError(t, err)

	// Per-commit data should have separate entries.
	require.Len(t, ha.commitQuality, 2)
	assert.Contains(t, ha.commitQuality, hash1.String())
	assert.Contains(t, ha.commitQuality, hash2.String())

	// Each commit should have its own TickQuality data.
	cq1 := ha.commitQuality[hash1.String()]
	cq2 := ha.commitQuality[hash2.String()]

	assert.GreaterOrEqual(t, cq1.filesAnalyzed(), 1)
	assert.GreaterOrEqual(t, cq2.filesAnalyzed(), 1)
}

func TestHistoryAnalyzer_Consume_DeletedFile(t *testing.T) {
	t.Parallel()

	ha := newTestAnalyzer()
	hash := gitlib.NewHash(testHashA)

	// Deleted file: After is nil.
	ha.UAST.SetChangesForTest([]uast.Change{
		{Before: &node.Node{Type: node.UASTFile}, After: nil},
	})
	ha.Ticks.Tick = 0

	err := ha.Consume(context.Background(), &analyze.Context{
		Commit: gitlib.NewCommitForTest(hash),
	})
	require.NoError(t, err)

	cq := ha.commitQuality[hash.String()]
	require.NotNil(t, cq)
	assert.Equal(t, 0, cq.filesAnalyzed())
}

func TestHistoryAnalyzer_Consume_MultipleFilesSameTick(t *testing.T) {
	t.Parallel()

	ha := newTestAnalyzer()
	hash := gitlib.NewHash(testHashA)

	funcNode1 := buildTestFunctionNode()
	funcNode2 := buildTestFunctionNode()

	ha.UAST.SetChangesForTest([]uast.Change{
		{After: funcNode1},
		{After: funcNode2},
	})
	ha.Ticks.Tick = 0

	err := ha.Consume(context.Background(), &analyze.Context{
		Commit: gitlib.NewCommitForTest(hash),
	})
	require.NoError(t, err)

	cq := ha.commitQuality[hash.String()]
	require.NotNil(t, cq)
	assert.GreaterOrEqual(t, cq.filesAnalyzed(), 2)
}

func TestHistoryAnalyzer_Consume_MultipleCommits(t *testing.T) {
	t.Parallel()

	ha := newTestAnalyzer()
	hash1 := gitlib.NewHash(testHashA)
	hash2 := gitlib.NewHash(testHashB)

	funcNode := buildTestFunctionNode()

	// Commit 1.
	ha.UAST.SetChangesForTest([]uast.Change{{After: funcNode}})
	ha.Ticks.Tick = 0
	require.NoError(t, ha.Consume(context.Background(), &analyze.Context{
		Commit: gitlib.NewCommitForTest(hash1),
	}))

	// Commit 2 (different tick).
	ha.UAST.SetChangesForTest([]uast.Change{{After: funcNode}})
	ha.Ticks.Tick = 1
	require.NoError(t, ha.Consume(context.Background(), &analyze.Context{
		Commit: gitlib.NewCommitForTest(hash2),
	}))

	assert.Len(t, ha.commitQuality, 2)
	assert.GreaterOrEqual(t, ha.commitQuality[hash1.String()].filesAnalyzed(), 1)
	assert.GreaterOrEqual(t, ha.commitQuality[hash2.String()].filesAnalyzed(), 1)
}

func TestHistoryAnalyzer_Finalize(t *testing.T) {
	t.Parallel()

	ha := newTestAnalyzer()
	hashStr := testHashA
	ha.commitQuality[hashStr] = &TickQuality{
		Complexities: []float64{10, 20},
	}

	report, err := ha.Finalize()
	require.NoError(t, err)

	cq, ok := report["commit_quality"].(map[string]*TickQuality)
	require.True(t, ok)
	assert.Len(t, cq, 1)
	assert.Len(t, cq[hashStr].Complexities, 2)
}

func TestHistoryAnalyzer_Finalize_IncludesCommitQuality(t *testing.T) {
	t.Parallel()

	ha := newTestAnalyzer()
	hashStr := testHashA
	ha.commitQuality[hashStr] = &TickQuality{
		Complexities: []float64{5, 10},
	}

	report, err := ha.Finalize()
	require.NoError(t, err)

	cq, ok := report["commit_quality"].(map[string]*TickQuality)
	require.True(t, ok, "report must contain commit_quality")
	assert.Len(t, cq, 1)
	assert.Contains(t, cq, hashStr)
	assert.Len(t, cq[hashStr].Complexities, 2)
}

func TestHistoryAnalyzer_Fork_IndependentState(t *testing.T) {
	t.Parallel()

	ha := newTestAnalyzer()
	ha.commitQuality["aaa"] = &TickQuality{
		Complexities: []float64{5, 10},
	}

	forks := ha.Fork(2)
	require.Len(t, forks, 2)

	fork1, ok1 := forks[0].(*HistoryAnalyzer)
	require.True(t, ok1)

	fork2, ok2 := forks[1].(*HistoryAnalyzer)
	require.True(t, ok2)

	// Forks should have empty independent maps.
	assert.Empty(t, fork1.commitQuality)
	assert.Empty(t, fork2.commitQuality)

	// Plumbing deps should be independent instances.
	assert.NotSame(t, ha.UAST, fork1.UAST)
	assert.NotSame(t, ha.Ticks, fork1.Ticks)
	assert.NotSame(t, fork1.UAST, fork2.UAST)

	// Static analyzers should be independent.
	assert.NotSame(t, ha.complexityAnalyzer, fork1.complexityAnalyzer)

	// Modifying one fork should not affect the other.
	fork1.commitQuality["abc"] = &TickQuality{Complexities: []float64{10}}

	assert.Empty(t, fork2.commitQuality)
}

func TestHistoryAnalyzer_Merge_CombinesCommits(t *testing.T) {
	t.Parallel()

	ha := newTestAnalyzer()
	ha.commitQuality["aaa"] = &TickQuality{
		Complexities: []float64{15},
	}

	branch := newTestAnalyzer()
	branch.commitQuality["bbb"] = &TickQuality{
		Complexities: []float64{10},
	}

	ha.Merge([]analyze.HistoryAnalyzer{branch})

	assert.Len(t, ha.commitQuality, 2)
	assert.Len(t, ha.commitQuality["aaa"].Complexities, 1)
	assert.Len(t, ha.commitQuality["bbb"].Complexities, 1)
}

func TestHistoryAnalyzer_Merge_CombinesCommitQuality(t *testing.T) {
	t.Parallel()

	ha := newTestAnalyzer()
	ha.commitQuality["aaa"] = &TickQuality{Complexities: []float64{5}}

	branch := newTestAnalyzer()
	branch.commitQuality["bbb"] = &TickQuality{Complexities: []float64{10}}

	ha.Merge([]analyze.HistoryAnalyzer{branch})

	assert.Len(t, ha.commitQuality, 2)
	assert.Contains(t, ha.commitQuality, "aaa")
	assert.Contains(t, ha.commitQuality, "bbb")
}

func TestHistoryAnalyzer_Merge_DistinctCommits(t *testing.T) {
	t.Parallel()

	ha := newTestAnalyzer()
	ha.commitQuality["aaa"] = &TickQuality{
		Complexities:    []float64{5, 10},
		HalsteadVolumes: []float64{100.0},
	}

	branch := newTestAnalyzer()
	branch.commitQuality["bbb"] = &TickQuality{
		Complexities:    []float64{15, 20},
		HalsteadVolumes: []float64{200.0, 300.0},
	}

	ha.Merge([]analyze.HistoryAnalyzer{branch})

	assert.Len(t, ha.commitQuality, 2)
	assert.Len(t, ha.commitQuality["aaa"].Complexities, 2)
	assert.Len(t, ha.commitQuality["bbb"].Complexities, 2)
}

func TestHistoryAnalyzer_ForkMerge_RoundTrip(t *testing.T) {
	t.Parallel()

	ha := newTestAnalyzer()

	forks := ha.Fork(2)

	fork1, ok1 := forks[0].(*HistoryAnalyzer)
	require.True(t, ok1)

	fork2, ok2 := forks[1].(*HistoryAnalyzer)
	require.True(t, ok2)

	fork1.commitQuality["aaa"] = &TickQuality{Complexities: []float64{5, 10}}
	fork1.commitQuality["bbb"] = &TickQuality{Complexities: []float64{15}}
	fork2.commitQuality["ccc"] = &TickQuality{Complexities: []float64{20, 25}}
	fork2.commitQuality["ddd"] = &TickQuality{Complexities: []float64{30}}

	ha.Merge(forks)

	assert.Len(t, ha.commitQuality, 4)
	assert.Equal(t, 2, ha.commitQuality["aaa"].filesAnalyzed())
	assert.Equal(t, 1, ha.commitQuality["bbb"].filesAnalyzed())
	assert.Equal(t, 2, ha.commitQuality["ccc"].filesAnalyzed())
	assert.Equal(t, 1, ha.commitQuality["ddd"].filesAnalyzed())
}

func TestHistoryAnalyzer_SequentialOnly(t *testing.T) {
	t.Parallel()

	ha := &HistoryAnalyzer{}
	assert.False(t, ha.SequentialOnly())
}

func TestHistoryAnalyzer_CPUHeavy(t *testing.T) {
	t.Parallel()

	ha := &HistoryAnalyzer{}
	assert.True(t, ha.CPUHeavy())
}

func TestHistoryAnalyzer_NeedsUAST(t *testing.T) {
	t.Parallel()

	ha := &HistoryAnalyzer{}
	assert.True(t, ha.NeedsUAST())
}

func TestHistoryAnalyzer_Snapshot(t *testing.T) {
	t.Parallel()

	ha := newTestAnalyzer()

	funcNode := buildTestFunctionNode()
	ha.UAST.SetChangesForTest([]uast.Change{{After: funcNode}})
	ha.Ticks.Tick = 5

	snap := ha.SnapshotPlumbing()
	require.NotNil(t, snap)

	// Reset state.
	ha.UAST.SetChangesForTest(nil)
	ha.Ticks.Tick = 0

	// Restore from snapshot.
	ha.ApplySnapshot(snap)

	assert.Equal(t, 5, ha.Ticks.Tick)
	changes := ha.UAST.Changes(context.Background())
	assert.Len(t, changes, 1)

	// Release should not panic.
	ha.ReleaseSnapshot(snap)
}

func TestHistoryAnalyzer_Snapshot_WrongType(t *testing.T) {
	t.Parallel()

	ha := newTestAnalyzer()

	ha.ApplySnapshot("wrong type")
	ha.ReleaseSnapshot("wrong type")
}

func TestHistoryAnalyzer_Hibernate_Boot(t *testing.T) {
	t.Parallel()

	ha := newTestAnalyzer()
	ha.commitQuality["aaa"] = &TickQuality{Complexities: []float64{5}}

	err := ha.Hibernate()
	require.NoError(t, err)

	assert.Len(t, ha.commitQuality, 1)

	err = ha.Boot()
	require.NoError(t, err)
}

func TestHistoryAnalyzer_StateGrowthPerCommit(t *testing.T) {
	t.Parallel()

	ha := &HistoryAnalyzer{}
	assert.Positive(t, ha.StateGrowthPerCommit())
}

func TestHistoryAnalyzer_Serialize_JSON(t *testing.T) {
	t.Parallel()

	ha := &HistoryAnalyzer{}
	report := buildTestQualityReport()

	var buf bytes.Buffer

	err := ha.Serialize(report, analyze.FormatJSON, &buf)
	require.NoError(t, err)

	var result map[string]any

	err = json.Unmarshal(buf.Bytes(), &result)
	require.NoError(t, err)

	assert.Contains(t, result, "time_series")
	assert.Contains(t, result, "aggregate")
}

func TestHistoryAnalyzer_Serialize_YAML(t *testing.T) {
	t.Parallel()

	ha := &HistoryAnalyzer{}
	report := buildTestQualityReport()

	var buf bytes.Buffer

	err := ha.Serialize(report, analyze.FormatYAML, &buf)
	require.NoError(t, err)

	output := buf.String()
	assert.Contains(t, output, "time_series:")
	assert.Contains(t, output, "aggregate:")
}

func TestHistoryAnalyzer_Serialize_Binary(t *testing.T) {
	t.Parallel()

	ha := &HistoryAnalyzer{}
	report := buildTestQualityReport()

	var buf bytes.Buffer

	err := ha.Serialize(report, analyze.FormatBinary, &buf)
	require.NoError(t, err)

	assert.Greater(t, buf.Len(), 8)
	assert.Equal(t, "CFB1", buf.String()[:4])
}

func TestHistoryAnalyzer_Serialize_Plot(t *testing.T) {
	t.Parallel()

	ha := &HistoryAnalyzer{}
	report := buildTestQualityReport()

	var buf bytes.Buffer

	err := ha.Serialize(report, analyze.FormatPlot, &buf)
	require.NoError(t, err)

	output := buf.String()
	assert.Contains(t, output, "<!doctype html>")
	assert.Contains(t, output, "Code Quality")
}

func TestHistoryAnalyzer_Serialize_Unknown(t *testing.T) {
	t.Parallel()

	ha := &HistoryAnalyzer{}
	report := buildTestQualityReport()

	var buf bytes.Buffer

	err := ha.Serialize(report, "unknown", &buf)
	require.ErrorIs(t, err, analyze.ErrUnsupportedFormat)
}

func TestHistoryAnalyzer_FormatReport(t *testing.T) {
	t.Parallel()

	ha := &HistoryAnalyzer{}
	report := buildTestQualityReport()

	var buf bytes.Buffer

	err := ha.FormatReport(report, &buf)
	require.NoError(t, err)
	assert.Contains(t, buf.String(), "time_series:")
}

func TestComputeAllMetrics_Basic(t *testing.T) {
	t.Parallel()

	report := buildTestQualityReport()

	computed, err := ComputeAllMetrics(report)
	require.NoError(t, err)

	assert.NotEmpty(t, computed.TimeSeries)
	assert.Positive(t, computed.Aggregate.TotalTicks)
	assert.Positive(t, computed.Aggregate.TotalFilesAnalyzed)

	// Verify median/p95 fields are populated.
	assert.Positive(t, computed.Aggregate.ComplexityMedianMean)
	assert.Positive(t, computed.Aggregate.ComplexityP95Mean)
	assert.Positive(t, computed.Aggregate.HalsteadVolMedianMean)
	assert.Positive(t, computed.Aggregate.TotalDeliveredBugs)
}

func TestComputeAllMetrics_FromCommitData(t *testing.T) {
	t.Parallel()

	hashA := testHashA
	hashB := testHashB

	report := analyze.Report{
		"commit_quality": map[string]*TickQuality{
			hashA: {
				Complexities:    []float64{10, 20},
				HalsteadVolumes: []float64{100, 200},
				DeliveredBugs:   []float64{0.1, 0.2},
				CommentScores:   []float64{0.5, 0.7},
				CohesionScores:  []float64{0.8, 0.9},
			},
			hashB: {
				Complexities:    []float64{30},
				HalsteadVolumes: []float64{300},
				DeliveredBugs:   []float64{0.3},
				CommentScores:   []float64{0.9},
				CohesionScores:  []float64{1.0},
			},
		},
		"commits_by_tick": map[int][]gitlib.Hash{
			0: {gitlib.NewHash(hashA)},
			1: {gitlib.NewHash(hashB)},
		},
	}

	computed, err := ComputeAllMetrics(report)
	require.NoError(t, err)

	assert.Len(t, computed.TimeSeries, 2)
	assert.Equal(t, 2, computed.Aggregate.TotalTicks)
	assert.Equal(t, 3, computed.Aggregate.TotalFilesAnalyzed)
}

func TestComputeAllMetrics_Empty(t *testing.T) {
	t.Parallel()

	report := analyze.Report{
		"tick_quality": map[int]*TickQuality{},
	}

	computed, err := ComputeAllMetrics(report)
	require.NoError(t, err)

	assert.Empty(t, computed.TimeSeries)
	assert.Equal(t, 0, computed.Aggregate.TotalTicks)
}

func TestComputeTickStats(t *testing.T) {
	t.Parallel()

	tq := &TickQuality{
		Complexities:    []float64{2, 4, 6, 8, 10},
		Cognitives:      []float64{1, 2, 3, 4, 5},
		MaxComplexities: []int{3, 5, 7, 8, 10},
		Functions:       []int{2, 3, 4, 5, 6},
		HalsteadVolumes: []float64{100, 200, 300, 400, 500},
		HalsteadEfforts: []float64{50, 100, 150, 200, 250},
		DeliveredBugs:   []float64{0.1, 0.2, 0.3, 0.4, 0.5},
		CommentScores:   []float64{0.3, 0.5, 0.7, 0.8, 0.9},
		DocCoverages:    []float64{0.4, 0.5, 0.6, 0.7, 0.8},
		CohesionScores:  []float64{0.8, 0.85, 0.9, 0.95, 1.0},
	}

	stats := computeTickStats(tq)

	assert.Equal(t, 5, stats.FilesAnalyzed)
	assert.InDelta(t, 6.0, stats.ComplexityMean, 0.01)   // (2+4+6+8+10)/5.
	assert.InDelta(t, 6.0, stats.ComplexityMedian, 0.01) // middle of sorted.
	assert.InDelta(t, 9.2, stats.ComplexityP95, 0.5)     // near top.
	assert.InDelta(t, 10.0, stats.ComplexityMax, 0.01)
	assert.InDelta(t, 300.0, stats.HalsteadVolMean, 0.01)
	assert.InDelta(t, 300.0, stats.HalsteadVolMedian, 0.01)
	assert.InDelta(t, 1500.0, stats.HalsteadVolSum, 0.01)
	assert.InDelta(t, 1.5, stats.DeliveredBugsSum, 0.01)
	assert.InDelta(t, 0.64, stats.CommentScoreMean, 0.01)
	assert.InDelta(t, 0.3, stats.CommentScoreMin, 0.01)
	assert.InDelta(t, 0.9, stats.CohesionMean, 0.01)
	assert.InDelta(t, 0.8, stats.CohesionMin, 0.01)
	assert.Equal(t, 20, stats.TotalFunctions) // 2+3+4+5+6.
	assert.Equal(t, 10, stats.MaxComplexity)  // max(3,5,7,8,10).
}

func TestComputeTickStats_ZeroFiles(t *testing.T) {
	t.Parallel()

	tq := &TickQuality{}
	stats := computeTickStats(tq)

	assert.Equal(t, 0, stats.FilesAnalyzed)
	assert.InDelta(t, 0.0, stats.ComplexityMean, 0.01)
	assert.InDelta(t, 0.0, stats.ComplexityMedian, 0.01)
}

func TestTickQuality_Merge(t *testing.T) {
	t.Parallel()

	tq1 := &TickQuality{
		Complexities:    []float64{5, 10},
		HalsteadVolumes: []float64{100},
		CommentScores:   []float64{0.8},
		CohesionScores:  []float64{0.9},
	}

	tq2 := &TickQuality{
		Complexities:    []float64{15},
		HalsteadVolumes: []float64{200, 300},
		CommentScores:   []float64{0.6},
		CohesionScores:  []float64{0.7},
	}

	tq1.merge(tq2)

	assert.Len(t, tq1.Complexities, 3)
	assert.Len(t, tq1.HalsteadVolumes, 3)
	assert.Len(t, tq1.CommentScores, 2)
	assert.Len(t, tq1.CohesionScores, 2)
}

func TestPercentileFloat(t *testing.T) {
	t.Parallel()

	values := []float64{1, 2, 3, 4, 5}

	assert.InDelta(t, 3.0, medianFloat(values), 0.01)
	assert.InDelta(t, 4.8, p95Float(values), 0.1)

	// Single value.
	assert.InDelta(t, 42.0, medianFloat([]float64{42}), 0.01)

	// Empty.
	assert.InDelta(t, 0.0, medianFloat(nil), 0.01)
}

func TestMeanStdDev(t *testing.T) {
	t.Parallel()

	mean, stddev := meanStdDev([]float64{2, 4, 4, 4, 5, 5, 7, 9})
	assert.InDelta(t, 5.0, mean, 0.01)
	assert.InDelta(t, 2.0, stddev, 0.01)

	mean, stddev = meanStdDev(nil)
	assert.InDelta(t, 0.0, mean, 0.01)
	assert.InDelta(t, 0.0, stddev, 0.01)
}

func TestMinMaxFloat(t *testing.T) {
	t.Parallel()

	assert.InDelta(t, 1.0, minFloat([]float64{3, 1, 4, 1, 5}), 0.01)
	assert.InDelta(t, 5.0, maxFloat([]float64{3, 1, 4, 1, 5}), 0.01)
	assert.InDelta(t, 0.0, minFloat(nil), 0.01)
	assert.InDelta(t, 0.0, maxFloat(nil), 0.01)
}

func TestSumIntMaxInt(t *testing.T) {
	t.Parallel()

	assert.Equal(t, 15, sumInt([]int{1, 2, 3, 4, 5}))
	assert.Equal(t, 5, maxInt([]int{1, 5, 3, 2, 4}))
	assert.Equal(t, 0, sumInt(nil))
	assert.Equal(t, 0, maxInt(nil))
}

func TestExtractInt(t *testing.T) {
	t.Parallel()

	report := analyze.Report{
		"int_val":   5,
		"float_val": 3.7,
		"str_val":   "hello",
	}

	assert.Equal(t, 5, extractInt(report, "int_val"))
	assert.Equal(t, 3, extractInt(report, "float_val"))
	assert.Equal(t, 0, extractInt(report, "str_val"))
	assert.Equal(t, 0, extractInt(report, "missing"))
}

func TestExtractFloat(t *testing.T) {
	t.Parallel()

	report := analyze.Report{
		"float_val": 3.14,
		"int_val":   7,
		"str_val":   "hello",
	}

	assert.InDelta(t, 3.14, extractFloat(report, "float_val"), 0.001)
	assert.InDelta(t, 7.0, extractFloat(report, "int_val"), 0.001)
	assert.InDelta(t, 0.0, extractFloat(report, "str_val"), 0.001)
	assert.InDelta(t, 0.0, extractFloat(report, "missing"), 0.001)
}

// --- Helpers ---.

func newTestAnalyzer() *HistoryAnalyzer {
	ha := &HistoryAnalyzer{
		UAST:  &plumbing.UASTChangesAnalyzer{},
		Ticks: &plumbing.TicksSinceStart{},
	}

	//nolint:errcheck // test helper; Initialize never errors.
	ha.Initialize(nil)

	return ha
}

func buildTestFunctionNode() *node.Node {
	return &node.Node{
		Type: node.UASTFile,
		Children: []*node.Node{
			{
				Type:  node.UASTFunction,
				Token: "testFunc",
				Pos:   &node.Positions{StartLine: 1, EndLine: 10},
				Children: []*node.Node{
					{
						Type: node.UASTIf,
						Pos:  &node.Positions{StartLine: 2, EndLine: 4},
						Children: []*node.Node{
							{Type: node.UASTIdentifier, Token: "x"},
						},
					},
					{
						Type:  node.UASTComment,
						Token: "This is a test comment for the function.",
						Pos:   &node.Positions{StartLine: 5, EndLine: 5},
					},
				},
			},
		},
	}
}

func TestAggregateCommitsToTicks_SingleCommitPerTick(t *testing.T) {
	t.Parallel()

	hashA := testHashA
	hashB := testHashB

	commitQuality := map[string]*TickQuality{
		hashA: {Complexities: []float64{10, 20}},
		hashB: {Complexities: []float64{30}},
	}
	commitsByTick := map[int][]gitlib.Hash{
		0: {gitlib.NewHash(hashA)},
		1: {gitlib.NewHash(hashB)},
	}

	result := AggregateCommitsToTicks(commitQuality, commitsByTick)

	require.Len(t, result, 2)
	assert.Len(t, result[0].Complexities, 2)
	assert.Len(t, result[1].Complexities, 1)
}

func TestAggregateCommitsToTicks_MultipleCommitsPerTick(t *testing.T) {
	t.Parallel()

	hashA := testHashA
	hashB := testHashB

	commitQuality := map[string]*TickQuality{
		hashA: {Complexities: []float64{10}, HalsteadVolumes: []float64{100}},
		hashB: {Complexities: []float64{20}, HalsteadVolumes: []float64{200}},
	}
	commitsByTick := map[int][]gitlib.Hash{
		0: {gitlib.NewHash(hashA), gitlib.NewHash(hashB)},
	}

	result := AggregateCommitsToTicks(commitQuality, commitsByTick)

	require.Len(t, result, 1)
	assert.Len(t, result[0].Complexities, 2)
	assert.Len(t, result[0].HalsteadVolumes, 2)
}

func TestAggregateCommitsToTicks_Empty(t *testing.T) {
	t.Parallel()

	result := AggregateCommitsToTicks(nil, nil)
	assert.Empty(t, result)
}

func TestAggregateCommitsToTicks_MissingCommit(t *testing.T) {
	t.Parallel()

	hashA := testHashA
	hashMissing := "cccccccccccccccccccccccccccccccccccccccc"

	commitQuality := map[string]*TickQuality{
		hashA: {Complexities: []float64{10}},
	}
	commitsByTick := map[int][]gitlib.Hash{
		0: {gitlib.NewHash(hashA), gitlib.NewHash(hashMissing)},
	}

	result := AggregateCommitsToTicks(commitQuality, commitsByTick)

	require.Len(t, result, 1)
	assert.Len(t, result[0].Complexities, 1)
}

func TestRegisterTickExtractor_Quality(t *testing.T) { //nolint:paralleltest // writes to global map
	report := analyze.Report{
		"commit_quality": map[string]*TickQuality{
			testHashA: {
				Complexities:    []float64{10, 20},
				Cognitives:      []float64{5, 10},
				MaxComplexities: []int{4, 8},
				Functions:       []int{2, 3},
				HalsteadVolumes: []float64{100, 200},
				HalsteadEfforts: []float64{50, 100},
				DeliveredBugs:   []float64{0.1, 0.2},
				CommentScores:   []float64{0.5, 0.7},
				DocCoverages:    []float64{0.4, 0.6},
				CohesionScores:  []float64{0.8, 0.9},
			},
			testHashB: {
				Complexities:    []float64{30},
				Cognitives:      []float64{15},
				MaxComplexities: []int{12},
				Functions:       []int{5},
				HalsteadVolumes: []float64{300},
				HalsteadEfforts: []float64{150},
				DeliveredBugs:   []float64{0.3},
				CommentScores:   []float64{0.9},
				DocCoverages:    []float64{0.8},
				CohesionScores:  []float64{1.0},
			},
		},
	}

	result := extractCommitTimeSeries(report)
	require.Len(t, result, 2)

	// Verify each commit hash maps to TickStats.
	statsA, ok := result[testHashA]
	require.True(t, ok)

	statsMap, ok := statsA.(TickStats)
	require.True(t, ok)
	assert.Equal(t, 2, statsMap.FilesAnalyzed)
	assert.InDelta(t, 15.0, statsMap.ComplexityMean, 0.01) // (10+20)/2.

	statsB, ok := result[testHashB]
	require.True(t, ok)

	statsBMap, ok := statsB.(TickStats)
	require.True(t, ok)
	assert.Equal(t, 1, statsBMap.FilesAnalyzed)
}

func buildTestQualityReport() analyze.Report {
	hashC1 := "c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1"
	hashC2 := "c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2"
	hashC3 := "c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3"

	return analyze.Report{
		"commit_quality": map[string]*TickQuality{
			hashC1: {
				Complexities:    []float64{10, 20, 30},
				Cognitives:      []float64{5, 10, 15},
				MaxComplexities: []int{4, 8, 12},
				Functions:       []int{2, 3, 5},
				HalsteadVolumes: []float64{100, 200, 300},
				HalsteadEfforts: []float64{50, 100, 150},
				DeliveredBugs:   []float64{0.1, 0.2, 0.3},
				CommentScores:   []float64{0.5, 0.7, 0.9},
				DocCoverages:    []float64{0.4, 0.6, 0.8},
				CohesionScores:  []float64{0.8, 0.9, 1.0},
			},
			hashC2: {
				Complexities:    []float64{15, 25},
				Cognitives:      []float64{7, 12},
				MaxComplexities: []int{6, 10},
				Functions:       []int{3, 4},
				HalsteadVolumes: []float64{150, 250},
				HalsteadEfforts: []float64{75, 125},
				DeliveredBugs:   []float64{0.2, 0.3},
				CommentScores:   []float64{0.6, 0.8},
				DocCoverages:    []float64{0.5, 0.7},
				CohesionScores:  []float64{0.85, 0.95},
			},
			hashC3: {
				Complexities:    []float64{5, 15, 25, 35},
				Cognitives:      []float64{3, 8, 13, 18},
				MaxComplexities: []int{3, 7, 10, 14},
				Functions:       []int{1, 3, 5, 7},
				HalsteadVolumes: []float64{50, 150, 250, 350},
				HalsteadEfforts: []float64{25, 75, 125, 175},
				DeliveredBugs:   []float64{0.05, 0.15, 0.25, 0.35},
				CommentScores:   []float64{0.4, 0.6, 0.8, 1.0},
				DocCoverages:    []float64{0.3, 0.5, 0.7, 0.9},
				CohesionScores:  []float64{0.7, 0.8, 0.9, 1.0},
			},
		},
		"commits_by_tick": map[int][]gitlib.Hash{
			0: {gitlib.NewHash(hashC1)},
			1: {gitlib.NewHash(hashC2)},
			2: {gitlib.NewHash(hashC3)},
		},
	}
}
