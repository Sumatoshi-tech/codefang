package anomaly

import (
	"context"
	"fmt"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/plumbing"
	pkgplumbing "github.com/Sumatoshi-tech/codefang/internal/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
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
		ConfigAnomalyThreshold:        float32(3.0),
		ConfigAnomalyWindowSize:       30,
		pkgplumbing.FactCommitsByTick: map[int][]gitlib.Hash{},
	}

	err := ha.Configure(facts)
	require.NoError(t, err)

	assert.InDelta(t, float32(3.0), ha.Threshold, 0.001)
	assert.Equal(t, 30, ha.WindowSize)
	assert.NotNil(t, ha.commitsByTick)
}

func TestAnalyzer_Configure_Validation(t *testing.T) {
	t.Parallel()

	ha := NewAnalyzer()
	// Invalid values: negative threshold, too-small window.
	facts := map[string]any{
		ConfigAnomalyThreshold:  float32(-1.0),
		ConfigAnomalyWindowSize: 0,
	}

	err := ha.Configure(facts)
	require.NoError(t, err)

	// Should fall back to defaults.
	assert.InDelta(t, DefaultAnomalyThreshold, ha.Threshold, 0.001)
	assert.Equal(t, DefaultAnomalyWindowSize, ha.WindowSize)
}

func TestAnalyzer_Initialize(t *testing.T) {
	t.Parallel()

	ha := NewAnalyzer()

	err := ha.Initialize(nil)
	require.NoError(t, err)

	assert.InDelta(t, DefaultAnomalyThreshold, ha.Threshold, 0.001)
	assert.Equal(t, DefaultAnomalyWindowSize, ha.WindowSize)
}

func TestAnalyzer_Consume_ReturnsTCWithCommitData(t *testing.T) {
	t.Parallel()

	ha := newTestAnalyzer(t)

	ha.TreeDiff.Changes = gitlib.Changes{
		{Action: gitlib.Modify, To: gitlib.ChangeEntry{Name: "main.go"}},
		{Action: gitlib.Insert, To: gitlib.ChangeEntry{Name: "util.go"}},
	}
	ha.Ticks.Tick = 0
	ha.LineStats.LineStats = map[gitlib.ChangeEntry]pkgplumbing.LineStats{
		{Name: "main.go"}: {Added: 10, Removed: 3},
		{Name: "util.go"}: {Added: 50, Removed: 0},
	}

	hash := gitlib.NewHash(testHashA)
	commit := gitlib.NewTestCommit(hash, gitlib.TestSignature("dev", "dev@test.com"), "test")

	tc, err := ha.Consume(context.Background(), &analyze.Context{Commit: commit})
	require.NoError(t, err)

	cm, isCM := tc.Data.(*CommitAnomalyData)
	require.True(t, isCM, "TC.Data must be *CommitAnomalyData")
	require.NotNil(t, cm)
	assert.Equal(t, 2, cm.FilesChanged)
	assert.Equal(t, 60, cm.LinesAdded)
	assert.Equal(t, 3, cm.LinesRemoved)
	assert.Equal(t, 57, cm.NetChurn)
	assert.Len(t, cm.Files, 2)
	assert.Equal(t, hash, tc.CommitHash)
}

func TestAnalyzer_Consume_EmptyCommit(t *testing.T) {
	t.Parallel()

	ha := newTestAnalyzer(t)

	ha.TreeDiff.Changes = nil
	ha.Ticks.Tick = 0
	ha.LineStats.LineStats = nil

	hash := gitlib.NewHash(testHashA)
	commit := gitlib.NewTestCommit(hash, gitlib.TestSignature("dev", "dev@test.com"), "test")

	tc, err := ha.Consume(context.Background(), &analyze.Context{Commit: commit})
	require.NoError(t, err)

	cm, isCM := tc.Data.(*CommitAnomalyData)
	require.True(t, isCM, "TC.Data must be *CommitAnomalyData")
	require.NotNil(t, cm)
	assert.Equal(t, 0, cm.FilesChanged)
	assert.Equal(t, 0, cm.LinesAdded)
	assert.Equal(t, 0, cm.NetChurn)
}

