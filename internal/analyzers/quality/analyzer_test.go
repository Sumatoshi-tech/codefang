package quality

import (
	"context"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/plumbing"
	pkgplumbing "github.com/Sumatoshi-tech/codefang/internal/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
	"github.com/Sumatoshi-tech/codefang/pkg/uast"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

// Test hash constants to avoid magic strings.
const (
	testHashA = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
	testHashB = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
)

func TestAnalyzer_Configure(t *testing.T) {
	t.Parallel()

	ha := NewAnalyzer()
	facts := map[string]any{
		pkgplumbing.FactCommitsByTick: map[int][]gitlib.Hash{},
	}

	err := ha.Configure(facts)
	require.NoError(t, err)
	assert.NotNil(t, ha.commitsByTick)
}

func TestAnalyzer_Initialize(t *testing.T) {
	t.Parallel()

	ha := NewAnalyzer()

	err := ha.Initialize(nil)
	require.NoError(t, err)

	assert.NotNil(t, ha.complexityAnalyzer)
	assert.NotNil(t, ha.halsteadAnalyzer)
	assert.NotNil(t, ha.commentsAnalyzer)
	assert.NotNil(t, ha.cohesionAnalyzer)
}

func TestAnalyzer_Consume_ReturnsTCWithTickQuality(t *testing.T) {
	t.Parallel()

	ha := newTestAnalyzer()
	funcNode := buildTestFunctionNode()
	hash := gitlib.NewHash(testHashA)

	ha.UAST.SetChangesForTest([]uast.Change{
		{After: funcNode},
	})
	ha.Ticks.Tick = 0

	tc, err := ha.Consume(context.Background(), &analyze.Context{
		Commit: gitlib.NewCommitForTest(hash),
	})
	require.NoError(t, err)

	tq, isTQ := tc.Data.(*TickQuality)
	require.True(t, isTQ, "TC.Data must be *TickQuality")
	require.NotNil(t, tq)
	assert.GreaterOrEqual(t, tq.filesAnalyzed(), 1)
	assert.Equal(t, hash, tc.CommitHash)
}

func TestAnalyzer_Consume_EmptyChanges(t *testing.T) {
	t.Parallel()

	ha := newTestAnalyzer()

	ha.UAST.SetChangesForTest(nil)
	ha.Ticks.Tick = 0

	tc, err := ha.Consume(context.Background(), &analyze.Context{})
	require.NoError(t, err)

	tq, isTQ := tc.Data.(*TickQuality)
	require.True(t, isTQ, "TC.Data must be *TickQuality")
	assert.Equal(t, 0, tq.filesAnalyzed())
}

func TestAnalyzer_Consume_DeletedFile(t *testing.T) {
	t.Parallel()

	ha := newTestAnalyzer()
	hash := gitlib.NewHash(testHashA)

	// Deleted file: After is nil.
	ha.UAST.SetChangesForTest([]uast.Change{
		{Before: &node.Node{Type: node.UASTFile}, After: nil},
	})
	ha.Ticks.Tick = 0

	tc, err := ha.Consume(context.Background(), &analyze.Context{
		Commit: gitlib.NewCommitForTest(hash),
	})
	require.NoError(t, err)

	tq, isTQ := tc.Data.(*TickQuality)
	require.True(t, isTQ)
	assert.Equal(t, 0, tq.filesAnalyzed())
}

func TestAnalyzer_Consume_MultipleFiles(t *testing.T) {
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

	tc, err := ha.Consume(context.Background(), &analyze.Context{
		Commit: gitlib.NewCommitForTest(hash),
	})
	require.NoError(t, err)

	tq, isTQ := tc.Data.(*TickQuality)
	require.True(t, isTQ)
	assert.GreaterOrEqual(t, tq.filesAnalyzed(), 2)
}

func TestAnalyzer_Consume_NoAccumulation(t *testing.T) {
	t.Parallel()

	ha := newTestAnalyzer()
	funcNode := buildTestFunctionNode()

	hash1 := gitlib.NewHash(testHashA)
	hash2 := gitlib.NewHash(testHashB)

	// Commit 1.
	ha.UAST.SetChangesForTest([]uast.Change{{After: funcNode}})
	ha.Ticks.Tick = 0

	tc1, err := ha.Consume(context.Background(), &analyze.Context{
		Commit: gitlib.NewCommitForTest(hash1),
	})
	require.NoError(t, err)

	// Commit 2.
	ha.UAST.SetChangesForTest([]uast.Change{{After: funcNode}})
	ha.Ticks.Tick = 1

	tc2, err := ha.Consume(context.Background(), &analyze.Context{
		Commit: gitlib.NewCommitForTest(hash2),
	})
	require.NoError(t, err)

	// Both TCs should have independent TickQuality data.
	tq1, ok1 := tc1.Data.(*TickQuality)
	require.True(t, ok1)

	tq2, ok2 := tc2.Data.(*TickQuality)
	require.True(t, ok2)

	assert.GreaterOrEqual(t, tq1.filesAnalyzed(), 1)
	assert.GreaterOrEqual(t, tq2.filesAnalyzed(), 1)
}

func TestAnalyzer_Fork_IndependentSubAnalyzers(t *testing.T) {
	t.Parallel()

	ha := newTestAnalyzer()

	forks := ha.Fork(2)
	require.Len(t, forks, 2)

	fork1, ok1 := forks[0].(*Analyzer)
	require.True(t, ok1)

	fork2, ok2 := forks[1].(*Analyzer)
	require.True(t, ok2)

	// Plumbing deps should be independent instances.
	assert.NotSame(t, ha.UAST, fork1.UAST)
	assert.NotSame(t, ha.Ticks, fork1.Ticks)
	assert.NotSame(t, fork1.UAST, fork2.UAST)

	// Static analyzers should be independent.
	assert.NotSame(t, ha.complexityAnalyzer, fork1.complexityAnalyzer)
	assert.NotSame(t, fork1.complexityAnalyzer, fork2.complexityAnalyzer)
}

func TestAnalyzer_Merge_IsNoOp(t *testing.T) {
	t.Parallel()

	ha := newTestAnalyzer()
	branch := newTestAnalyzer()

	// Merge should not panic.
	ha.Merge([]analyze.HistoryAnalyzer{branch})
}

func TestAnalyzer_Fork_SharesCommitsByTick(t *testing.T) {
	t.Parallel()

	ha := newTestAnalyzer()
	ha.commitsByTick = map[int][]gitlib.Hash{
		0: {gitlib.NewHash(testHashA)},
	}

	forks := ha.Fork(1)
	require.Len(t, forks, 1)

	fork1, ok := forks[0].(*Analyzer)
	require.True(t, ok)

	// commitsByTick is shared read-only.
	assert.Equal(t, ha.commitsByTick, fork1.commitsByTick)
}

func TestAnalyzer_SequentialOnly(t *testing.T) {
	t.Parallel()

	ha := NewAnalyzer()
	assert.False(t, ha.SequentialOnly())
}

func TestAnalyzer_CPUHeavy(t *testing.T) {
	t.Parallel()

	ha := NewAnalyzer()
	assert.True(t, ha.CPUHeavy())
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

func TestComputeAllMetrics_FromCanonical(t *testing.T) {
	t.Parallel()

	hash := "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
	report := analyze.Report{
		"commit_quality": map[string]*TickQuality{
			hash: {
				Complexities:    []float64{10, 20},
				HalsteadVolumes: []float64{100, 200},
				DeliveredBugs:   []float64{0.1, 0.2},
				CommentScores:   []float64{0.5, 0.7},
				CohesionScores:  []float64{0.8, 0.9},
			},
		},
		"commits_by_tick": map[int][]gitlib.Hash{
			0: {gitlib.NewHash(hash)},
		},
	}

	computed, err := ComputeAllMetrics(report)
	require.NoError(t, err)

	assert.Len(t, computed.TimeSeries, 1)
	assert.Equal(t, 1, computed.Aggregate.TotalTicks)
	assert.Equal(t, 2, computed.Aggregate.TotalFilesAnalyzed)
}

func TestComputeAllMetrics_Empty(t *testing.T) {
	t.Parallel()

	report := analyze.Report{
		"commit_quality":  map[string]*TickQuality{},
		"commits_by_tick": map[int][]gitlib.Hash{},
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

func newTestAnalyzer() *Analyzer {
	ha := NewAnalyzer()
	ha.UAST = &plumbing.UASTChangesAnalyzer{}
	ha.Ticks = &plumbing.TicksSinceStart{}

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
