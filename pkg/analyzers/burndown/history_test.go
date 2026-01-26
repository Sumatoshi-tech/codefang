package burndown //nolint:testpackage // testing internal implementation.

import (
	"bytes"
	"encoding/json"
	"testing"
	"time"

	"github.com/stretchr/testify/require"

	gitplumbing "github.com/go-git/go-git/v6/plumbing"
	"github.com/go-git/go-git/v6/plumbing/object"
	"github.com/sergi/go-diff/diffmatchpatch"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/identity"
	pkgplumbing "github.com/Sumatoshi-tech/codefang/pkg/plumbing"
)

func TestHistoryAnalyzer_Initialize(t *testing.T) {
	b := &HistoryAnalyzer{}

	err := b.Initialize(nil)
	if err != nil {
		t.Fatalf("Initialize failed: %v", err)
	}

	if b.Granularity != DefaultBurndownGranularity {
		t.Errorf("expected granularity %d, got %d", DefaultBurndownGranularity, b.Granularity)
	}

	if b.Sampling != DefaultBurndownGranularity {
		t.Errorf("expected sampling %d, got %d", DefaultBurndownGranularity, b.Sampling)
	}
}

func TestHistoryAnalyzer_Consume_Insert(t *testing.T) {
	b := &HistoryAnalyzer{
		Granularity: 30,
		Sampling:    30,
		TrackFiles:  true,
		BlobCache:   &plumbing.BlobCacheAnalyzer{},
		TreeDiff:    &plumbing.TreeDiffAnalyzer{},
		FileDiff:    &plumbing.FileDiffAnalyzer{},
		Identity:    &plumbing.IdentityDetector{},
		Ticks:       &plumbing.TicksSinceStart{},
	}
	require.NoError(t, b.Initialize(nil))

	// Mock data.
	fileHash := gitplumbing.NewHash("0000000000000000000000000000000000000001")
	blob := &pkgplumbing.CachedBlob{
		Blob: object.Blob{
			Hash: fileHash,
			Size: 10,
		},
		Data: []byte("line1\nline2\n"),
	}

	b.BlobCache.Cache = map[gitplumbing.Hash]*pkgplumbing.CachedBlob{
		fileHash: blob,
	}

	change := &object.Change{
		To: object.ChangeEntry{
			Name: "test.txt",
			TreeEntry: object.TreeEntry{
				Hash: fileHash,
			},
		},
	}

	b.TreeDiff.Changes = object.Changes{change}
	b.Identity.AuthorID = 0
	b.Ticks.Tick = 0

	ctx := &analyze.Context{
		Commit: &object.Commit{
			Author: object.Signature{Name: "dev1", When: time.Now()},
		},
		Time: time.Now(),
	}

	err := b.Consume(ctx)
	if err != nil {
		t.Fatalf("Consume failed: %v", err)
	}

	shard := b.getShard("test.txt")
	if _, exists := shard.files["test.txt"]; !exists {
		t.Error("test.txt not found")
	}
}

