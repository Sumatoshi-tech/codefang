package filehistory

import (
	"bytes"
	"encoding/json"
	"testing"
	"time"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
	pkgplumbing "github.com/Sumatoshi-tech/codefang/pkg/plumbing"
)

func TestAnalyzer_Consume(t *testing.T) {
	t.Parallel()

	h := &Analyzer{
		Identity:  &plumbing.IdentityDetector{},
		TreeDiff:  &plumbing.TreeDiffAnalyzer{},
		LineStats: &plumbing.LinesStatsCalculator{},
	}
	require.NoError(t, h.Initialize(nil))

	// 1. Insert.
	hash1 := gitlib.NewHash("1111111111111111111111111111111111111111")
	change1 := &gitlib.Change{
		Action: gitlib.Insert,
		To:     gitlib.ChangeEntry{Name: "test.txt", Hash: hash1},
	}
	h.TreeDiff.Changes = gitlib.Changes{change1}

	h.Identity.AuthorID = 0
	h.LineStats.LineStats = map[gitlib.ChangeEntry]pkgplumbing.LineStats{
		change1.To: {Added: 10, Removed: 0, Changed: 0},
	}

	commit1 := gitlib.NewTestCommit(
		gitlib.NewHash("c100000000000000000000000000000000000001"),
		gitlib.Signature{When: time.Now()},
		"insert",
	)
	require.NoError(t, h.Consume(&analyze.Context{Commit: commit1}))

	if len(h.files) != 1 {
		t.Errorf("expected 1 file, got %d", len(h.files))
	}

	fh := h.files["test.txt"]
	if len(fh.Hashes) != 1 {
		t.Errorf("expected 1 commit, got %d", len(fh.Hashes))
	}

	if fh.People[0].Added != 10 {
		t.Errorf("expected 10 added lines for author 0, got %d", fh.People[0].Added)
	}

	// 2. Modify.
	hash2 := gitlib.NewHash("2222222222222222222222222222222222222222")
	change2 := &gitlib.Change{
		Action: gitlib.Modify,
		From:   gitlib.ChangeEntry{Name: "test.txt", Hash: hash1},
		To:     gitlib.ChangeEntry{Name: "test.txt", Hash: hash2},
	}
	h.TreeDiff.Changes = gitlib.Changes{change2}
	h.Identity.AuthorID = 1
	h.LineStats.LineStats = map[gitlib.ChangeEntry]pkgplumbing.LineStats{
		change2.To: {Added: 5, Removed: 2, Changed: 3},
	}

	commit2 := gitlib.NewTestCommit(
		gitlib.NewHash("c200000000000000000000000000000000000002"),
		gitlib.Signature{When: time.Now()},
		"modify",
	)
	require.NoError(t, h.Consume(&analyze.Context{Commit: commit2}))

	if len(fh.Hashes) != 2 {
		t.Errorf("expected 2 commits, got %d", len(fh.Hashes))
	}

	if fh.People[1].Added != 5 {
		t.Errorf("expected 5 added lines for author 1, got %d", fh.People[1].Added)
	}

	// 3. Rename.
	change3 := &gitlib.Change{
		Action: gitlib.Modify,
		From:   gitlib.ChangeEntry{Name: "test.txt", Hash: hash2},
		To:     gitlib.ChangeEntry{Name: "renamed.txt", Hash: hash2},
	}
	h.TreeDiff.Changes = gitlib.Changes{change3}
	h.Identity.AuthorID = 0
	h.LineStats.LineStats = map[gitlib.ChangeEntry]pkgplumbing.LineStats{
		change3.To: {Added: 0, Removed: 0, Changed: 0},
	}

	commit3 := gitlib.NewTestCommit(
		gitlib.NewHash("c300000000000000000000000000000000000003"),
		gitlib.Signature{When: time.Now()},
		"rename",
	)
	require.NoError(t, h.Consume(&analyze.Context{Commit: commit3}))

	if _, ok := h.files["test.txt"]; ok {
		t.Error("test.txt should be gone")
	}

	if _, ok := h.files["renamed.txt"]; !ok {
		t.Error("renamed.txt should exist")
	}

	fh = h.files["renamed.txt"]
	if len(fh.Hashes) != 3 {
		t.Errorf("expected 3 commits, got %d", len(fh.Hashes))
	}

	// 4. Delete.
	change4 := &gitlib.Change{
		Action: gitlib.Delete,
		From:   gitlib.ChangeEntry{Name: "renamed.txt", Hash: hash2},
	}
	h.TreeDiff.Changes = gitlib.Changes{change4}
	h.Identity.AuthorID = 0
	h.LineStats.LineStats = map[gitlib.ChangeEntry]pkgplumbing.LineStats{
		change4.From: {Added: 0, Removed: 13, Changed: 0},
	}

	commit4 := gitlib.NewTestCommit(
		gitlib.NewHash("c400000000000000000000000000000000000004"),
		gitlib.Signature{When: time.Now()},
		"delete",
	)
	require.NoError(t, h.Consume(&analyze.Context{Commit: commit4}))

	if _, ok := h.files["renamed.txt"]; !ok {
		t.Error("renamed.txt should still exist in history")
	}

	if len(fh.Hashes) != 4 {
		t.Errorf("expected 4 commits, got %d", len(fh.Hashes))
	}
}

