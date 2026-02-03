package couples //nolint:testpackage // testing internal implementation.

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
	"github.com/Sumatoshi-tech/codefang/pkg/identity"
)

func TestHistoryAnalyzer_Configure(t *testing.T) {
	t.Parallel()

	c := &HistoryAnalyzer{}
	facts := map[string]any{
		identity.FactIdentityDetectorPeopleCount:        10,
		identity.FactIdentityDetectorReversedPeopleDict: []string{"dev1"},
	}

	err := c.Configure(facts)
	if err != nil {
		t.Fatalf("Configure failed: %v", err)
	}

	if c.PeopleNumber != 10 {
		t.Errorf("expected PeopleNumber 10, got %d", c.PeopleNumber)
	}

	if len(c.reversedPeopleDict) != 1 {
		t.Errorf("expected reversedPeopleDict len 1, got %d", len(c.reversedPeopleDict))
	}
}

func TestHistoryAnalyzer_Initialize(t *testing.T) {
	t.Parallel()

	c := &HistoryAnalyzer{PeopleNumber: 1}

	err := c.Initialize(nil)
	if err != nil {
		t.Fatalf("Initialize failed: %v", err)
	}

	if len(c.people) != 2 {
		t.Errorf("expected people len 2, got %d", len(c.people))
	}

	if len(c.peopleCommits) != 2 {
		t.Errorf("expected peopleCommits len 2, got %d", len(c.peopleCommits))
	}

	if c.renames == nil {
		t.Error("expected renames to be initialized")
	}
}

func TestHistoryAnalyzer_Consume(t *testing.T) {
	t.Parallel()

	c := &HistoryAnalyzer{
		PeopleNumber: 1,
		Identity:     &plumbing.IdentityDetector{},
		TreeDiff:     &plumbing.TreeDiffAnalyzer{},
	}
	require.NoError(t, c.Initialize(nil))

	// 1. Insert two files in same commit (author 0).
	hash1 := gitlib.NewHash("1111111111111111111111111111111111111111")
	change1 := &gitlib.Change{Action: gitlib.Insert, To: gitlib.ChangeEntry{Name: "f1", Hash: hash1}}
	change2 := &gitlib.Change{Action: gitlib.Insert, To: gitlib.ChangeEntry{Name: "f2", Hash: hash1}}

	c.TreeDiff.Changes = gitlib.Changes{change1, change2}
	c.Identity.AuthorID = 0

	commit1 := gitlib.NewTestCommit(
		gitlib.NewHash("c100000000000000000000000000000000000001"),
		gitlib.Signature{When: time.Now()},
		"insert",
	)
	require.NoError(t, c.Consume(&analyze.Context{Commit: commit1}))

	if c.peopleCommits[0] != 1 {
		t.Errorf("expected author 0 commits 1, got %d", c.peopleCommits[0])
	}

	if c.people[0]["f1"] != 1 {
		t.Errorf("expected author 0 f1 count 1, got %d", c.people[0]["f1"])
	}

	if c.files["f1"]["f2"] != 1 {
		t.Errorf("expected f1-f2 coupling 1, got %d", c.files["f1"]["f2"])
	}

	// 2. Modify f1 (author 1).
	change3 := &gitlib.Change{
		Action: gitlib.Modify,
		From:   gitlib.ChangeEntry{Name: "f1", Hash: hash1},
		To:     gitlib.ChangeEntry{Name: "f1", Hash: hash1},
	}
	c.TreeDiff.Changes = gitlib.Changes{change3}
	c.Identity.AuthorID = 1

	commit2 := gitlib.NewTestCommit(
		gitlib.NewHash("c200000000000000000000000000000000000002"),
		gitlib.Signature{When: time.Now()},
		"modify",
	)
	require.NoError(t, c.Consume(&analyze.Context{Commit: commit2}))

	if c.people[1]["f1"] != 1 {
		t.Errorf("expected author 1 f1 count 1, got %d", c.people[1]["f1"])
	}

	// 3. Delete f2 (author 0).
	change4 := &gitlib.Change{
		Action: gitlib.Delete,
		From:   gitlib.ChangeEntry{Name: "f2", Hash: hash1},
	}
	c.TreeDiff.Changes = gitlib.Changes{change4}
	c.Identity.AuthorID = 0

	commit3 := gitlib.NewTestCommit(
		gitlib.NewHash("c300000000000000000000000000000000000003"),
		gitlib.Signature{When: time.Now()},
		"delete",
	)
	require.NoError(t, c.Consume(&analyze.Context{Commit: commit3}))

	if c.people[0]["f2"] != 2 { // 1 insert + 1 delete.
		t.Errorf("expected author 0 f2 count 2, got %d", c.people[0]["f2"])
	}
}