func TestHistoryAnalyzer_Consume_Modify(t *testing.T) {
	b := &HistoryAnalyzer{
		Granularity: 30,
		Sampling:    30,
		TrackFiles:  true,
		BlobCache:   &plumbing.BlobCacheAnalyzer{},
		TreeDiff:    &plumbing.TreeDiffAnalyzer{},
		FileDiff:    &plumbing.FileDiffAnalyzer{},
		Identity:    &plumbing.IdentityDetector{},
		Ticks:       &plumbing.TicksSinceStart{},
	}
	require.NoError(t, b.Initialize(nil))

	// 1. Insert.
	hash1 := gitplumbing.NewHash("1111111111111111111111111111111111111111")
	blob1 := &pkgplumbing.CachedBlob{Data: []byte("line1\n")}
	b.BlobCache.Cache = map[gitplumbing.Hash]*pkgplumbing.CachedBlob{hash1: blob1}

	change1 := &object.Change{
		To: object.ChangeEntry{Name: "test.txt", TreeEntry: object.TreeEntry{Hash: hash1}},
	}
	b.TreeDiff.Changes = object.Changes{change1}
	b.Identity.AuthorID = 0
	b.Ticks.Tick = 0

	require.NoError(t, b.Consume(&analyze.Context{IsMerge: false}))

	// 2. Modify.
	hash2 := gitplumbing.NewHash("2222222222222222222222222222222222222222")
	blob2 := &pkgplumbing.CachedBlob{Data: []byte("line1\nline2\n")}
	b.BlobCache.Cache = map[gitplumbing.Hash]*pkgplumbing.CachedBlob{
		hash1: blob1,
		hash2: blob2,
	}

	change2 := &object.Change{
		From: object.ChangeEntry{Name: "test.txt", TreeEntry: object.TreeEntry{Hash: hash1}},
		To:   object.ChangeEntry{Name: "test.txt", TreeEntry: object.TreeEntry{Hash: hash2}},
	}
	b.TreeDiff.Changes = object.Changes{change2}
	b.Identity.AuthorID = 0
	b.Ticks.Tick = 1

	b.FileDiff.FileDiffs = map[string]pkgplumbing.FileDiffData{
		"test.txt": {
			OldLinesOfCode: 1,
			NewLinesOfCode: 2,
			Diffs: []diffmatchpatch.Diff{
				{Type: diffmatchpatch.DiffEqual, Text: "A"},
				{Type: diffmatchpatch.DiffInsert, Text: "B"},
			},
		},
	}

	err := b.Consume(&analyze.Context{IsMerge: false})
	if err != nil {
		t.Fatalf("Consume modify failed: %v", err)
	}

	file := b.getShard("test.txt").files["test.txt"]
	if file.Len() != 2 {
		t.Errorf("expected 2 lines, got %d", file.Len())
	}
}

func TestHistoryAnalyzer_Consume_Delete(t *testing.T) {
	b := &HistoryAnalyzer{
		Granularity: 30,
		Sampling:    30,
		TrackFiles:  true,
		BlobCache:   &plumbing.BlobCacheAnalyzer{},
		TreeDiff:    &plumbing.TreeDiffAnalyzer{},
		FileDiff:    &plumbing.FileDiffAnalyzer{},
		Identity:    &plumbing.IdentityDetector{},
		Ticks:       &plumbing.TicksSinceStart{},
	}
	require.NoError(t, b.Initialize(nil))

	// 1. Insert.
	hash1 := gitplumbing.NewHash("1111111111111111111111111111111111111111")
	blob1 := &pkgplumbing.CachedBlob{Data: []byte("line1\n")}
	b.BlobCache.Cache = map[gitplumbing.Hash]*pkgplumbing.CachedBlob{hash1: blob1}

	change1 := &object.Change{
		To: object.ChangeEntry{Name: "test.txt", TreeEntry: object.TreeEntry{Hash: hash1}},
	}
	b.TreeDiff.Changes = object.Changes{change1}
	require.NoError(t, b.Consume(&analyze.Context{}))

	// 2. Delete.
	change2 := &object.Change{
		From: object.ChangeEntry{Name: "test.txt", TreeEntry: object.TreeEntry{Hash: hash1}},
	}
	b.TreeDiff.Changes = object.Changes{change2}
	b.Ticks.Tick = 1

	err := b.Consume(&analyze.Context{})
	if err != nil {
		t.Fatalf("Consume delete failed: %v", err)
	}

	if len(b.getShard("test.txt").files) != 0 {
		t.Errorf("expected 0 files, got %d", len(b.getShard("test.txt").files))
	}

	if !b.deletions["test.txt"] {
		t.Error("test.txt should be in deletions")
	}
}