func TestAnalyzer_Merge(t *testing.T) {
	t.Parallel()

	h := &Analyzer{
		Identity:  &plumbing.IdentityDetector{},
		TreeDiff:  &plumbing.TreeDiffAnalyzer{},
		LineStats: &plumbing.LinesStatsCalculator{},
	}
	require.NoError(t, h.Initialize(nil))

	// Simulate merge commit.
	commit := gitlib.NewTestCommit(
		gitlib.NewHash("m100000000000000000000000000000000000001"),
		gitlib.Signature{When: time.Now()},
		"merge",
		gitlib.NewHash("p100000000000000000000000000000000000001"),
		gitlib.NewHash("p200000000000000000000000000000000000002"),
	)

	// First call should consume.
	err := h.Consume(&analyze.Context{Commit: commit})
	if err != nil {
		t.Fatalf("Consume failed: %v", err)
	}

	if !h.merges[commit.Hash()] {
		t.Error("expected merge to be recorded")
	}

	// Second call for same commit should not process again.
	err = h.Consume(&analyze.Context{Commit: commit})
	if err != nil {
		t.Fatalf("Consume 2 failed: %v", err)
	}
}

func TestAnalyzer_Serialize(t *testing.T) {
	t.Parallel()

	h := &Analyzer{}
	require.NoError(t, h.Initialize(nil))

	// Manually construct report.
	report := analyze.Report{
		"Files": map[string]FileHistory{
			"test.txt": {
				Hashes: []gitlib.Hash{gitlib.NewHash("c100000000000000000000000000000000000001")},
				People: map[int]pkgplumbing.LineStats{
					0: {Added: 10, Removed: 0, Changed: 5},
				},
			},
		},
	}

	// YAML - now uses computed metrics.
	var buf bytes.Buffer

	err := h.Serialize(report, analyze.FormatYAML, &buf)
	if err != nil {
		t.Fatalf("Serialize YAML failed: %v", err)
	}

	// Should contain metrics structure keys.
	assert.Contains(t, buf.String(), "file_churn:")
	assert.Contains(t, buf.String(), "aggregate:")

	// Default format falls back to YAML.
	var defaultBuf bytes.Buffer

	err = h.Serialize(report, analyze.FormatBinary, &defaultBuf)
	if err != nil {
		t.Fatalf("Serialize default failed: %v", err)
	}

	if defaultBuf.Len() == 0 {
		t.Error("expected output for default format")
	}
}

func TestAnalyzer_Serialize_JSON_UsesComputedMetrics(t *testing.T) {
	t.Parallel()

	h := &Analyzer{}
	require.NoError(t, h.Initialize(nil))

	report := analyze.Report{
		"Files": map[string]FileHistory{
			"test.go": {
				Hashes: []gitlib.Hash{
					gitlib.NewHash("c100000000000000000000000000000000000001"),
					gitlib.NewHash("c200000000000000000000000000000000000002"),
				},
				People: map[int]pkgplumbing.LineStats{
					0: {Added: 100, Removed: 10, Changed: 20},
				},
			},
		},
	}

	var buf bytes.Buffer

	err := h.Serialize(report, analyze.FormatJSON, &buf)
	require.NoError(t, err)

	var result map[string]any

	err = json.Unmarshal(buf.Bytes(), &result)
	require.NoError(t, err)

	// Should have computed metrics structure.
	assert.Contains(t, result, "file_churn")
	assert.Contains(t, result, "file_contributors")
	assert.Contains(t, result, "hotspots")
	assert.Contains(t, result, "aggregate")
}