func TestAnalyzer_Consume_NilContext(t *testing.T) {
	t.Parallel()

	ha := newTestAnalyzer(t)

	tc, err := ha.Consume(context.Background(), nil)
	require.NoError(t, err)
	assert.Equal(t, analyze.TC{}, tc)
}

func TestAnalyzer_Consume_NoAccumulation(t *testing.T) {
	t.Parallel()

	ha := newTestAnalyzer(t)

	hash1 := gitlib.NewHash(testHashA)
	hash2 := gitlib.NewHash(testHashB)

	// Commit 1 on tick 0.
	ha.TreeDiff.Changes = gitlib.Changes{
		{Action: gitlib.Modify, To: gitlib.ChangeEntry{Name: "a.go"}},
	}
	ha.Ticks.Tick = 0
	ha.LineStats.LineStats = map[gitlib.ChangeEntry]pkgplumbing.LineStats{
		{Name: "a.go"}: {Added: 5, Removed: 2},
	}
	commit1 := gitlib.NewTestCommit(hash1, gitlib.TestSignature("dev", "dev@test.com"), "c1")

	tc1, err := ha.Consume(context.Background(), &analyze.Context{Commit: commit1})
	require.NoError(t, err)

	// Commit 2 on tick 0.
	ha.TreeDiff.Changes = gitlib.Changes{
		{Action: gitlib.Insert, To: gitlib.ChangeEntry{Name: "b.go"}},
	}
	ha.LineStats.LineStats = map[gitlib.ChangeEntry]pkgplumbing.LineStats{
		{Name: "b.go"}: {Added: 10, Removed: 0},
	}
	commit2 := gitlib.NewTestCommit(hash2, gitlib.TestSignature("dev", "dev@test.com"), "c2")

	tc2, err := ha.Consume(context.Background(), &analyze.Context{Commit: commit2})
	require.NoError(t, err)

	// Both TCs should have independent data.
	cm1, ok1 := tc1.Data.(*CommitAnomalyData)
	require.True(t, ok1)

	cm2, ok2 := tc2.Data.(*CommitAnomalyData)
	require.True(t, ok2)

	assert.Equal(t, 1, cm1.FilesChanged)
	assert.Equal(t, 5, cm1.LinesAdded)
	assert.Equal(t, 1, cm2.FilesChanged)
	assert.Equal(t, 10, cm2.LinesAdded)
}

func TestAnalyzer_Consume_LanguageAndAuthor(t *testing.T) {
	t.Parallel()

	ha := newTestAnalyzer(t)

	blobHash1 := gitlib.Hash{0x01}
	blobHash2 := gitlib.Hash{0x02}

	ha.TreeDiff.Changes = gitlib.Changes{
		{Action: gitlib.Modify, To: gitlib.ChangeEntry{Name: "main.go", Hash: blobHash1}},
		{Action: gitlib.Insert, To: gitlib.ChangeEntry{Name: "util.py", Hash: blobHash2}},
	}
	ha.Ticks.Tick = 0
	ha.LineStats.LineStats = map[gitlib.ChangeEntry]pkgplumbing.LineStats{
		{Name: "main.go"}: {Added: 10, Removed: 3},
		{Name: "util.py"}: {Added: 5, Removed: 0},
	}
	ha.Languages.SetLanguagesForTest(map[gitlib.Hash]string{
		blobHash1: "Go",
		blobHash2: "Python",
	})
	ha.Identity.AuthorID = 42

	commitHash := gitlib.NewHash(testHashA)
	commit := gitlib.NewTestCommit(commitHash, gitlib.TestSignature("dev", "dev@test.com"), "test")

	tc, err := ha.Consume(context.Background(), &analyze.Context{Commit: commit})
	require.NoError(t, err)

	cm, isCM := tc.Data.(*CommitAnomalyData)
	require.True(t, isCM)
	require.NotNil(t, cm)
	assert.Len(t, cm.Languages, 2)
	assert.Equal(t, 1, cm.Languages["Go"])
	assert.Equal(t, 1, cm.Languages["Python"])
	assert.Equal(t, 42, cm.AuthorID)
}

