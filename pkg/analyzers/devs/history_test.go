package devs

import (
	"bytes"
	"context"
	"encoding/json"
	"strings"
	"testing"
	"time"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
	"github.com/Sumatoshi-tech/codefang/pkg/identity"
	pkgplumbing "github.com/Sumatoshi-tech/codefang/pkg/plumbing"
)

func TestHistoryAnalyzer_Configure(t *testing.T) {
	t.Parallel()

	d := &HistoryAnalyzer{}
	facts := map[string]any{
		ConfigDevsConsiderEmptyCommits:                  true,
		identity.FactIdentityDetectorReversedPeopleDict: []string{"dev1"},
		pkgplumbing.FactTickSize:                        12 * time.Hour,
	}

	err := d.Configure(facts)
	if err != nil {
		t.Fatalf("Configure failed: %v", err)
	}

	if !d.ConsiderEmptyCommits {
		t.Error("expected ConsiderEmptyCommits true")
	}

	if len(d.reversedPeopleDict) != 1 {
		t.Errorf("expected reversedPeopleDict len 1, got %d", len(d.reversedPeopleDict))
	}

	if d.tickSize != 12*time.Hour {
		t.Errorf("expected tickSize 12h, got %v", d.tickSize)
	}
}

func TestHistoryAnalyzer_Initialize(t *testing.T) {
	t.Parallel()

	d := &HistoryAnalyzer{}

	err := d.Initialize(nil)
	if err != nil {
		t.Fatalf("Initialize failed: %v", err)
	}

	if d.tickSize != 24*time.Hour {
		t.Errorf("expected default tickSize 24h, got %v", d.tickSize)
	}

	if d.commitDevData == nil {
		t.Error("expected commitDevData initialized")
	}

	if d.merges == nil {
		t.Error("expected merges initialized")
	}
}

func TestHistoryAnalyzer_Consume(t *testing.T) {
	t.Parallel()

	d := &HistoryAnalyzer{
		Identity:  &plumbing.IdentityDetector{},
		TreeDiff:  &plumbing.TreeDiffAnalyzer{},
		Ticks:     &plumbing.TicksSinceStart{},
		Languages: &plumbing.LanguagesDetectionAnalyzer{},
		LineStats: &plumbing.LinesStatsCalculator{},
	}
	require.NoError(t, d.Initialize(nil))

	// 1. Standard commit.
	hash1 := gitlib.NewHash("1111111111111111111111111111111111111111")
	change1 := &gitlib.Change{
		Action: gitlib.Insert,
		To:     gitlib.ChangeEntry{Name: "test.go", Hash: hash1},
	}
	d.TreeDiff.Changes = gitlib.Changes{change1}
	d.Ticks.Tick = 0
	d.Identity.AuthorID = 0
	d.Languages.SetLanguagesForTest(map[gitlib.Hash]string{hash1: "Go"})
	d.LineStats.LineStats = map[gitlib.ChangeEntry]pkgplumbing.LineStats{
		change1.To: {Added: 10, Removed: 0, Changed: 0},
	}

	commitHash1 := gitlib.NewHash("c100000000000000000000000000000000000001")
	commit1 := gitlib.NewTestCommit(
		commitHash1,
		gitlib.TestSignature("dev", "dev@test.com"),
		"test",
	)
	require.NoError(t, d.Consume(context.Background(), &analyze.Context{Commit: commit1}))

	cdd := d.commitDevData[commitHash1.String()]
	if cdd == nil {
		t.Fatal("expected commit data for commit1")
	}

	if cdd.Commits != 1 {
		t.Errorf("expected 1 commit, got %d", cdd.Commits)
	}

	if cdd.Added != 10 {
		t.Errorf("expected 10 added lines, got %d", cdd.Added)
	}

	if cdd.Languages["Go"].Added != 10 {
		t.Errorf("expected 10 added Go lines, got %d", cdd.Languages["Go"].Added)
	}

	// 2. Empty commit (ignored by default).
	d.TreeDiff.Changes = gitlib.Changes{}
	commitHash2 := gitlib.NewHash("c200000000000000000000000000000000000002")
	commit2 := gitlib.NewTestCommit(
		commitHash2,
		gitlib.TestSignature("dev", "dev@test.com"),
		"empty",
	)
	require.NoError(t, d.Consume(context.Background(), &analyze.Context{Commit: commit2}))

	if len(d.commitDevData) != 1 {
		t.Errorf("expected still 1 commit entry, got %d", len(d.commitDevData))
	}

	// 3. Empty commit (considered).
	d.ConsiderEmptyCommits = true
	commitHash3 := gitlib.NewHash("c300000000000000000000000000000000000003")
	commit3 := gitlib.NewTestCommit(
		commitHash3,
		gitlib.TestSignature("dev", "dev@test.com"),
		"empty considered",
	)
	require.NoError(t, d.Consume(context.Background(), &analyze.Context{Commit: commit3}))

	if len(d.commitDevData) != 2 {
		t.Errorf("expected 2 commit entries, got %d", len(d.commitDevData))
	}

	// 4. Merge commit (processed once).
	commitMerge := gitlib.NewTestCommit(
		gitlib.NewHash("m100000000000000000000000000000000000001"),
		gitlib.TestSignature("dev", "dev@test.com"),
		"merge",
		gitlib.NewHash("p100000000000000000000000000000000000001"),
		gitlib.NewHash("p200000000000000000000000000000000000002"),
	)
	require.NoError(t, d.Consume(context.Background(), &analyze.Context{Commit: commitMerge}))

	if !d.merges[commitMerge.Hash()] {
		t.Error("expected merge marked")
	}

	if len(d.commitDevData) != 3 {
		t.Errorf("expected 3 commit entries (merge counted), got %d", len(d.commitDevData))
	}

	// 5. Merge commit (ctx.IsMerge=true) -> ignored due to OneShotMergeProcessor (already seen).
	require.NoError(t, d.Consume(context.Background(), &analyze.Context{Commit: commitMerge, IsMerge: true}))

	if len(d.commitDevData) != 3 {
		t.Errorf("expected still 3 commit entries (merge duplicate ignored), got %d", len(d.commitDevData))
	}
}