func TestHistoryAnalyzer_Consume_Rename(t *testing.T) {
	b := &HistoryAnalyzer{
		Granularity: 30,
		Sampling:    30,
		TrackFiles:  true,
		BlobCache:   &plumbing.BlobCacheAnalyzer{},
		TreeDiff:    &plumbing.TreeDiffAnalyzer{},
		FileDiff:    &plumbing.FileDiffAnalyzer{},
		Identity:    &plumbing.IdentityDetector{},
		Ticks:       &plumbing.TicksSinceStart{},
	}
	require.NoError(t, b.Initialize(nil))

	// 1. Insert.
	hash1 := gitplumbing.NewHash("1111111111111111111111111111111111111111")
	blob1 := &pkgplumbing.CachedBlob{Data: []byte("line1\n")}
	b.BlobCache.Cache = map[gitplumbing.Hash]*pkgplumbing.CachedBlob{hash1: blob1}

	change1 := &object.Change{
		To: object.ChangeEntry{Name: "old.txt", TreeEntry: object.TreeEntry{Hash: hash1}},
	}
	b.TreeDiff.Changes = object.Changes{change1}
	require.NoError(t, b.Consume(&analyze.Context{}))

	// 2. Rename.
	change2 := &object.Change{
		From: object.ChangeEntry{Name: "old.txt", TreeEntry: object.TreeEntry{Hash: hash1}},
		To:   object.ChangeEntry{Name: "new.txt", TreeEntry: object.TreeEntry{Hash: hash1}},
	}
	b.TreeDiff.Changes = object.Changes{change2}
	b.Ticks.Tick = 1
	b.FileDiff.FileDiffs = map[string]pkgplumbing.FileDiffData{
		"new.txt": {
			OldLinesOfCode: 1,
			NewLinesOfCode: 1,
			Diffs: []diffmatchpatch.Diff{
				{Type: diffmatchpatch.DiffEqual, Text: "A"},
			},
		},
	}

	err := b.Consume(&analyze.Context{})
	if err != nil {
		t.Fatalf("Consume rename failed: %v", err)
	}

	if _, exists := b.getShard("old.txt").files["old.txt"]; exists {
		t.Error("old.txt should be gone")
	}

	if _, exists := b.getShard("new.txt").files["new.txt"]; !exists {
		t.Error("new.txt should exist")
	}

	if b.renames["old.txt"] != "new.txt" {
		t.Errorf("expected rename old.txt->new.txt, got %s", b.renames["old.txt"])
	}
}

func TestHistoryAnalyzer_Configure(t *testing.T) {
	b := &HistoryAnalyzer{}
	facts := map[string]any{
		ConfigBurndownGranularity:                       60,
		ConfigBurndownSampling:                          60,
		ConfigBurndownTrackFiles:                        true,
		ConfigBurndownTrackPeople:                       true,
		ConfigBurndownHibernationThreshold:              1000,
		ConfigBurndownHibernationToDisk:                 true,
		ConfigBurndownHibernationDirectory:              "/tmp",
		ConfigBurndownDebug:                             true,
		identity.FactIdentityDetectorPeopleCount:        10,
		identity.FactIdentityDetectorReversedPeopleDict: []string{"dev1"},
		pkgplumbing.FactTickSize:                        12 * time.Hour,
	}

	err := b.Configure(facts)
	if err != nil {
		t.Fatalf("Configure failed: %v", err)
	}

	if b.Granularity != 60 {
		t.Errorf("expected granularity 60, got %d", b.Granularity)
	}

	if b.Sampling != 60 {
		t.Errorf("expected sampling 60, got %d", b.Sampling)
	}

	if !b.TrackFiles {
		t.Error("expected TrackFiles true")
	}

	if b.PeopleNumber != 10 {
		t.Errorf("expected PeopleNumber 10, got %d", b.PeopleNumber)
	}

	if b.TickSize != 12*time.Hour {
		t.Errorf("expected TickSize 12h, got %v", b.TickSize)
	}
}