func TestComputeAllMetrics_Basic(t *testing.T) {
	t.Parallel()

	report := buildTestReport()

	computed, err := ComputeAllMetrics(report)
	require.NoError(t, err)

	assert.NotNil(t, computed.Anomalies)
	assert.NotNil(t, computed.TimeSeries)
	assert.Positive(t, computed.Aggregate.TotalTicks)
}

func TestComputeAllMetrics_WithAnomaly(t *testing.T) {
	t.Parallel()

	report := buildTestReportWithSpike()

	computed, err := ComputeAllMetrics(report)
	require.NoError(t, err)

	assert.NotEmpty(t, computed.Anomalies, "should detect the spike")
	assert.Positive(t, computed.Aggregate.TotalAnomalies)
	assert.Positive(t, computed.Aggregate.AnomalyRate)
}

func TestZScoreSet_MaxAbs(t *testing.T) {
	t.Parallel()

	zs := ZScoreSet{
		NetChurn:          1.5,
		FilesChanged:      -3.0,
		LinesAdded:        2.0,
		LinesRemoved:      0.5,
		LanguageDiversity: 1.0,
		AuthorCount:       0.3,
	}

	assert.InDelta(t, 3.0, zs.MaxAbs(), 1e-9)

	zs2 := ZScoreSet{
		NetChurn:          1.0,
		FilesChanged:      1.0,
		LinesAdded:        1.0,
		LinesRemoved:      1.0,
		LanguageDiversity: -5.0,
		AuthorCount:       0.5,
	}

	assert.InDelta(t, 5.0, zs2.MaxAbs(), 1e-9)
}

func TestExtractCommitTimeSeries_Anomaly(t *testing.T) {
	t.Parallel()

	h := NewAnalyzer()

	report := analyze.Report{
		"commit_metrics": map[string]*CommitAnomalyData{
			testHashA: {
				FilesChanged: 10, LinesAdded: 100, LinesRemoved: 20, NetChurn: 80,
			},
		},
	}

	result := h.ExtractCommitTimeSeries(report)
	require.Len(t, result, 1)

	statsA, ok := result[testHashA]
	require.True(t, ok)

	statsMap, ok := statsA.(*CommitAnomalyData)
	require.True(t, ok)
	assert.Equal(t, 10, statsMap.FilesChanged)
	assert.Equal(t, 80, statsMap.NetChurn)
}

func TestAggregateCommitsToTicks_Basic(t *testing.T) {
	t.Parallel()

	h1 := gitlib.NewHash(testHashA)
	h2 := gitlib.NewHash(testHashB)

	commitMetrics := map[string]*CommitAnomalyData{
		h1.String(): {
			FilesChanged: 3, LinesAdded: 20, LinesRemoved: 5,
			Files: []string{"a.go", "b.go", "c.go"}, Languages: map[string]int{"Go": 3}, AuthorID: 1,
		},
		h2.String(): {
			FilesChanged: 2, LinesAdded: 10, LinesRemoved: 3,
			Files: []string{"d.go", "e.go"}, Languages: map[string]int{"Go": 1, "Python": 1}, AuthorID: 2,
		},
	}
	commitsByTick := map[int][]gitlib.Hash{
		0: {h1, h2},
	}

	result := AggregateCommitsToTicks(commitMetrics, commitsByTick)
	require.Len(t, result, 1)

	tm := result[0]
	assert.Equal(t, 5, tm.FilesChanged)
	assert.Equal(t, 30, tm.LinesAdded)
	assert.Equal(t, 8, tm.LinesRemoved)
	assert.Equal(t, 22, tm.NetChurn)
	assert.Len(t, tm.Files, 5)
	assert.Equal(t, 4, tm.Languages["Go"])
	assert.Equal(t, 1, tm.Languages["Python"])
	assert.Len(t, tm.AuthorIDs, 2)
}