func TestHistoryAnalyzer_Consume_StoresPerCommitData(t *testing.T) { //nolint:paralleltest // Initialize writes global map
	d := &HistoryAnalyzer{
		Identity:  &plumbing.IdentityDetector{},
		TreeDiff:  &plumbing.TreeDiffAnalyzer{},
		Ticks:     &plumbing.TicksSinceStart{},
		Languages: &plumbing.LanguagesDetectionAnalyzer{},
		LineStats: &plumbing.LinesStatsCalculator{},
	}
	require.NoError(t, d.Initialize(nil))

	hash1 := gitlib.NewHash("1111111111111111111111111111111111111111")
	change1 := &gitlib.Change{
		Action: gitlib.Insert,
		To:     gitlib.ChangeEntry{Name: "test.go", Hash: hash1},
	}
	d.TreeDiff.Changes = gitlib.Changes{change1}
	d.Ticks.Tick = 0
	d.Identity.AuthorID = 0
	d.Languages.SetLanguagesForTest(map[gitlib.Hash]string{hash1: "Go"})
	d.LineStats.LineStats = map[gitlib.ChangeEntry]pkgplumbing.LineStats{
		change1.To: {Added: 10, Removed: 3, Changed: 2},
	}

	commitHash := gitlib.NewHash("c100000000000000000000000000000000000001")
	commit := gitlib.NewTestCommit(
		commitHash,
		gitlib.TestSignature("dev", "dev@test.com"),
		"test commit",
	)
	require.NoError(t, d.Consume(context.Background(), &analyze.Context{Commit: commit}))

	// Per-commit data should exist.
	require.Len(t, d.commitDevData, 1)

	cdd, ok := d.commitDevData[commitHash.String()]
	require.True(t, ok)
	assert.Equal(t, 1, cdd.Commits)
	assert.Equal(t, 10, cdd.Added)
	assert.Equal(t, 3, cdd.Removed)
	assert.Equal(t, 2, cdd.Changed)
}