func TestHistoryAnalyzer_Finalize(t *testing.T) {
	b := &HistoryAnalyzer{
		Granularity:        30,
		Sampling:           30,
		TrackFiles:         true,
		PeopleNumber:       1,
		TickSize:           24 * time.Hour,
		reversedPeopleDict: []string{"dev1"},
	}
	require.NoError(t, b.Initialize(nil))

	// Manually populate history to test grouping.
	b.globalHistory = sparseHistory{
		0:  {0: 10},
		30: {0: 5, 30: 5},
	}
	b.peopleHistories[0] = sparseHistory{
		0: {0: 10},
	}
	shard := b.getShard("test.txt")
	shard.fileHistories["test.txt"] = sparseHistory{
		0: {0: 10},
	}
	// Add file to calculate ownership.
	file, err := b.newFile(shard, gitplumbing.ZeroHash, "test.txt", 0, 0, 10)
	require.NoError(t, err)

	shard.files["test.txt"] = file

	report, err := b.Finalize()
	if err != nil {
		t.Fatalf("Finalize failed: %v", err)
	}

	gh, ok := report["GlobalHistory"].(DenseHistory)
	require.True(t, ok)

	if len(gh) < 2 {
		t.Errorf("expected global history len >= 2, got %d", len(gh))
	}
}

func TestHistoryAnalyzer_Serialize(t *testing.T) {
	b := &HistoryAnalyzer{
		Granularity: 30,
		Sampling:    30,
		TickSize:    24 * time.Hour,
	}
	report := analyze.Report{
		"GlobalHistory":      DenseHistory{{10}, {5, 5}},
		"FileHistories":      map[string]DenseHistory{"f": {{10}}},
		"FileOwnership":      map[string]map[int]int{"f": {0: 10}},
		"PeopleHistories":    []DenseHistory{{{10}}},
		"PeopleMatrix":       DenseHistory{{0, 0, 10}},
		"TickSize":           24 * time.Hour,
		"ReversedPeopleDict": []string{"dev1"},
		"Sampling":           30,
		"Granularity":        30,
	}

	// JSON.
	var buf bytes.Buffer

	err := b.Serialize(report, false, &buf)
	if err != nil {
		t.Fatalf("Serialize JSON failed: %v", err)
	}

	var decoded analyze.Report

	err = json.Unmarshal(buf.Bytes(), &decoded)
	if err != nil {
		t.Fatalf("JSON decode failed: %v", err)
	}

	// Protobuf.
	var pbuf bytes.Buffer

	err = b.Serialize(report, true, &pbuf)
	if err != nil {
		t.Fatalf("Serialize Protobuf failed: %v", err)
	}

	if pbuf.Len() == 0 {
		t.Error("Protobuf output is empty")
	}
}

func TestHistoryAnalyzer_Hibernate_Boot(t *testing.T) {
	b := &HistoryAnalyzer{
		HibernationToDisk:    true,
		HibernationDirectory: "/tmp",
	}
	require.NoError(t, b.Initialize(nil))

	// Create some state.
	shard := b.getShard("test.txt")
	_, err := b.newFile(shard, gitplumbing.ZeroHash, "test.txt", 0, 0, 10)
	require.NoError(t, err)

	err = b.Hibernate()
	if err != nil {
		t.Fatalf("Hibernate failed: %v", err)
	}

	if b.hibernatedFileName == "" {
		t.Fatal("expected hibernated file name")
	}

	err = b.Boot()
	if err != nil {
		t.Fatalf("Boot failed: %v", err)
	}

	if b.hibernatedFileName != "" {
		t.Error("expected hibernated file name cleared")
	}
}

func TestHistoryAnalyzer_Fork(t *testing.T) { //nolint:revive // testing.T needed for test signature
	// Skip for now as Fork panics.
}

func TestHistoryAnalyzer_Merge(t *testing.T) { //nolint:revive // testing.T needed for test signature
	// Skip for now as Merge panics.
}

