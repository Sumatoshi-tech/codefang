package file_history //nolint:staticcheck,testpackage // testing internal implementation.

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
	pkgplumbing "github.com/Sumatoshi-tech/codefang/pkg/plumbing"
)

func TestFileHistoryAnalyzer_Consume(t *testing.T) {
	t.Parallel()

	h := &FileHistoryAnalyzer{
		Identity:  &plumbing.IdentityDetector{},
		TreeDiff:  &plumbing.TreeDiffAnalyzer{},
		LineStats: &plumbing.LinesStatsCalculator{},
	}
	require.NoError(t, h.Initialize(nil))

	// 1. Insert.
	hash1 := gitplumbing.NewHash("1111111111111111111111111111111111111111")
	change1 := &object.Change{
		To: object.ChangeEntry{Name: "test.txt", TreeEntry: object.TreeEntry{Hash: hash1}},
	}
	h.TreeDiff.Changes = object.Changes{change1}

	h.Identity.AuthorID = 0
	h.LineStats.LineStats = map[object.ChangeEntry]pkgplumbing.LineStats{
		change1.To: {Added: 10, Removed: 0, Changed: 0},
	}

	commit1 := &object.Commit{Hash: gitplumbing.NewHash("c1"), Author: object.Signature{When: time.Now()}}
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
	hash2 := gitplumbing.NewHash("2222222222222222222222222222222222222222")
	change2 := &object.Change{
		From: object.ChangeEntry{Name: "test.txt", TreeEntry: object.TreeEntry{Hash: hash1}},
		To:   object.ChangeEntry{Name: "test.txt", TreeEntry: object.TreeEntry{Hash: hash2}},
	}
	h.TreeDiff.Changes = object.Changes{change2}
	h.Identity.AuthorID = 1
	h.LineStats.LineStats = map[object.ChangeEntry]pkgplumbing.LineStats{
		change2.To: {Added: 5, Removed: 2, Changed: 3},
	}

	commit2 := &object.Commit{Hash: gitplumbing.NewHash("c2"), Author: object.Signature{When: time.Now()}}
	require.NoError(t, h.Consume(&analyze.Context{Commit: commit2}))

	if len(fh.Hashes) != 2 {
		t.Errorf("expected 2 commits, got %d", len(fh.Hashes))
	}

	if fh.People[1].Added != 5 {
		t.Errorf("expected 5 added lines for author 1, got %d", fh.People[1].Added)
	}

	// 3. Rename.
	change3 := &object.Change{
		From: object.ChangeEntry{Name: "test.txt", TreeEntry: object.TreeEntry{Hash: hash2}},
		To:   object.ChangeEntry{Name: "renamed.txt", TreeEntry: object.TreeEntry{Hash: hash2}},
	}
	h.TreeDiff.Changes = object.Changes{change3}
	h.Identity.AuthorID = 0
	h.LineStats.LineStats = map[object.ChangeEntry]pkgplumbing.LineStats{
		change3.To: {Added: 0, Removed: 0, Changed: 0},
	}

	commit3 := &object.Commit{Hash: gitplumbing.NewHash("c3"), Author: object.Signature{When: time.Now()}}
	require.NoError(t, h.Consume(&analyze.Context{Commit: commit3}))

	if _, ok := h.files["test.txt"]; ok {
		t.Error("test.txt should be gone")
	}

	if _, ok := h.files["renamed.txt"]; !ok {
		t.Error("renamed.txt should exist")
	}

	fh = h.files["renamed.txt"] // Should be same object (or new one with copied stats?)
	// Implementation: h.files[change.To.Name] = oldFH
	// So same object.
	if len(fh.Hashes) != 3 {
		t.Errorf("expected 3 commits, got %d", len(fh.Hashes))
	}

	// 4. Delete.
	change4 := &object.Change{
		From: object.ChangeEntry{Name: "renamed.txt", TreeEntry: object.TreeEntry{Hash: hash2}},
	}
	h.TreeDiff.Changes = object.Changes{change4}
	h.Identity.AuthorID = 0
	h.LineStats.LineStats = map[object.ChangeEntry]pkgplumbing.LineStats{
		change4.From: {Added: 0, Removed: 13, Changed: 0},
	}

	commit4 := &object.Commit{Hash: gitplumbing.NewHash("c4"), Author: object.Signature{When: time.Now()}}
	require.NoError(t, h.Consume(&analyze.Context{Commit: commit4}))

	// Deleted file remains in history?
	// Implementation:
	// case merkletrie.Delete: fh.Hashes = append(fh.Hashes, commit.Hash)
	// It doesn't delete from h.files.
	if _, ok := h.files["renamed.txt"]; !ok {
		t.Error("renamed.txt should still exist in history")
	}

	if len(fh.Hashes) != 4 {
		t.Errorf("expected 4 commits, got %d", len(fh.Hashes))
	}
}