func TestHistoryAnalyzer_Finalize_IncludesCommitDevData(t *testing.T) { //nolint:paralleltest // Initialize writes global map
	d := &HistoryAnalyzer{
		reversedPeopleDict: []string{"dev1"},
		tickSize:           24 * time.Hour,
	}
	require.NoError(t, d.Initialize(nil))

	hashStr := "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
	d.commitDevData[hashStr] = &CommitDevData{
		Commits: 1,
		Added:   50,
		Removed: 10,
		Changed: 5,
	}

	report, err := d.Finalize()
	require.NoError(t, err)

	cdd, ok := report["CommitDevData"].(map[string]*CommitDevData)
	require.True(t, ok, "report must contain CommitDevData")
	assert.Len(t, cdd, 1)
	assert.Contains(t, cdd, hashStr)
}

func TestHistoryAnalyzer_Finalize(t *testing.T) {
	t.Parallel()

	d := &HistoryAnalyzer{
		reversedPeopleDict: []string{"dev1"},
		tickSize:           24 * time.Hour,
	}
	require.NoError(t, d.Initialize(nil))

	hashStr := "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
	d.commitDevData[hashStr] = &CommitDevData{Commits: 1, AuthorID: 0}

	report, err := d.Finalize()
	if err != nil {
		t.Fatalf("Finalize failed: %v", err)
	}

	cdd, ok := report["CommitDevData"].(map[string]*CommitDevData)
	require.True(t, ok, "type assertion failed for CommitDevData")

	if cdd[hashStr].Commits != 1 {
		t.Error("expected commit data")
	}

	_, hasTicks := report["Ticks"]
	assert.False(t, hasTicks, "should not emit Ticks key")
}

func TestHistoryAnalyzer_Serialize(t *testing.T) {
	t.Parallel()

	d := &HistoryAnalyzer{}

	ticks := map[int]map[int]*DevTick{
		0: {
			0: {
				Commits:   1,
				LineStats: pkgplumbing.LineStats{Added: 10},
				Languages: map[string]pkgplumbing.LineStats{"Go": {Added: 10}},
			},
			identity.AuthorMissing: {
				Commits: 1,
			},
		},
	}

	report := analyze.Report{
		"Ticks":              ticks,
		"ReversedPeopleDict": []string{"dev0"},
		"TickSize":           24 * time.Hour,
	}

	// YAML (uses computed metrics format).
	var buf bytes.Buffer

	err := d.Serialize(report, analyze.FormatYAML, &buf)
	if err != nil {
		t.Fatalf("Serialize YAML failed: %v", err)
	}

	yamlOut := buf.String()
	if !strings.Contains(yamlOut, "aggregate:") {
		t.Error("expected aggregate in YAML output")
	}

	if !strings.Contains(yamlOut, "developers:") {
		t.Error("expected developers in YAML output")
	}

	if !strings.Contains(yamlOut, "languages:") {
		t.Error("expected languages in YAML output")
	}

	// Binary.
	var pbuf bytes.Buffer

	err = d.Serialize(report, analyze.FormatBinary, &pbuf)
	if err != nil {
		t.Fatalf("Serialize Binary failed: %v", err)
	}

	if pbuf.Len() == 0 {
		t.Error("expected binary output")
	}

	// Plot (HTML).
	var plotBuf bytes.Buffer

	err = d.Serialize(report, analyze.FormatPlot, &plotBuf)
	if err != nil {
		t.Fatalf("Serialize Plot failed: %v", err)
	}

	plotOut := plotBuf.String()
	if !strings.Contains(plotOut, "Developer Analytics Dashboard") {
		t.Error("expected dashboard title in plot output")
	}

	if !strings.Contains(plotOut, "echarts") {
		t.Error("expected echarts in plot output")
	}

	if !strings.Contains(plotOut, "dev0") {
		t.Error("expected developer name in plot output")
	}

	// Plot with empty ticks (empty chart).
	emptyReport := analyze.Report{
		"Ticks":              map[int]map[int]*DevTick{},
		"ReversedPeopleDict": []string{},
		"TickSize":           24 * time.Hour,
	}

	var emptyPlotBuf bytes.Buffer

	err = d.Serialize(emptyReport, analyze.FormatPlot, &emptyPlotBuf)
	if err != nil {
		t.Fatalf("Serialize Plot empty failed: %v", err)
	}

	if !strings.Contains(emptyPlotBuf.String(), "Developer Analytics Dashboard") {
		t.Error("expected dashboard title in empty plot output")
	}
}