func TestAggregateCommitsToTicks_MultipleTicks(t *testing.T) {
	t.Parallel()

	h1 := gitlib.NewHash(testHashA)
	h2 := gitlib.NewHash(testHashB)

	commitMetrics := map[string]*CommitAnomalyData{
		h1.String(): {FilesChanged: 3, LinesAdded: 20, LinesRemoved: 5, AuthorID: 1},
		h2.String(): {FilesChanged: 2, LinesAdded: 10, LinesRemoved: 3, AuthorID: 2},
	}
	commitsByTick := map[int][]gitlib.Hash{
		0: {h1},
		1: {h2},
	}

	result := AggregateCommitsToTicks(commitMetrics, commitsByTick)
	require.Len(t, result, 2)
	assert.Equal(t, 3, result[0].FilesChanged)
	assert.Equal(t, 2, result[1].FilesChanged)
}

func TestAggregateCommitsToTicks_EmptyInputs(t *testing.T) {
	t.Parallel()

	assert.Nil(t, AggregateCommitsToTicks(nil, map[int][]gitlib.Hash{0: {}}))
	assert.Nil(t, AggregateCommitsToTicks(map[string]*CommitAnomalyData{"a": {}}, nil))
}

func TestAggregateCommitsToTicks_MissingCommit(t *testing.T) {
	t.Parallel()

	h1 := gitlib.NewHash(testHashA)
	hMissing := gitlib.NewHash("cccccccccccccccccccccccccccccccccccccccc")

	commitMetrics := map[string]*CommitAnomalyData{
		h1.String(): {FilesChanged: 3, LinesAdded: 20, LinesRemoved: 5, AuthorID: 1},
	}
	commitsByTick := map[int][]gitlib.Hash{
		0: {h1, hMissing},
	}

	result := AggregateCommitsToTicks(commitMetrics, commitsByTick)
	require.Len(t, result, 1)
	assert.Equal(t, 3, result[0].FilesChanged)
}

func TestComputeAllMetrics_FromCommitData(t *testing.T) {
	t.Parallel()

	h1 := gitlib.NewHash(testHashA)
	h2 := gitlib.NewHash(testHashB)
	h3 := gitlib.NewHash("cccccccccccccccccccccccccccccccccccccccc")

	report := analyze.Report{
		"anomalies": []Record{},
		"commit_metrics": map[string]*CommitAnomalyData{
			h1.String(): {FilesChanged: 5, LinesAdded: 20, LinesRemoved: 10, Languages: map[string]int{"Go": 3}, AuthorID: 0},
			h2.String(): {FilesChanged: 3, LinesAdded: 15, LinesRemoved: 8, Languages: map[string]int{"Go": 3}, AuthorID: 1},
			h3.String(): {FilesChanged: 4, LinesAdded: 18, LinesRemoved: 9, Languages: map[string]int{"Go": 2, "Rust": 2}, AuthorID: 0},
		},
		"commits_by_tick": map[int][]gitlib.Hash{
			0: {h1},
			1: {h2},
			2: {h3},
		},
		"threshold":   float32(2.0),
		"window_size": 20,
	}

	computed, err := ComputeAllMetrics(report)
	require.NoError(t, err)
	assert.NotNil(t, computed.TimeSeries)
	assert.Positive(t, computed.Aggregate.TotalTicks)
	assert.Equal(t, 3, computed.Aggregate.TotalTicks)
}