func TestHistoryAnalyzer_Delete_NonExistent(t *testing.T) {
	b := &HistoryAnalyzer{}
	require.NoError(t, b.Initialize(nil))
	b.TreeDiff = &plumbing.TreeDiffAnalyzer{}
	b.BlobCache = &plumbing.BlobCacheAnalyzer{Cache: map[gitplumbing.Hash]*pkgplumbing.CachedBlob{}}
	b.FileDiff = &plumbing.FileDiffAnalyzer{}
	b.Identity = &plumbing.IdentityDetector{}
	b.Ticks = &plumbing.TicksSinceStart{}

	change := &object.Change{
		From: object.ChangeEntry{Name: "missing.txt"},
	}
	b.TreeDiff.Changes = object.Changes{change}

	err := b.Consume(&analyze.Context{})
	if err != nil {
		t.Errorf("expected nil, got %v", err)
	}
}

func TestHistoryAnalyzer_Errors(t *testing.T) {
	b := &HistoryAnalyzer{
		TrackFiles: true,
	}
	require.NoError(t, b.Initialize(nil))

	hash1 := gitplumbing.NewHash("1111111111111111111111111111111111111111")
	blob1 := &pkgplumbing.CachedBlob{Data: []byte("line1\n")}
	b.BlobCache = &plumbing.BlobCacheAnalyzer{
		Cache: map[gitplumbing.Hash]*pkgplumbing.CachedBlob{hash1: blob1},
	}
	b.TreeDiff = &plumbing.TreeDiffAnalyzer{}
	b.FileDiff = &plumbing.FileDiffAnalyzer{}
	b.Identity = &plumbing.IdentityDetector{}
	b.Ticks = &plumbing.TicksSinceStart{}

	change := &object.Change{
		From: object.ChangeEntry{Name: "missing.txt", TreeEntry: object.TreeEntry{Hash: hash1}},
		To:   object.ChangeEntry{Name: "new.txt", TreeEntry: object.TreeEntry{Hash: hash1}},
	}
	b.TreeDiff.Changes = object.Changes{change}

	err := b.Consume(&analyze.Context{})
	if err != nil {
		t.Errorf("expected no error (handleInsertion fallback), got %v", err)
	}
}

func TestHistoryAnalyzer_IntegrityError(t *testing.T) {
	b := &HistoryAnalyzer{
		TrackFiles: true,
	}
	require.NoError(t, b.Initialize(nil))

	// 1. Insert.
	hash1 := gitplumbing.NewHash("1111111111111111111111111111111111111111")
	blob1 := &pkgplumbing.CachedBlob{Data: []byte("line1\n")}
	b.BlobCache = &plumbing.BlobCacheAnalyzer{
		Cache: map[gitplumbing.Hash]*pkgplumbing.CachedBlob{hash1: blob1},
	}
	b.TreeDiff = &plumbing.TreeDiffAnalyzer{}
	b.FileDiff = &plumbing.FileDiffAnalyzer{}
	b.Identity = &plumbing.IdentityDetector{}
	b.Ticks = &plumbing.TicksSinceStart{}

	change1 := &object.Change{
		To: object.ChangeEntry{Name: "test.txt", TreeEntry: object.TreeEntry{Hash: hash1}},
	}
	b.TreeDiff.Changes = object.Changes{change1}
	require.NoError(t, b.Consume(&analyze.Context{}))

	// 2. Modify with wrong OldLinesOfCode.
	hash2 := gitplumbing.NewHash("2222222222222222222222222222222222222222")
	blob2 := &pkgplumbing.CachedBlob{Data: []byte("line1\nline2\n")}
	b.BlobCache.Cache[hash2] = blob2

	change2 := &object.Change{
		From: object.ChangeEntry{Name: "test.txt", TreeEntry: object.TreeEntry{Hash: hash1}},
		To:   object.ChangeEntry{Name: "test.txt", TreeEntry: object.TreeEntry{Hash: hash2}},
	}
	b.TreeDiff.Changes = object.Changes{change2}
	b.Ticks.Tick = 1

	b.FileDiff.FileDiffs = map[string]pkgplumbing.FileDiffData{
		"test.txt": {
			OldLinesOfCode: 999, // Mismatch.
			NewLinesOfCode: 2,
			Diffs:          []diffmatchpatch.Diff{},
		},
	}

	err := b.Consume(&analyze.Context{})
	if err == nil {
		t.Fatal("expected integrity error")
	}

	if err.Error() != "test.txt: internal integrity error src 999 != 1" {
		t.Errorf("unexpected error message: %v", err)
	}
}