func TestHistoryAnalyzer_Misc(t *testing.T) {
	t.Parallel()

	d := &HistoryAnalyzer{}
	if d.Name() == "" {
		t.Error("Name empty")
	}

	if d.Flag() == "" {
		t.Error("Flag empty")
	}

	if d.Description() == "" {
		t.Error("Description empty")
	}

	if len(d.ListConfigurationOptions()) == 0 {
		t.Error("expected options")
	}

	require.NoError(t, d.Initialize(nil))

	clones := d.Fork(2)
	if len(clones) != 2 {
		t.Error("expected 2 clones")
	}
}

func TestHistoryAnalyzer_Serialize_JSON_UsesComputedMetrics(t *testing.T) {
	t.Parallel()

	d := &HistoryAnalyzer{}

	ticks := map[int]map[int]*DevTick{
		0: {
			0: {
				Commits:   5,
				LineStats: pkgplumbing.LineStats{Added: 100, Removed: 30},
				Languages: map[string]pkgplumbing.LineStats{"Go": {Added: 100}},
			},
		},
	}

	report := analyze.Report{
		"Ticks":              ticks,
		"ReversedPeopleDict": []string{"Alice"},
		"TickSize":           24 * time.Hour,
	}

	var buf bytes.Buffer

	err := d.Serialize(report, analyze.FormatJSON, &buf)
	require.NoError(t, err)

	// Parse the JSON output.
	var result map[string]any

	err = json.Unmarshal(buf.Bytes(), &result)
	require.NoError(t, err)

	// Should have computed metrics structure with lowercase keys (from json tags).
	assert.Contains(t, result, "aggregate")
	assert.Contains(t, result, "developers")
	assert.Contains(t, result, "languages")
	assert.Contains(t, result, "busfactor")
	assert.Contains(t, result, "activity")
	assert.Contains(t, result, "churn")
}

func TestHistoryAnalyzer_Serialize_YAML_UsesComputedMetrics(t *testing.T) {
	t.Parallel()

	d := &HistoryAnalyzer{}

	ticks := map[int]map[int]*DevTick{
		0: {
			0: {
				Commits:   5,
				LineStats: pkgplumbing.LineStats{Added: 100, Removed: 30},
				Languages: map[string]pkgplumbing.LineStats{"Go": {Added: 100}},
			},
		},
	}

	report := analyze.Report{
		"Ticks":              ticks,
		"ReversedPeopleDict": []string{"Alice"},
		"TickSize":           24 * time.Hour,
	}

	var buf bytes.Buffer

	err := d.Serialize(report, analyze.FormatYAML, &buf)
	require.NoError(t, err)

	output := buf.String()
	// Should have computed metrics structure (YAML keys).
	assert.Contains(t, output, "aggregate:")
	assert.Contains(t, output, "developers:")
	assert.Contains(t, output, "languages:")
	assert.Contains(t, output, "busfactor:")
	assert.Contains(t, output, "activity:")
	assert.Contains(t, output, "churn:")
}

func TestRegisterTickExtractor_Devs(t *testing.T) { //nolint:paralleltest // writes to global map
	report := analyze.Report{
		"CommitDevData": map[string]*CommitDevData{
			"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa": {
				Commits: 1,
				Added:   100,
				Removed: 20,
				Changed: 5,
			},
			"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb": {
				Commits: 1,
				Added:   50,
				Removed: 10,
				Changed: 3,
			},
		},
	}

	result := extractCommitTimeSeries(report)
	require.Len(t, result, 2)

	statsA, ok := result["aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"]
	require.True(t, ok)

	statsMap, ok := statsA.(map[string]any)
	require.True(t, ok)
	assert.Equal(t, 1, statsMap["commits"])
	assert.Equal(t, 100, statsMap["lines_added"])
	assert.Equal(t, 20, statsMap["lines_removed"])
	assert.Equal(t, 80, statsMap["net_change"])
	assert.Contains(t, statsMap, "author_id")
}

