package couples //nolint:testpackage // testing internal implementation.

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
	hash1 := gitplumbing.NewHash("1111111111111111111111111111111111111111")
	change1 := &object.Change{To: object.ChangeEntry{Name: "f1", TreeEntry: object.TreeEntry{Hash: hash1}}}
	change2 := &object.Change{To: object.ChangeEntry{Name: "f2", TreeEntry: object.TreeEntry{Hash: hash1}}}

	c.TreeDiff.Changes = object.Changes{change1, change2}
	c.Identity.AuthorID = 0

	commit1 := &object.Commit{Hash: gitplumbing.NewHash("c1"), Author: object.Signature{When: time.Now()}}
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
	change3 := &object.Change{
		From: object.ChangeEntry{Name: "f1", TreeEntry: object.TreeEntry{Hash: hash1}},
		To:   object.ChangeEntry{Name: "f1", TreeEntry: object.TreeEntry{Hash: hash1}},
	}
	c.TreeDiff.Changes = object.Changes{change3}
	c.Identity.AuthorID = 1 // Author 1 is valid (PeopleNumber 1 -> indices 0, 1. Wait. PeopleNumber usually size?).
	// If PeopleNumber is 1, array size is PeopleNumber+1 (2). Indices 0, 1.

	commit2 := &object.Commit{Hash: gitplumbing.NewHash("c2"), Author: object.Signature{When: time.Now()}}
	require.NoError(t, c.Consume(&analyze.Context{Commit: commit2}))

	if c.people[1]["f1"] != 1 {
		t.Errorf("expected author 1 f1 count 1, got %d", c.people[1]["f1"])
	}

	// 3. Delete f2 (author 0).
	change4 := &object.Change{
		From: object.ChangeEntry{Name: "f2", TreeEntry: object.TreeEntry{Hash: hash1}},
	}
	c.TreeDiff.Changes = object.Changes{change4}
	c.Identity.AuthorID = 0

	commit3 := &object.Commit{Hash: gitplumbing.NewHash("c3"), Author: object.Signature{When: time.Now()}}
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

	commit := &object.Commit{
		Hash:         gitplumbing.NewHash("m1"),
		ParentHashes: []gitplumbing.Hash{gitplumbing.NewHash("p1"), gitplumbing.NewHash("p2")},
		Author:       object.Signature{When: time.Now()},
	}

	// First pass (shouldConsume=true, merges marked).
	require.NoError(t, c.Consume(&analyze.Context{Commit: commit}))

	if !c.merges[commit.Hash] {
		t.Error("expected merge marked")
	}

	// Test Consume logic with IsMerge=true
	// Insert in merge: only if file known?
	// Logic: if !mergeMode || c.files[toName] == nil.

	change := &object.Change{To: object.ChangeEntry{Name: "new_merge.txt"}}
	c.TreeDiff.Changes = object.Changes{change}
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
	// Overlap on f1: dev0 has 10, dev1 has 5. Min is 5.
	// Matrix[0][1] += 5.
	// Matrix[1][0] += 5.
	if pm[0][1] != 5 {
		t.Errorf("expected people matrix 0-1 to be 5, got %d", pm[0][1])
	}

	fm, ok := report["FilesMatrix"].([]map[int]int64)
	require.True(t, ok, "type assertion failed for fm")
	// File f1 (idx 0) and f2 (idx 1) have f1-f2 coocc 3.
	// Matrix[0][1] = 3.
	if fm[0][1] != 3 {
		t.Errorf("expected files matrix 0-1 to be 3, got %d", fm[0][1])
	}
}

func TestHistoryAnalyzer_Serialize(t *testing.T) {
	t.Parallel()

	c := &HistoryAnalyzer{}

	report := analyze.Report{
		"PeopleMatrix":       []map[int]int64{{1: 5}, {0: 5}},
		"PeopleFiles":        [][]int{{0}, {0}},
		"Files":              []string{"f1"},
		"FilesLines":         []int{10},
		"FilesMatrix":        []map[int]int64{{0: 1}},
		"ReversedPeopleDict": []string{"dev0", "dev1"},
	}

	// JSON/YAML.
	var buf bytes.Buffer

	err := c.Serialize(report, false, &buf)
	if err != nil {
		t.Fatalf("Serialize JSON failed: %v", err)
	}

	if !strings.Contains(buf.String(), "files_coocc") {
		t.Error("expected files_coocc in output")
	}

	// Binary.
	var pbuf bytes.Buffer

	err = c.Serialize(report, true, &pbuf)
	if err != nil {
		t.Fatalf("Serialize Binary failed: %v", err)
	}

	if pbuf.Len() == 0 {
		t.Error("expected binary output")
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

	if len(c.ListConfigurationOptions()) != 0 {
		t.Error("expected 0 options")
	}

	require.NoError(t, c.Initialize(nil))

	clones := c.Fork(2)
	if len(clones) != 2 {
		t.Error("expected 2 clones")
	}
}