func TestHistoryAnalyzer_Consume_Merge(t *testing.T) {
	t.Parallel()

	c := &HistoryAnalyzer{
		PeopleNumber: 1,
		Identity:     &plumbing.IdentityDetector{},
		TreeDiff:     &plumbing.TreeDiffAnalyzer{},
	}
	require.NoError(t, c.Initialize(nil))

	commit := gitlib.NewTestCommit(
		gitlib.NewHash("m100000000000000000000000000000000000001"),
		gitlib.Signature{When: time.Now()},
		"merge",
		gitlib.NewHash("p100000000000000000000000000000000000001"),
		gitlib.NewHash("p200000000000000000000000000000000000002"),
	)

	// First pass (shouldConsume=true, merges marked).
	require.NoError(t, c.Consume(&analyze.Context{Commit: commit}))

	if !c.merges[commit.Hash()] {
		t.Error("expected merge marked")
	}

	// Test Consume logic with IsMerge=true.
	change := &gitlib.Change{Action: gitlib.Insert, To: gitlib.ChangeEntry{Name: "new_merge.txt"}}
	c.TreeDiff.Changes = gitlib.Changes{change}
	c.Identity.AuthorID = 0

	require.NoError(t, c.Consume(&analyze.Context{Commit: commit, IsMerge: true}))

	if c.people[0]["new_merge.txt"] != 1 {
		t.Errorf("expected new_merge.txt counted in merge, got %d", c.people[0]["new_merge.txt"])
	}
}

func TestHistoryAnalyzer_Finalize(t *testing.T) {
	t.Parallel()

	c := &HistoryAnalyzer{
		PeopleNumber:       1,
		reversedPeopleDict: []string{"dev1"},
	}
	require.NoError(t, c.Initialize(nil))

	// Manually populate state.
	c.people[0] = map[string]int{"f1": 10, "f2": 5}
	c.people[1] = map[string]int{"f1": 5} // Overlap on f1.
	c.files["f1"] = map[string]int{"f2": 3}
	c.files["f2"] = map[string]int{"f1": 3}

	report, err := c.Finalize()
	if err != nil {
		t.Fatalf("Finalize failed: %v", err)
	}

	pm, ok := report["PeopleMatrix"].([]map[int]int64)
	require.True(t, ok, "type assertion failed for pm")

	if pm[0][1] != 5 {
		t.Errorf("expected people matrix 0-1 to be 5, got %d", pm[0][1])
	}

	fm, ok := report["FilesMatrix"].([]map[int]int64)
	require.True(t, ok, "type assertion failed for fm")

	if fm[0][1] != 3 {
		t.Errorf("expected files matrix 0-1 to be 3, got %d", fm[0][1])
	}
}

func TestHistoryAnalyzer_Serialize_JSON_UsesComputedMetrics(t *testing.T) {
	t.Parallel()

	c := &HistoryAnalyzer{}

	fm := []map[int]int64{{1: 3}, {0: 3}}
	pm := []map[int]int64{{1: 5}, {0: 5}}

	report := analyze.Report{
		"Files":              []string{"f1.go", "f2.go"},
		"FilesLines":         []int{10, 20},
		"FilesMatrix":        fm,
		"PeopleMatrix":       pm,
		"PeopleFiles":        [][]int{{0, 1}, {0}},
		"ReversedPeopleDict": []string{"dev0", "dev1"},
	}

	var buf bytes.Buffer
	err := c.Serialize(report, analyze.FormatJSON, &buf)
	require.NoError(t, err)

	var result map[string]any
	err = json.Unmarshal(buf.Bytes(), &result)
	require.NoError(t, err)

	// Should have computed metrics structure
	assert.Contains(t, result, "file_coupling")
	assert.Contains(t, result, "developer_coupling")
	assert.Contains(t, result, "file_ownership")
	assert.Contains(t, result, "aggregate")
}

func TestHistoryAnalyzer_Serialize_YAML_UsesComputedMetrics(t *testing.T) {
	t.Parallel()

	c := &HistoryAnalyzer{}

	fm := []map[int]int64{{1: 3}, {0: 3}}
	pm := []map[int]int64{{1: 5}, {0: 5}}

	report := analyze.Report{
		"Files":              []string{"f1.go", "f2.go"},
		"FilesLines":         []int{10, 20},
		"FilesMatrix":        fm,
		"PeopleMatrix":       pm,
		"PeopleFiles":        [][]int{{0, 1}, {0}},
		"ReversedPeopleDict": []string{"dev0", "dev1"},
	}

	var buf bytes.Buffer
	err := c.Serialize(report, analyze.FormatYAML, &buf)
	require.NoError(t, err)

	output := buf.String()
	// Should have computed metrics structure (YAML keys)
	assert.Contains(t, output, "file_coupling:")
	assert.Contains(t, output, "developer_coupling:")
	assert.Contains(t, output, "file_ownership:")
	assert.Contains(t, output, "aggregate:")
}

func TestHistoryAnalyzer_Serialize_Default(t *testing.T) {
	t.Parallel()

	c := &HistoryAnalyzer{}

	report := analyze.Report{
		"Files":              []string{"f1.go"},
		"FilesLines":         []int{10},
		"FilesMatrix":        []map[int]int64{{}},
		"PeopleMatrix":       []map[int]int64{{}},
		"PeopleFiles":        [][]int{{}},
		"ReversedPeopleDict": []string{"dev0"},
	}

	var buf bytes.Buffer
	err := c.Serialize(report, analyze.FormatBinary, &buf)
	require.NoError(t, err)

	if buf.Len() == 0 {
		t.Error("expected output for default format")
	}
}

func TestHistoryAnalyzer_Misc(t *testing.T) {
	t.Parallel()

	c := &HistoryAnalyzer{}
	if c.Name() == "" {
		t.Error("Name empty")
	}

	if c.Flag() == "" {
		t.Error("Flag empty")
	}

	if c.Description() == "" {
		t.Error("Description empty")
	}

	require.NoError(t, c.Initialize(nil))

	clones := c.Fork(2)
	if len(clones) != 2 {
		t.Error("expected 2 clones")
	}
}