// --- Helpers ---.

func newTestAnalyzer(tb testing.TB) *Analyzer {
	tb.Helper()

	treeDiff := &plumbing.TreeDiffAnalyzer{}
	blobCache := &plumbing.BlobCacheAnalyzer{TreeDiff: treeDiff}

	ha := &Analyzer{
		TreeDiff:  treeDiff,
		Ticks:     &plumbing.TicksSinceStart{},
		LineStats: &plumbing.LinesStatsCalculator{},
		Languages: &plumbing.LanguagesDetectionAnalyzer{TreeDiff: treeDiff, BlobCache: blobCache},
		Identity:  &plumbing.IdentityDetector{},
	}

	require.NoError(tb, ha.Initialize(nil))

	return ha
}

func buildTestReport() analyze.Report {
	hash0 := "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
	hash1 := "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
	hash2 := "cccccccccccccccccccccccccccccccccccccccc"

	commitMetrics := map[string]*CommitAnomalyData{
		hash0: {
			FilesChanged: 5, LinesAdded: 20, LinesRemoved: 10, NetChurn: 10,
			Files: []string{"main.go"}, Languages: map[string]int{"Go": 3, "Python": 2}, AuthorID: 0,
		},
		hash1: {
			FilesChanged: 3, LinesAdded: 15, LinesRemoved: 8, NetChurn: 7,
			Files: []string{"util.go"}, Languages: map[string]int{"Go": 3}, AuthorID: 1,
		},
		hash2: {
			FilesChanged: 4, LinesAdded: 18, LinesRemoved: 9, NetChurn: 9,
			Files: []string{"lib.go"}, Languages: map[string]int{"Go": 2, "Rust": 2}, AuthorID: 0,
		},
	}
	commitsByTick := map[int][]gitlib.Hash{
		0: {gitlib.NewHash(hash0)},
		1: {gitlib.NewHash(hash1)},
		2: {gitlib.NewHash(hash2)},
	}

	return analyze.Report{
		"anomalies":       []Record{},
		"commit_metrics":  commitMetrics,
		"commits_by_tick": commitsByTick,
		"threshold":       float32(2.0),
		"window_size":     20,
	}
}

func buildTestReportWithSpike() analyze.Report {
	commitMetrics := make(map[string]*CommitAnomalyData)
	commitsByTick := make(map[int][]gitlib.Hash)

	// 10 stable ticks.
	for tick := range 10 {
		hash := testHashForTick(tick)
		commitMetrics[hash] = &CommitAnomalyData{
			FilesChanged: 5, LinesAdded: 20, LinesRemoved: 10, NetChurn: 10,
			Files:     []string{"main.go"},
			Languages: map[string]int{"Go": 5},
			AuthorID:  0,
		}
		commitsByTick[tick] = []gitlib.Hash{gitlib.NewHash(hash)}
	}

	// Spike at tick 10.
	hashSpike := testHashForTick(10)
	commitMetrics[hashSpike] = &CommitAnomalyData{
		FilesChanged: 200, LinesAdded: 5000, LinesRemoved: 50, NetChurn: 4950,
		Files:     []string{"huge.go"},
		Languages: map[string]int{"Go": 50, "Python": 30, "Shell": 20, "YAML": 100},
		AuthorID:  0,
	}
	commitsByTick[10] = []gitlib.Hash{gitlib.NewHash(hashSpike)}

	tickMetrics := AggregateCommitsToTicks(commitMetrics, commitsByTick)
	anomalies := detectAnomaliesFromTicks(tickMetrics, 2.0, 5)

	return analyze.Report{
		"anomalies":       anomalies,
		"commit_metrics":  commitMetrics,
		"commits_by_tick": commitsByTick,
		"threshold":       float32(2.0),
		"window_size":     5,
	}
}

func testHashForTick(tick int) string {
	return fmt.Sprintf("%038x%02x", 0, tick)
}
