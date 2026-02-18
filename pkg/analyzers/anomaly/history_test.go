package anomaly

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
	pkgplumbing "github.com/Sumatoshi-tech/codefang/pkg/plumbing"
)

func TestHistoryAnalyzer_Descriptor(t *testing.T) {
	t.Parallel()

	ha := &HistoryAnalyzer{}
	desc := ha.Descriptor()

	assert.Equal(t, "history/anomaly", desc.ID)
	assert.Equal(t, analyze.ModeHistory, desc.Mode)
	assert.NotEmpty(t, desc.Description)
}

func TestHistoryAnalyzer_NameAndFlag(t *testing.T) {
	t.Parallel()

	ha := &HistoryAnalyzer{}

	assert.Equal(t, "TemporalAnomaly", ha.Name())
	assert.Equal(t, "anomaly", ha.Flag())
}

func TestHistoryAnalyzer_Configure(t *testing.T) {
	t.Parallel()

	ha := &HistoryAnalyzer{}
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

func TestHistoryAnalyzer_Configure_Validation(t *testing.T) {
	t.Parallel()

	ha := &HistoryAnalyzer{}
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

func TestHistoryAnalyzer_Initialize(t *testing.T) {
	t.Parallel()

	ha := &HistoryAnalyzer{}

	err := ha.Initialize(nil)
	require.NoError(t, err)

	assert.NotNil(t, ha.commitMetrics)
	assert.InDelta(t, DefaultAnomalyThreshold, ha.Threshold, 0.001)
	assert.Equal(t, DefaultAnomalyWindowSize, ha.WindowSize)
}

func TestHistoryAnalyzer_Consume_BasicCommit(t *testing.T) {
	t.Parallel()

	ha := newTestAnalyzer()

	// Simulate a commit with 2 file changes and some line stats.
	ha.TreeDiff.Changes = gitlib.Changes{
		{Action: gitlib.Modify, To: gitlib.ChangeEntry{Name: "main.go"}},
		{Action: gitlib.Insert, To: gitlib.ChangeEntry{Name: "util.go"}},
	}
	ha.Ticks.Tick = 0
	ha.LineStats.LineStats = map[gitlib.ChangeEntry]pkgplumbing.LineStats{
		{Name: "main.go"}: {Added: 10, Removed: 3},
		{Name: "util.go"}: {Added: 50, Removed: 0},
	}

	hash := gitlib.NewHash("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
	commit := gitlib.NewTestCommit(hash, gitlib.TestSignature("dev", "dev@test.com"), "test")

	err := ha.Consume(context.Background(), &analyze.Context{Commit: commit})
	require.NoError(t, err)

	cm := ha.commitMetrics[hash.String()]
	require.NotNil(t, cm)
	assert.Equal(t, 2, cm.FilesChanged)
	assert.Equal(t, 60, cm.LinesAdded)
	assert.Equal(t, 3, cm.LinesRemoved)
	assert.Equal(t, 57, cm.NetChurn)
	assert.Len(t, cm.Files, 2)
}

func TestHistoryAnalyzer_Consume_EmptyCommit(t *testing.T) {
	t.Parallel()

	ha := newTestAnalyzer()

	ha.TreeDiff.Changes = nil
	ha.Ticks.Tick = 0
	ha.LineStats.LineStats = nil

	hash := gitlib.NewHash("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
	commit := gitlib.NewTestCommit(hash, gitlib.TestSignature("dev", "dev@test.com"), "test")

	err := ha.Consume(context.Background(), &analyze.Context{Commit: commit})
	require.NoError(t, err)

	cm := ha.commitMetrics[hash.String()]
	require.NotNil(t, cm)
	assert.Equal(t, 0, cm.FilesChanged)
	assert.Equal(t, 0, cm.LinesAdded)
	assert.Equal(t, 0, cm.NetChurn)
}

func TestHistoryAnalyzer_Consume_MultipleCommitsSameTick(t *testing.T) {
	t.Parallel()

	ha := newTestAnalyzer()

	hash1 := gitlib.NewHash("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
	hash2 := gitlib.NewHash("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")

	// First commit on tick 0.
	ha.TreeDiff.Changes = gitlib.Changes{
		{Action: gitlib.Modify, To: gitlib.ChangeEntry{Name: "a.go"}},
	}
	ha.Ticks.Tick = 0
	ha.LineStats.LineStats = map[gitlib.ChangeEntry]pkgplumbing.LineStats{
		{Name: "a.go"}: {Added: 5, Removed: 2},
	}
	commit1 := gitlib.NewTestCommit(hash1, gitlib.TestSignature("dev", "dev@test.com"), "c1")

	require.NoError(t, ha.Consume(context.Background(), &analyze.Context{Commit: commit1}))

	// Second commit on tick 0.
	ha.TreeDiff.Changes = gitlib.Changes{
		{Action: gitlib.Insert, To: gitlib.ChangeEntry{Name: "b.go"}},
	}
	ha.LineStats.LineStats = map[gitlib.ChangeEntry]pkgplumbing.LineStats{
		{Name: "b.go"}: {Added: 10, Removed: 0},
	}
	commit2 := gitlib.NewTestCommit(hash2, gitlib.TestSignature("dev", "dev@test.com"), "c2")

	require.NoError(t, ha.Consume(context.Background(), &analyze.Context{Commit: commit2}))

	require.Len(t, ha.commitMetrics, 2)

	cm1 := ha.commitMetrics[hash1.String()]
	assert.Equal(t, 1, cm1.FilesChanged)
	assert.Equal(t, 5, cm1.LinesAdded)
	assert.Equal(t, 2, cm1.LinesRemoved)

	cm2 := ha.commitMetrics[hash2.String()]
	assert.Equal(t, 1, cm2.FilesChanged)
	assert.Equal(t, 10, cm2.LinesAdded)
	assert.Equal(t, 0, cm2.LinesRemoved)
}

func TestHistoryAnalyzer_Consume_LanguageAndAuthor(t *testing.T) {
	t.Parallel()

	ha := newTestAnalyzer()

	// Set up language detection with known hashes.
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

	commitHash := gitlib.NewHash("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
	commit := gitlib.NewTestCommit(commitHash, gitlib.TestSignature("dev", "dev@test.com"), "test")

	err := ha.Consume(context.Background(), &analyze.Context{Commit: commit})
	require.NoError(t, err)

	cm := ha.commitMetrics[commitHash.String()]
	require.NotNil(t, cm)
	assert.Len(t, cm.Languages, 2)
	assert.Equal(t, 1, cm.Languages["Go"])
	assert.Equal(t, 1, cm.Languages["Python"])
	assert.Equal(t, 42, cm.AuthorID)
}

func TestHistoryAnalyzer_Consume_MultipleAuthorsSameTick(t *testing.T) {
	t.Parallel()

	ha := newTestAnalyzer()

	hash1 := gitlib.NewHash("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
	hash2 := gitlib.NewHash("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")

	// First commit by author 1.
	ha.TreeDiff.Changes = gitlib.Changes{
		{Action: gitlib.Modify, To: gitlib.ChangeEntry{Name: "a.go"}},
	}
	ha.Ticks.Tick = 0
	ha.LineStats.LineStats = map[gitlib.ChangeEntry]pkgplumbing.LineStats{
		{Name: "a.go"}: {Added: 5, Removed: 2},
	}
	ha.Identity.AuthorID = 1
	commit1 := gitlib.NewTestCommit(hash1, gitlib.TestSignature("dev1", "dev1@test.com"), "c1")

	require.NoError(t, ha.Consume(context.Background(), &analyze.Context{Commit: commit1}))

	// Second commit by author 2 on same tick.
	ha.TreeDiff.Changes = gitlib.Changes{
		{Action: gitlib.Insert, To: gitlib.ChangeEntry{Name: "b.go"}},
	}
	ha.LineStats.LineStats = map[gitlib.ChangeEntry]pkgplumbing.LineStats{
		{Name: "b.go"}: {Added: 10, Removed: 0},
	}
	ha.Identity.AuthorID = 2
	commit2 := gitlib.NewTestCommit(hash2, gitlib.TestSignature("dev2", "dev2@test.com"), "c2")

	require.NoError(t, ha.Consume(context.Background(), &analyze.Context{Commit: commit2}))

	require.Len(t, ha.commitMetrics, 2)
	assert.Equal(t, 1, ha.commitMetrics[hash1.String()].AuthorID)
	assert.Equal(t, 2, ha.commitMetrics[hash2.String()].AuthorID)
}

func TestHistoryAnalyzer_Finalize_EmptyHistory(t *testing.T) {
	t.Parallel()

	ha := newTestAnalyzer()

	report, err := ha.Finalize()
	require.NoError(t, err)

	anomalies, ok := report["anomalies"].([]Record)
	require.True(t, ok)
	assert.Empty(t, anomalies)
}

func TestHistoryAnalyzer_Finalize_NoAnomalies(t *testing.T) {
	t.Parallel()

	ha := newTestAnalyzer()

	// Feed 10 ticks with stable metrics via per-commit data.
	ha.commitsByTick = make(map[int][]gitlib.Hash)

	for tick := range 10 {
		hash := gitlib.NewHash(fmt.Sprintf("aa%038d", tick))
		ha.commitMetrics[hash.String()] = &CommitAnomalyData{
			FilesChanged: 5,
			LinesAdded:   20,
			LinesRemoved: 10,
			NetChurn:     10,
			Files:        []string{"main.go"},
		}
		ha.commitsByTick[tick] = []gitlib.Hash{hash}
	}

	report, err := ha.Finalize()
	require.NoError(t, err)

	anomalies, ok := report["anomalies"].([]Record)
	require.True(t, ok)
	assert.Empty(t, anomalies)
}

func TestHistoryAnalyzer_Finalize_SpikeDetected(t *testing.T) {
	t.Parallel()

	ha := newTestAnalyzer()
	ha.WindowSize = 5
	ha.Threshold = 2.0

	ha.commitsByTick = make(map[int][]gitlib.Hash)

	// Feed 10 stable ticks followed by a spike.
	stableTicks := 10
	for tick := range stableTicks {
		hash := gitlib.NewHash(fmt.Sprintf("aa%038d", tick))
		ha.commitMetrics[hash.String()] = &CommitAnomalyData{
			FilesChanged: 5,
			LinesAdded:   20,
			LinesRemoved: 10,
			NetChurn:     10,
			Files:        []string{"main.go"},
		}
		ha.commitsByTick[tick] = []gitlib.Hash{hash}
	}

	// Spike at tick 10: massive churn.
	spikeHash := gitlib.NewHash(fmt.Sprintf("bb%038d", stableTicks))
	ha.commitMetrics[spikeHash.String()] = &CommitAnomalyData{
		FilesChanged: 100,
		LinesAdded:   5000,
		LinesRemoved: 50,
		NetChurn:     4950,
		Files:        []string{"huge_refactor.go"},
	}
	ha.commitsByTick[stableTicks] = []gitlib.Hash{spikeHash}

	report, err := ha.Finalize()
	require.NoError(t, err)

	anomalies, ok := report["anomalies"].([]Record)
	require.True(t, ok)
	require.NotEmpty(t, anomalies, "spike should be detected as anomaly")

	// The spike tick should be in the anomaly list.
	found := false

	for _, a := range anomalies {
		if a.Tick == stableTicks {
			found = true

			assert.Greater(t, a.MaxAbsZScore, 2.0)
			assert.Contains(t, a.Files, "huge_refactor.go")
		}
	}

	assert.True(t, found, "spike tick should be in anomalies")
}

func TestHistoryAnalyzer_Finalize_SortedBySeverity(t *testing.T) {
	t.Parallel()

	ha := newTestAnalyzer()
	ha.WindowSize = 3
	ha.Threshold = 1.0

	ha.commitsByTick = make(map[int][]gitlib.Hash)

	// Build baseline.
	for tick := range 5 {
		hash := gitlib.NewHash(fmt.Sprintf("aa%038d", tick))
		ha.commitMetrics[hash.String()] = &CommitAnomalyData{
			FilesChanged: 5, LinesAdded: 20, LinesRemoved: 10, NetChurn: 10,
		}
		ha.commitsByTick[tick] = []gitlib.Hash{hash}
	}

	// Two spikes of different magnitude.
	h5 := gitlib.NewHash(fmt.Sprintf("bb%038d", 5))
	ha.commitMetrics[h5.String()] = &CommitAnomalyData{
		FilesChanged: 50, LinesAdded: 200, LinesRemoved: 10, NetChurn: 190,
	}
	ha.commitsByTick[5] = []gitlib.Hash{h5}

	h6 := gitlib.NewHash(fmt.Sprintf("bb%038d", 6))
	ha.commitMetrics[h6.String()] = &CommitAnomalyData{
		FilesChanged: 200, LinesAdded: 2000, LinesRemoved: 10, NetChurn: 1990,
	}
	ha.commitsByTick[6] = []gitlib.Hash{h6}

	report, err := ha.Finalize()
	require.NoError(t, err)

	anomalies, ok := report["anomalies"].([]Record)
	require.True(t, ok)

	if len(anomalies) >= 2 {
		assert.GreaterOrEqual(t, anomalies[0].MaxAbsZScore, anomalies[1].MaxAbsZScore,
			"anomalies should be sorted by severity descending")
	}
}

func TestHistoryAnalyzer_Fork_IndependentState(t *testing.T) {
	t.Parallel()

	ha := newTestAnalyzer()
	ha.commitMetrics["aaa"] = &CommitAnomalyData{FilesChanged: 5}

	forks := ha.Fork(2)
	require.Len(t, forks, 2)

	fork1, ok1 := forks[0].(*HistoryAnalyzer)
	require.True(t, ok1)

	fork2, ok2 := forks[1].(*HistoryAnalyzer)
	require.True(t, ok2)

	// Forks should have empty independent maps.
	assert.Empty(t, fork1.commitMetrics, "fork should have empty commitMetrics")
	assert.Empty(t, fork2.commitMetrics, "fork should have empty commitMetrics")

	// Config should be shared.
	assert.InDelta(t, ha.Threshold, fork1.Threshold, 0.001)
	assert.Equal(t, ha.WindowSize, fork1.WindowSize)

	// Plumbing deps should be independent instances.
	assert.NotSame(t, ha.Languages, fork1.Languages)
	assert.NotSame(t, ha.Identity, fork1.Identity)
	assert.NotSame(t, fork1.Languages, fork2.Languages)
	assert.NotSame(t, fork1.Identity, fork2.Identity)

	// Modifying one fork should not affect the other.
	fork1.commitMetrics["bbb"] = &CommitAnomalyData{FilesChanged: 10}
	assert.Empty(t, fork2.commitMetrics)
}

func TestHistoryAnalyzer_Merge_CombinesCommits(t *testing.T) {
	t.Parallel()

	ha := newTestAnalyzer()
	ha.commitMetrics["aaa"] = &CommitAnomalyData{
		FilesChanged: 3, LinesAdded: 10, LinesRemoved: 5,
		Files: []string{"a.go"},
	}

	branch := newTestAnalyzer()
	branch.commitMetrics["bbb"] = &CommitAnomalyData{
		FilesChanged: 2, LinesAdded: 8, LinesRemoved: 4,
		Files: []string{"b.go"},
	}

	ha.Merge([]analyze.HistoryAnalyzer{branch})

	assert.Len(t, ha.commitMetrics, 2)
	assert.Equal(t, 3, ha.commitMetrics["aaa"].FilesChanged)
	assert.Equal(t, 2, ha.commitMetrics["bbb"].FilesChanged)
}

func TestHistoryAnalyzer_Merge_DistinctCommits(t *testing.T) {
	t.Parallel()

	ha := newTestAnalyzer()
	ha.commitMetrics["aaa"] = &CommitAnomalyData{
		FilesChanged: 3, LinesAdded: 10, LinesRemoved: 5,
		Files:     []string{"a.go"},
		Languages: map[string]int{"Go": 2},
		AuthorID:  1,
	}

	branch := newTestAnalyzer()
	branch.commitMetrics["bbb"] = &CommitAnomalyData{
		FilesChanged: 2, LinesAdded: 8, LinesRemoved: 4,
		Files:     []string{"b.go"},
		Languages: map[string]int{"Go": 1, "Python": 1},
		AuthorID:  2,
	}

	ha.Merge([]analyze.HistoryAnalyzer{branch})

	assert.Len(t, ha.commitMetrics, 2)
	assert.Equal(t, 3, ha.commitMetrics["aaa"].FilesChanged)
	assert.Equal(t, 2, ha.commitMetrics["bbb"].FilesChanged)
	assert.Equal(t, 1, ha.commitMetrics["aaa"].AuthorID)
	assert.Equal(t, 2, ha.commitMetrics["bbb"].AuthorID)
}

func TestHistoryAnalyzer_ForkMerge_RoundTrip(t *testing.T) {
	t.Parallel()

	ha := newTestAnalyzer()

	forks := ha.Fork(2)

	fork1, ok1 := forks[0].(*HistoryAnalyzer)
	require.True(t, ok1)

	fork2, ok2 := forks[1].(*HistoryAnalyzer)
	require.True(t, ok2)

	// Each fork processes distinct commits.
	fork1.commitMetrics["aaa"] = &CommitAnomalyData{FilesChanged: 3, Files: []string{"a.go"}}
	fork1.commitMetrics["bbb"] = &CommitAnomalyData{FilesChanged: 2, Files: []string{"b.go"}}
	fork2.commitMetrics["ccc"] = &CommitAnomalyData{FilesChanged: 4, Files: []string{"c.go"}}
	fork2.commitMetrics["ddd"] = &CommitAnomalyData{FilesChanged: 1, Files: []string{"d.go"}}

	ha.Merge(forks)

	assert.Len(t, ha.commitMetrics, 4)
	assert.Equal(t, 3, ha.commitMetrics["aaa"].FilesChanged)
	assert.Equal(t, 2, ha.commitMetrics["bbb"].FilesChanged)
	assert.Equal(t, 4, ha.commitMetrics["ccc"].FilesChanged)
	assert.Equal(t, 1, ha.commitMetrics["ddd"].FilesChanged)
}

func TestHistoryAnalyzer_SequentialOnly(t *testing.T) {
	t.Parallel()

	ha := &HistoryAnalyzer{}
	assert.False(t, ha.SequentialOnly())
}

func TestHistoryAnalyzer_CPUHeavy(t *testing.T) {
	t.Parallel()

	ha := &HistoryAnalyzer{}
	assert.False(t, ha.CPUHeavy())
}

func TestHistoryAnalyzer_Snapshot(t *testing.T) {
	t.Parallel()

	ha := newTestAnalyzer()

	hash1 := gitlib.Hash{0x01}

	ha.TreeDiff.Changes = gitlib.Changes{
		{Action: gitlib.Modify, To: gitlib.ChangeEntry{Name: "main.go", Hash: hash1}},
	}
	ha.Ticks.Tick = 5
	ha.LineStats.LineStats = map[gitlib.ChangeEntry]pkgplumbing.LineStats{
		{Name: "main.go"}: {Added: 10, Removed: 3},
	}
	ha.Languages.SetLanguagesForTest(map[gitlib.Hash]string{
		hash1: "Go",
	})
	ha.Identity.AuthorID = 7

	snap := ha.SnapshotPlumbing()
	require.NotNil(t, snap)

	// Reset analyzer state.
	ha.TreeDiff.Changes = nil
	ha.Ticks.Tick = 0
	ha.LineStats.LineStats = nil
	ha.Languages.SetLanguagesForTest(nil)
	ha.Identity.AuthorID = 0

	// Restore from snapshot.
	ha.ApplySnapshot(snap)

	assert.Len(t, ha.TreeDiff.Changes, 1)
	assert.Equal(t, 5, ha.Ticks.Tick)
	assert.Len(t, ha.LineStats.LineStats, 1)
	assert.Equal(t, "Go", ha.Languages.Languages()[hash1])
	assert.Equal(t, 7, ha.Identity.AuthorID)

	// Release should not panic.
	ha.ReleaseSnapshot(snap)
}

func TestHistoryAnalyzer_Snapshot_WrongType(t *testing.T) {
	t.Parallel()

	ha := newTestAnalyzer()

	// Should not panic with wrong type.
	ha.ApplySnapshot("wrong type")
	ha.ReleaseSnapshot("wrong type")
}

func TestHistoryAnalyzer_Hibernate_Boot(t *testing.T) {
	t.Parallel()

	ha := newTestAnalyzer()
	ha.commitMetrics["aaa"] = &CommitAnomalyData{FilesChanged: 5}

	err := ha.Hibernate()
	require.NoError(t, err)

	// State should be preserved.
	assert.Len(t, ha.commitMetrics, 1)

	err = ha.Boot()
	require.NoError(t, err)
	assert.NotNil(t, ha.commitMetrics)
}

func TestHistoryAnalyzer_Boot_NilMap(t *testing.T) {
	t.Parallel()

	ha := &HistoryAnalyzer{}

	err := ha.Boot()
	require.NoError(t, err)
	assert.NotNil(t, ha.commitMetrics)
}

func TestHistoryAnalyzer_StateGrowthPerCommit(t *testing.T) {
	t.Parallel()

	ha := &HistoryAnalyzer{}
	assert.Positive(t, ha.StateGrowthPerCommit())
}

func TestHistoryAnalyzer_Serialize_JSON(t *testing.T) {
	t.Parallel()

	ha := &HistoryAnalyzer{}

	report := buildTestReport()

	var buf bytes.Buffer

	err := ha.Serialize(report, analyze.FormatJSON, &buf)
	require.NoError(t, err)

	var result map[string]any

	err = json.Unmarshal(buf.Bytes(), &result)
	require.NoError(t, err)

	assert.Contains(t, result, "anomalies")
	assert.Contains(t, result, "time_series")
	assert.Contains(t, result, "aggregate")
}

func TestHistoryAnalyzer_Serialize_YAML(t *testing.T) {
	t.Parallel()

	ha := &HistoryAnalyzer{}

	report := buildTestReport()

	var buf bytes.Buffer

	err := ha.Serialize(report, analyze.FormatYAML, &buf)
	require.NoError(t, err)

	output := buf.String()
	assert.Contains(t, output, "anomalies:")
	assert.Contains(t, output, "time_series:")
	assert.Contains(t, output, "aggregate:")
}

func TestHistoryAnalyzer_Serialize_Binary(t *testing.T) {
	t.Parallel()

	ha := &HistoryAnalyzer{}

	report := buildTestReport()

	var buf bytes.Buffer

	err := ha.Serialize(report, analyze.FormatBinary, &buf)
	require.NoError(t, err)

	// Binary format starts with magic bytes "CFB1".
	assert.Greater(t, buf.Len(), 8)
	assert.Equal(t, "CFB1", buf.String()[:4])
}

func TestHistoryAnalyzer_Serialize_Plot(t *testing.T) {
	t.Parallel()

	ha := &HistoryAnalyzer{}

	report := buildTestReport()

	var buf bytes.Buffer

	err := ha.Serialize(report, analyze.FormatPlot, &buf)
	require.NoError(t, err)

	output := buf.String()
	assert.Contains(t, output, "<!doctype html>")
	assert.Contains(t, output, "Temporal Anomaly Detection")
}

func TestHistoryAnalyzer_Serialize_Unknown(t *testing.T) {
	t.Parallel()

	ha := &HistoryAnalyzer{}

	report := buildTestReport()

	var buf bytes.Buffer

	err := ha.Serialize(report, "unknown", &buf)
	require.ErrorIs(t, err, analyze.ErrUnsupportedFormat)
}

func TestHistoryAnalyzer_ListConfigurationOptions(t *testing.T) {
	t.Parallel()

	ha := &HistoryAnalyzer{}
	opts := ha.ListConfigurationOptions()

	assert.Len(t, opts, 2)
	assert.Equal(t, ConfigAnomalyThreshold, opts[0].Name)
	assert.Equal(t, ConfigAnomalyWindowSize, opts[1].Name)
}

func TestHistoryAnalyzer_FormatReport(t *testing.T) {
	t.Parallel()

	ha := &HistoryAnalyzer{}

	report := buildTestReport()

	var buf bytes.Buffer

	err := ha.FormatReport(report, &buf)
	require.NoError(t, err)
	assert.Contains(t, buf.String(), "anomalies:")
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

	// Verify new fields participate in MaxAbs.
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

// --- Helpers ---.

func newTestAnalyzer() *HistoryAnalyzer {
	treeDiff := &plumbing.TreeDiffAnalyzer{}
	blobCache := &plumbing.BlobCacheAnalyzer{TreeDiff: treeDiff}

	ha := &HistoryAnalyzer{
		TreeDiff:  treeDiff,
		Ticks:     &plumbing.TicksSinceStart{},
		LineStats: &plumbing.LinesStatsCalculator{},
		Languages: &plumbing.LanguagesDetectionAnalyzer{TreeDiff: treeDiff, BlobCache: blobCache},
		Identity:  &plumbing.IdentityDetector{},
	}

	//nolint:errcheck // test helper; Initialize never errors.
	ha.Initialize(nil)

	return ha
}

func buildTestReport() analyze.Report {
	tickMetrics := map[int]*TickMetrics{
		0: {
			FilesChanged: 5, LinesAdded: 20, LinesRemoved: 10, NetChurn: 10,
			Files: []string{"main.go"}, Languages: map[string]int{"Go": 3, "Python": 2}, AuthorIDs: map[int]struct{}{0: {}},
		},
		1: {
			FilesChanged: 3, LinesAdded: 15, LinesRemoved: 8, NetChurn: 7,
			Files: []string{"util.go"}, Languages: map[string]int{"Go": 3}, AuthorIDs: map[int]struct{}{1: {}},
		},
		2: {
			FilesChanged: 4, LinesAdded: 18, LinesRemoved: 9, NetChurn: 9,
			Files: []string{"lib.go"}, Languages: map[string]int{"Go": 2, "Rust": 2}, AuthorIDs: map[int]struct{}{0: {}, 1: {}},
		},
	}

	return analyze.Report{
		"anomalies":       []Record{},
		"tick_metrics":    tickMetrics,
		"commits_by_tick": map[int][]gitlib.Hash{},
		"threshold":       float32(2.0),
		"window_size":     20,
	}
}

func TestHistoryAnalyzer_Consume_StoresPerCommitMetrics(t *testing.T) {
	t.Parallel()

	ha := newTestAnalyzer()

	ha.TreeDiff.Changes = gitlib.Changes{
		{Action: gitlib.Modify, To: gitlib.ChangeEntry{Name: "main.go"}},
		{Action: gitlib.Insert, To: gitlib.ChangeEntry{Name: "util.go"}},
	}
	ha.Ticks.Tick = 0
	ha.LineStats.LineStats = map[gitlib.ChangeEntry]pkgplumbing.LineStats{
		{Name: "main.go"}: {Added: 10, Removed: 3},
		{Name: "util.go"}: {Added: 50, Removed: 0},
	}

	commitHash := gitlib.NewHash("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
	commit := gitlib.NewTestCommit(commitHash, gitlib.TestSignature("dev", "dev@test.com"), "test")

	err := ha.Consume(context.Background(), &analyze.Context{Commit: commit})
	require.NoError(t, err)

	require.Len(t, ha.commitMetrics, 1)

	cm, ok := ha.commitMetrics[commitHash.String()]
	require.True(t, ok)
	assert.Equal(t, 2, cm.FilesChanged)
	assert.Equal(t, 60, cm.LinesAdded)
	assert.Equal(t, 3, cm.LinesRemoved)
	assert.Equal(t, 57, cm.NetChurn)
}

func TestHistoryAnalyzer_Finalize_IncludesCommitMetrics(t *testing.T) {
	t.Parallel()

	ha := newTestAnalyzer()
	hashStr := "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
	ha.commitMetrics[hashStr] = &CommitAnomalyData{
		FilesChanged: 5,
		LinesAdded:   100,
		LinesRemoved: 20,
		NetChurn:     80,
	}

	report, err := ha.Finalize()
	require.NoError(t, err)

	cm, ok := report["commit_metrics"].(map[string]*CommitAnomalyData)
	require.True(t, ok, "report must contain commit_metrics")
	assert.Len(t, cm, 1)
	assert.Contains(t, cm, hashStr)
}

func TestRegisterTickExtractor_Anomaly(t *testing.T) { //nolint:paralleltest // writes to global map
	report := analyze.Report{
		"commit_metrics": map[string]*CommitAnomalyData{
			"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa": {
				FilesChanged: 10, LinesAdded: 100, LinesRemoved: 20, NetChurn: 80,
			},
		},
	}

	result := extractCommitTimeSeries(report)
	require.Len(t, result, 1)

	statsA, ok := result["aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"]
	require.True(t, ok)

	statsMap, ok := statsA.(*CommitAnomalyData)
	require.True(t, ok)
	assert.Equal(t, 10, statsMap.FilesChanged)
	assert.Equal(t, 80, statsMap.NetChurn)
}

func TestAggregateCommitsToTicks_Basic(t *testing.T) {
	t.Parallel()

	h1 := gitlib.NewHash("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
	h2 := gitlib.NewHash("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")

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

	h1 := gitlib.NewHash("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
	h2 := gitlib.NewHash("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")

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

	h1 := gitlib.NewHash("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
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

	h1 := gitlib.NewHash("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
	h2 := gitlib.NewHash("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")
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

func buildTestReportWithSpike() analyze.Report {
	commitMetrics := make(map[string]*CommitAnomalyData)
	commitsByTick := make(map[int][]gitlib.Hash)

	// 10 stable ticks.
	stableTicks := 10
	for tick := range stableTicks {
		hash := gitlib.NewHash(fmt.Sprintf("aa%038d", tick))
		commitMetrics[hash.String()] = &CommitAnomalyData{
			FilesChanged: 5, LinesAdded: 20, LinesRemoved: 10, NetChurn: 10,
			Languages: map[string]int{"Go": 5}, AuthorID: 0,
		}
		commitsByTick[tick] = []gitlib.Hash{hash}
	}

	// Spike.
	spikeHash := gitlib.NewHash(fmt.Sprintf("bb%038d", stableTicks))
	commitMetrics[spikeHash.String()] = &CommitAnomalyData{
		FilesChanged: 200, LinesAdded: 5000, LinesRemoved: 50, NetChurn: 4950,
		Files:     []string{"huge.go"},
		Languages: map[string]int{"Go": 50, "Python": 30, "Shell": 20, "YAML": 100},
		AuthorID:  0,
	}
	commitsByTick[stableTicks] = []gitlib.Hash{spikeHash}

	// Pre-compute anomalies for the report.
	ha := &HistoryAnalyzer{
		Threshold:     DefaultAnomalyThreshold,
		WindowSize:    5,
		commitMetrics: commitMetrics,
		commitsByTick: commitsByTick,
	}

	report, _ := ha.Finalize() //nolint:errcheck // test helper.

	return report
}