func TestAggregateCommitsToTicks_Basic(t *testing.T) {
	t.Parallel()

	h1 := gitlib.NewHash("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
	h2 := gitlib.NewHash("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")

	commitDevData := map[string]*CommitDevData{
		h1.String(): {
			Commits: 1, Added: 20, Removed: 5, Changed: 3, AuthorID: 1,
			Languages: map[string]pkgplumbing.LineStats{"Go": {Added: 20, Removed: 5, Changed: 3}},
		},
		h2.String(): {
			Commits: 1, Added: 10, Removed: 3, Changed: 2, AuthorID: 2,
			Languages: map[string]pkgplumbing.LineStats{"Python": {Added: 10, Removed: 3, Changed: 2}},
		},
	}
	commitsByTick := map[int][]gitlib.Hash{
		0: {h1, h2},
	}

	result := AggregateCommitsToTicks(commitDevData, commitsByTick)
	require.Len(t, result, 1)
	require.Len(t, result[0], 2) // Two developers.

	dt1 := result[0][1]
	require.NotNil(t, dt1)
	assert.Equal(t, 1, dt1.Commits)
	assert.Equal(t, 20, dt1.Added)
	assert.Equal(t, 5, dt1.Removed)

	dt2 := result[0][2]
	require.NotNil(t, dt2)
	assert.Equal(t, 1, dt2.Commits)
	assert.Equal(t, 10, dt2.Added)
}

func TestAggregateCommitsToTicks_SameAuthorMultipleCommits(t *testing.T) {
	t.Parallel()

	h1 := gitlib.NewHash("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
	h2 := gitlib.NewHash("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")

	commitDevData := map[string]*CommitDevData{
		h1.String(): {Commits: 1, Added: 20, Removed: 5, AuthorID: 1},
		h2.String(): {Commits: 1, Added: 10, Removed: 3, AuthorID: 1},
	}
	commitsByTick := map[int][]gitlib.Hash{
		0: {h1, h2},
	}

	result := AggregateCommitsToTicks(commitDevData, commitsByTick)
	require.Len(t, result, 1)
	require.Len(t, result[0], 1) // Same author.

	dt := result[0][1]
	assert.Equal(t, 2, dt.Commits)
	assert.Equal(t, 30, dt.Added)
	assert.Equal(t, 8, dt.Removed)
}

func TestAggregateCommitsToTicks_EmptyInputs(t *testing.T) {
	t.Parallel()

	assert.Nil(t, AggregateCommitsToTicks(nil, map[int][]gitlib.Hash{0: {}}))
	assert.Nil(t, AggregateCommitsToTicks(map[string]*CommitDevData{"a": {}}, nil))
}

func TestComputeAllMetrics_FromCommitData(t *testing.T) {
	t.Parallel()

	h1 := gitlib.NewHash("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
	h2 := gitlib.NewHash("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")

	report := analyze.Report{
		"CommitDevData": map[string]*CommitDevData{
			h1.String(): {
				Commits: 1, Added: 20, Removed: 5, Changed: 3, AuthorID: 0,
				Languages: map[string]pkgplumbing.LineStats{"Go": {Added: 20, Removed: 5, Changed: 3}},
			},
			h2.String(): {
				Commits: 1, Added: 10, Removed: 3, Changed: 2, AuthorID: 1,
				Languages: map[string]pkgplumbing.LineStats{"Python": {Added: 10, Removed: 3, Changed: 2}},
			},
		},
		"CommitsByTick": map[int][]gitlib.Hash{
			0: {h1},
			1: {h2},
		},
		"ReversedPeopleDict": []string{"Alice", "Bob"},
		"TickSize":           24 * time.Hour,
	}

	computed, err := ComputeAllMetrics(report)
	require.NoError(t, err)
	assert.Len(t, computed.Developers, 2)
	assert.Positive(t, computed.Aggregate.TotalCommits)
	assert.Equal(t, 2, computed.Aggregate.TotalCommits)
}