func TestAnalyzer_Serialize_YAML_UsesComputedMetrics(t *testing.T) {
	t.Parallel()

	h := &Analyzer{}
	require.NoError(t, h.Initialize(nil))

	report := analyze.Report{
		"Files": map[string]FileHistory{
			"test.go": {
				Hashes: []gitlib.Hash{
					gitlib.NewHash("c100000000000000000000000000000000000001"),
					gitlib.NewHash("c200000000000000000000000000000000000002"),
				},
				People: map[int]pkgplumbing.LineStats{
					0: {Added: 100, Removed: 10, Changed: 20},
				},
			},
		},
	}

	var buf bytes.Buffer

	err := h.Serialize(report, analyze.FormatYAML, &buf)
	require.NoError(t, err)

	output := buf.String()
	// Should have computed metrics structure (YAML keys).
	assert.Contains(t, output, "file_churn:")
	assert.Contains(t, output, "file_contributors:")
	assert.Contains(t, output, "hotspots:")
	assert.Contains(t, output, "aggregate:")
}

func TestAnalyzer_Misc(t *testing.T) {
	t.Parallel()

	h := &Analyzer{}
	if h.Name() == "" {
		t.Error("Name empty")
	}

	if h.Flag() == "" {
		t.Error("Flag empty")
	}

	if h.Description() == "" {
		t.Error("Description empty")
	}

	if len(h.ListConfigurationOptions()) != 0 {
		t.Error("expected 0 options")
	}

	if h.Configure(nil) != nil {
		t.Error("Configure failed")
	}

	// Fork.
	require.NoError(t, h.Initialize(nil))
	h.files["f"] = &FileHistory{}

	clones := h.Fork(2)
	if len(clones) != 2 {
		t.Error("expected 2 clones")
	}

	c1, ok := clones[0].(*Analyzer)
	require.True(t, ok, "type assertion failed for c1")

	// After fix: clones should have empty files (independent state).
	if len(c1.files) != 0 {
		t.Error("expected 0 files in clone (independent copy)")
	}
}

func TestFork_CreatesIndependentCopies(t *testing.T) {
	t.Parallel()

	h := &Analyzer{}
	require.NoError(t, h.Initialize(nil))

	clones := h.Fork(2)

	c1, ok := clones[0].(*Analyzer)
	require.True(t, ok, "type assertion failed for c1")

	c2, ok := clones[1].(*Analyzer)
	require.True(t, ok, "type assertion failed for c2")

	// Modify c1's state.
	c1.files["test.go"] = &FileHistory{
		People: map[int]pkgplumbing.LineStats{0: {Added: 10}},
	}

	// c2 should not be affected.
	require.Empty(t, c2.files, "clones should have independent state")
}

func TestMerge_CombinesFiles(t *testing.T) {
	t.Parallel()

	main := &Analyzer{}
	require.NoError(t, main.Initialize(nil))
	main.files["a.go"] = &FileHistory{
		People: map[int]pkgplumbing.LineStats{0: {Added: 5}},
		Hashes: []gitlib.Hash{gitlib.NewHash("abc123")},
	}

	branch := &Analyzer{}
	require.NoError(t, branch.Initialize(nil))
	branch.files["b.go"] = &FileHistory{
		People: map[int]pkgplumbing.LineStats{1: {Added: 10}},
		Hashes: []gitlib.Hash{gitlib.NewHash("def456")},
	}

	main.Merge([]analyze.HistoryAnalyzer{branch})

	// Main should have both files.
	require.Len(t, main.files, 2)
	require.NotNil(t, main.files["a.go"])
	require.NotNil(t, main.files["b.go"])
}

func TestMerge_CombinesPeopleStats(t *testing.T) {
	t.Parallel()

	main := &Analyzer{}
	require.NoError(t, main.Initialize(nil))
	main.files["test.go"] = &FileHistory{
		People: map[int]pkgplumbing.LineStats{0: {Added: 5, Removed: 2}},
	}

	branch := &Analyzer{}
	require.NoError(t, branch.Initialize(nil))
	branch.files["test.go"] = &FileHistory{
		People: map[int]pkgplumbing.LineStats{0: {Added: 3, Removed: 1}},
	}

	main.Merge([]analyze.HistoryAnalyzer{branch})

	// Stats should be summed.
	stats := main.files["test.go"].People[0]
	require.Equal(t, 8, stats.Added)
	require.Equal(t, 3, stats.Removed)
}

func TestMerge_CombinesMerges(t *testing.T) {
	t.Parallel()

	main := &Analyzer{}
	require.NoError(t, main.Initialize(nil))
	main.merges[gitlib.NewHash("abc123")] = true

	branch := &Analyzer{}
	require.NoError(t, branch.Initialize(nil))
	branch.merges[gitlib.NewHash("def456")] = true

	main.Merge([]analyze.HistoryAnalyzer{branch})

	// Both merges should be present.
	require.Len(t, main.merges, 2)
	require.True(t, main.merges[gitlib.NewHash("abc123")])
	require.True(t, main.merges[gitlib.NewHash("def456")])
}