func TestFileHistoryAnalyzer_Merge(t *testing.T) {
	t.Parallel()

	h := &FileHistoryAnalyzer{
		Identity:  &plumbing.IdentityDetector{},
		TreeDiff:  &plumbing.TreeDiffAnalyzer{},
		LineStats: &plumbing.LinesStatsCalculator{},
	}
	require.NoError(t, h.Initialize(nil))

	// Simulate merge commit.
	commit := &object.Commit{
		Hash:         gitplumbing.NewHash("m1"),
		Author:       object.Signature{When: time.Now()},
		ParentHashes: []gitplumbing.Hash{gitplumbing.NewHash("p1"), gitplumbing.NewHash("p2")},
	}

	// First call should consume (NumParents > 1 -> merges[hash] = true)
	// shouldConsume = true (not in merges yet).
	err := h.Consume(&analyze.Context{Commit: commit})
	if err != nil {
		t.Fatalf("Consume failed: %v", err)
	}

	if !h.merges[commit.Hash] {
		t.Error("expected merge to be recorded")
	}

	// Second call for same commit (diamond merge or similar logic? Or re-entry?)
	// If Consume called again with same commit?
	// If h.merges[commit.Hash] { shouldConsume = false }.

	err = h.Consume(&analyze.Context{Commit: commit})
	if err != nil {
		t.Fatalf("Consume 2 failed: %v", err)
	}
	// Changes shouldn't be processed 2nd time.
	// We can check by side effects.
}

func TestFileHistoryAnalyzer_Serialize(t *testing.T) {
	t.Parallel()

	h := &FileHistoryAnalyzer{}
	require.NoError(t, h.Initialize(nil))

	// Manually construct report to avoid Finalize logic dependencies.
	report := analyze.Report{
		"Files": map[string]FileHistory{
			"test.txt": {
				Hashes: []gitplumbing.Hash{gitplumbing.NewHash("c1")},
				People: map[int]pkgplumbing.LineStats{
					0: {Added: 10, Removed: 0, Changed: 5},
				},
			},
		},
	}

	// JSON.
	var buf bytes.Buffer

	err := h.Serialize(report, false, &buf)
	if err != nil {
		t.Fatalf("Serialize JSON failed: %v", err)
	}

	if !strings.Contains(buf.String(), "test.txt") {
		t.Error("expected test.txt in JSON output")
	}

	if !strings.Contains(buf.String(), "10,0,5") {
		t.Error("expected stats in JSON output")
	}

	// Binary.
	var pbuf bytes.Buffer

	err = h.Serialize(report, true, &pbuf)
	if err != nil {
		t.Fatalf("Serialize Binary failed: %v", err)
	}

	if pbuf.Len() == 0 {
		t.Error("expected binary output")
	}
}

func TestFileHistoryAnalyzer_Misc(t *testing.T) {
	t.Parallel()

	h := &FileHistoryAnalyzer{}
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

	c1, ok := clones[0].(*FileHistoryAnalyzer)
	require.True(t, ok, "type assertion failed for c1")

	if len(c1.files) != 1 {
		t.Error("expected 1 file in clone")
	}
}
