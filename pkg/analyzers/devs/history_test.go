package devs //nolint:testpackage // testing internal implementation.

import (
	"bytes"
	"strings"
	"testing"

	"time"

	"github.com/stretchr/testify/require"

	gitplumbing "github.com/go-git/go-git/v6/plumbing"
	"github.com/go-git/go-git/v6/plumbing/object"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/identity"
	pkgplumbing "github.com/Sumatoshi-tech/codefang/pkg/plumbing"
)

func TestDevsHistoryAnalyzer_Configure(t *testing.T) {
	t.Parallel()

	d := &DevsHistoryAnalyzer{}
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

func TestDevsHistoryAnalyzer_Initialize(t *testing.T) {
	t.Parallel()

	d := &DevsHistoryAnalyzer{}

	err := d.Initialize(nil)
	if err != nil {
		t.Fatalf("Initialize failed: %v", err)
	}

	if d.tickSize != 24*time.Hour {
		t.Errorf("expected default tickSize 24h, got %v", d.tickSize)
	}

	if d.ticks == nil {
		t.Error("expected ticks initialized")
	}

	if d.merges == nil {
		t.Error("expected merges initialized")
	}
}

func TestDevsHistoryAnalyzer_Consume(t *testing.T) {
	t.Parallel()

	d := &DevsHistoryAnalyzer{
		Identity:  &plumbing.IdentityDetector{},
		TreeDiff:  &plumbing.TreeDiffAnalyzer{},
		Ticks:     &plumbing.TicksSinceStart{},
		Languages: &plumbing.LanguagesDetectionAnalyzer{},
		LineStats: &plumbing.LinesStatsCalculator{},
	}
	require.NoError(t, d.Initialize(nil))

	// 1. Standard commit.
	hash1 := gitplumbing.NewHash("1111111111111111111111111111111111111111")
	change1 := &object.Change{
		To: object.ChangeEntry{Name: "test.go", TreeEntry: object.TreeEntry{Hash: hash1}},
	}
	d.TreeDiff.Changes = object.Changes{change1}
	d.Ticks.Tick = 0
	d.Identity.AuthorID = 0
	d.Languages.Languages = map[gitplumbing.Hash]string{hash1: "Go"}
	d.LineStats.LineStats = map[object.ChangeEntry]pkgplumbing.LineStats{
		change1.To: {Added: 10, Removed: 0, Changed: 0},
	}

	commit1 := &object.Commit{Hash: gitplumbing.NewHash("c1")}
	require.NoError(t, d.Consume(&analyze.Context{Commit: commit1}))

	tick := d.ticks[0]
	if tick == nil {
		t.Fatal("expected tick 0")
	}

	dev := tick[0]
	if dev == nil {
		t.Fatal("expected dev 0")
	}

	if dev.Commits != 1 {
		t.Errorf("expected 1 commit, got %d", dev.Commits)
	}

	if dev.Added != 10 {
		t.Errorf("expected 10 added lines, got %d", dev.Added)
	}

	if dev.Languages["Go"].Added != 10 {
		t.Errorf("expected 10 added Go lines, got %d", dev.Languages["Go"].Added)
	}

	// 2. Empty commit (ignored by default).
	d.TreeDiff.Changes = object.Changes{}
	require.NoError(t, d.Consume(&analyze.Context{Commit: &object.Commit{Hash: gitplumbing.NewHash("c2")}}))

	if dev.Commits != 1 {
		t.Errorf("expected still 1 commit, got %d", dev.Commits)
	}

	// 3. Empty commit (considered).
	d.ConsiderEmptyCommits = true
	require.NoError(t, d.Consume(&analyze.Context{Commit: &object.Commit{Hash: gitplumbing.NewHash("c3")}}))

	if dev.Commits != 2 {
		t.Errorf("expected 2 commits, got %d", dev.Commits)
	}

	// 4. Merge commit (processed once).
	commitMerge := &object.Commit{
		Hash:         gitplumbing.NewHash("m1"),
		ParentHashes: []gitplumbing.Hash{gitplumbing.NewHash("p1"), gitplumbing.NewHash("p2")},
	}
	require.NoError(t, d.Consume(&analyze.Context{Commit: commitMerge}))

	if !d.merges[commitMerge.Hash] {
		t.Error("expected merge marked")
	}

	if dev.Commits != 3 {
		t.Errorf("expected 3 commits (merge counted), got %d", dev.Commits)
	}

	// 5. Merge commit (ctx.IsMerge=true) -> ignored due to OneShotMergeProcessor (already seen).
	require.NoError(t, d.Consume(&analyze.Context{Commit: commitMerge, IsMerge: true}))

	if dev.Commits != 3 {
		t.Errorf("expected still 3 commits (merge duplicate ignored), got %d", dev.Commits)
	}
}

func TestDevsHistoryAnalyzer_Finalize(t *testing.T) {
	t.Parallel()

	d := &DevsHistoryAnalyzer{
		reversedPeopleDict: []string{"dev1"},
		tickSize:           24 * time.Hour,
	}
	require.NoError(t, d.Initialize(nil))
	d.ticks[0] = map[int]*DevTick{
		0: {Commits: 1},
	}

	report, err := d.Finalize()
	if err != nil {
		t.Fatalf("Finalize failed: %v", err)
	}

	ticks, ok := report["Ticks"].(map[int]map[int]*DevTick)
	require.True(t, ok, "type assertion failed for ticks")

	if ticks[0][0].Commits != 1 {
		t.Error("expected tick data")
	}
}

func TestDevsHistoryAnalyzer_Serialize(t *testing.T) {
	t.Parallel()

	d := &DevsHistoryAnalyzer{}

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

	// JSON.
	var buf bytes.Buffer

	err := d.Serialize(report, false, &buf)
	if err != nil {
		t.Fatalf("Serialize JSON failed: %v", err)
	}

	if !strings.Contains(buf.String(), "ticks:") {
		t.Error("expected ticks in output")
	}

	if !strings.Contains(buf.String(), "Go: [10, 0, 0]") {
		t.Error("expected language stats in output")
	}

	// Binary.
	var pbuf bytes.Buffer

	err = d.Serialize(report, true, &pbuf)
	if err != nil {
		t.Fatalf("Serialize Binary failed: %v", err)
	}

	if pbuf.Len() == 0 {
		t.Error("expected binary output")
	}
}

func TestDevsHistoryAnalyzer_Misc(t *testing.T) {
	t.Parallel()

	d := &DevsHistoryAnalyzer{}
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