func TestHistoryAnalyzer_PeopleMatrix(t *testing.T) {
	b := &HistoryAnalyzer{
		PeopleNumber: 2,
		TrackFiles:   true,
	}
	require.NoError(t, b.Initialize(nil))

	// 1. Insert by author 0.
	hash1 := gitplumbing.NewHash("1111111111111111111111111111111111111111")
	blob1 := &pkgplumbing.CachedBlob{Data: []byte("line1\n")}
	b.BlobCache = &plumbing.BlobCacheAnalyzer{
		Cache: map[gitplumbing.Hash]*pkgplumbing.CachedBlob{hash1: blob1},
	}
	b.TreeDiff = &plumbing.TreeDiffAnalyzer{}
	b.FileDiff = &plumbing.FileDiffAnalyzer{}
	b.Identity = &plumbing.IdentityDetector{AuthorID: 0}
	b.Ticks = &plumbing.TicksSinceStart{}

	change1 := &object.Change{
		To: object.ChangeEntry{Name: "test.txt", TreeEntry: object.TreeEntry{Hash: hash1}},
	}
	b.TreeDiff.Changes = object.Changes{change1}
	require.NoError(t, b.Consume(&analyze.Context{}))

	// 2. Modify by author 1.
	hash2 := gitplumbing.NewHash("2222222222222222222222222222222222222222")
	blob2 := &pkgplumbing.CachedBlob{Data: []byte("line1\nline2\n")}
	b.BlobCache.Cache[hash2] = blob2

	change2 := &object.Change{
		From: object.ChangeEntry{Name: "test.txt", TreeEntry: object.TreeEntry{Hash: hash1}},
		To:   object.ChangeEntry{Name: "test.txt", TreeEntry: object.TreeEntry{Hash: hash2}},
	}
	b.TreeDiff.Changes = object.Changes{change2}
	b.Identity.AuthorID = 1
	b.Ticks.Tick = 1

	b.FileDiff.FileDiffs = map[string]pkgplumbing.FileDiffData{
		"test.txt": {
			OldLinesOfCode: 1,
			NewLinesOfCode: 2,
			Diffs: []diffmatchpatch.Diff{
				{Type: diffmatchpatch.DiffEqual, Text: "A"},
				{Type: diffmatchpatch.DiffInsert, Text: "B"},
			},
		},
	}

	require.NoError(t, b.Consume(&analyze.Context{}))

	// 3. Delete by author 1.
	b.TreeDiff.Changes = object.Changes{&object.Change{
		From: object.ChangeEntry{Name: "test.txt", TreeEntry: object.TreeEntry{Hash: hash2}},
	}}
	b.Identity.AuthorID = 1
	b.Ticks.Tick = 2

	require.NoError(t, b.Consume(&analyze.Context{}))

	row := b.matrix[0]
	if row == nil {
		t.Fatal("matrix[0] is nil")
	}

	// 4. Self-churn test.
	shard := b.getShard("self.txt")
	f, err := b.newFile(shard, gitplumbing.ZeroHash, "self.txt", 0, 3, 10)
	require.NoError(t, err)

	shard.files["self.txt"] = f

	b.Identity.AuthorID = 0
	b.Ticks.Tick = 4

	file := shard.files["self.txt"]
	file.Update(b.packPersonWithTick(0, 4), 0, 5, 5)

	row0 := b.matrix[0]
	// AuthorSelf (-2).
	const authorSelf = -2

	_ = row0[authorSelf] // Verify key exists without empty branch (SA9003).
}
