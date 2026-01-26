package plumbing //nolint:testpackage // testing internal implementation.

import (
	"bytes"
	"testing"
	"time"

	"github.com/stretchr/testify/require"

	"github.com/go-git/go-git/v6"
	gitplumbing "github.com/go-git/go-git/v6/plumbing"
	"github.com/go-git/go-git/v6/plumbing/object"
	"github.com/go-git/go-git/v6/storage/memory"
	"github.com/sergi/go-diff/diffmatchpatch"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/identity"
	pkgplumbing "github.com/Sumatoshi-tech/codefang/pkg/plumbing"
)

func TestTreeDiffAnalyzer_Consume_Full(t *testing.T) {
	s := memory.NewStorage()
	r, err := git.Init(s, nil)
	require.NoError(t, err)

	td := &TreeDiffAnalyzer{}
	require.NoError(t, td.Initialize(r))
	require.NoError(t, td.Configure(map[string]any{
		ConfigTreeDiffLanguages:           []string{"Go"},
		ConfigTreeDiffEnableBlacklist:     true,
		ConfigTreeDiffBlacklistedPrefixes: []string{"vendor/"},
	}))

	// Create blobs.
	createBlob := func(content string) gitplumbing.Hash {
		obj := r.Storer.NewEncodedObject()
		obj.SetType(gitplumbing.BlobObject)
		obj.SetSize(int64(len(content)))
		w, blobErr := obj.Writer()
		require.NoError(t, blobErr)
		_, blobErr = w.Write([]byte(content))
		require.NoError(t, blobErr)
		w.Close()

		h, blobErr := r.Storer.SetEncodedObject(obj)
		require.NoError(t, blobErr)

		if h == gitplumbing.ZeroHash {
			t.Fatal("Zero hash returned")
		}

		return h
	}

	hGo := createBlob("package main")
	hTxt := createBlob("text")

	dummyTree := &object.Tree{}
	changes := object.Changes{
		&object.Change{To: object.ChangeEntry{Name: "file.go", TreeEntry: object.TreeEntry{Hash: hGo}, Tree: dummyTree}},
		&object.Change{To: object.ChangeEntry{Name: "other.txt", TreeEntry: object.TreeEntry{Hash: hTxt}, Tree: dummyTree}},
		&object.Change{To: object.ChangeEntry{Name: "vendor/a.go", TreeEntry: object.TreeEntry{Hash: hGo}, Tree: dummyTree}},
	}

	filtered := td.filterDiffs(changes)
	if len(filtered) != 1 {
		t.Errorf("expected 1 change, got %d", len(filtered))
	}

	if len(filtered) > 0 && filtered[0].To.Name != "file.go" {
		t.Errorf("expected file.go, got %s", filtered[0].To.Name)
	}

	// Test Consume logic with previous commit
	// Create commit 1.
	tree1 := object.Tree{Entries: []object.TreeEntry{
		{Name: "file.go", Mode: 0o100644, Hash: hGo},
	}}
	tObj1 := r.Storer.NewEncodedObject()
	tObj1.SetType(gitplumbing.TreeObject)
	require.NoError(t, tree1.Encode(tObj1))
	treeHash1, err := r.Storer.SetEncodedObject(tObj1)
	require.NoError(t, err)

	cObj1 := r.Storer.NewEncodedObject()
	cObj1.SetType(gitplumbing.CommitObject)

	commit1 := &object.Commit{TreeHash: treeHash1}
	require.NoError(t, commit1.Encode(cObj1))
	commitHash1, err := r.Storer.SetEncodedObject(cObj1)
	require.NoError(t, err)
	realCommit1, err := r.CommitObject(commitHash1)
	require.NoError(t, err)

	// Create commit 2 (modified).
	hGo2 := createBlob("package main\n// mod")
	tree2 := object.Tree{Entries: []object.TreeEntry{
		{Name: "file.go", Mode: 0o100644, Hash: hGo2},
	}}
	tObj2 := r.Storer.NewEncodedObject()
	tObj2.SetType(gitplumbing.TreeObject)
	require.NoError(t, tree2.Encode(tObj2))
	treeHash2, err := r.Storer.SetEncodedObject(tObj2)
	require.NoError(t, err)

	cObj2 := r.Storer.NewEncodedObject()
	cObj2.SetType(gitplumbing.CommitObject)

	commit2 := &object.Commit{TreeHash: treeHash2, ParentHashes: []gitplumbing.Hash{commitHash1}}
	require.NoError(t, commit2.Encode(cObj2))
	commitHash2, err := r.Storer.SetEncodedObject(cObj2)
	require.NoError(t, err)
	realCommit2, err := r.CommitObject(commitHash2)
	require.NoError(t, err)

	// Consume C1.
	err = td.Consume(&analyze.Context{Commit: realCommit1})
	if err != nil {
		t.Fatal(err)
	}

	if len(td.Changes) != 1 {
		t.Errorf("expected 1 change in C1, got %d", len(td.Changes))
	}

	// Consume C2.
	err = td.Consume(&analyze.Context{Commit: realCommit2})
	if err != nil {
		t.Fatal(err)
	}

	if len(td.Changes) != 1 {
		t.Errorf("expected 1 change in C2, got %d", len(td.Changes))
	}

	// Misc.
	_, err = td.Finalize()
	require.NoError(t, err)
	td.Fork(1)
	td.Merge(nil)
	require.NoError(t, td.Serialize(nil, false, nil))
}

func TestFileDiffAnalyzer_Consume_Full(t *testing.T) {
	// ... Existing test covers basic flow
	// Add test for cleanup and whitespace ignore.
	fd := &FileDiffAnalyzer{
		TreeDiff:  &TreeDiffAnalyzer{},
		BlobCache: &BlobCacheAnalyzer{Cache: map[gitplumbing.Hash]*pkgplumbing.CachedBlob{}},
	}
	require.NoError(t, fd.Initialize(nil))
	require.NoError(t, fd.Configure(map[string]any{
		ConfigFileWhitespaceIgnore: true,
	}))

	h1 := gitplumbing.NewHash("1111111111111111111111111111111111111111")
	h2 := gitplumbing.NewHash("2222222222222222222222222222222222222222")

	fd.BlobCache.Cache[h1] = &pkgplumbing.CachedBlob{Data: []byte("a  b")}
	fd.BlobCache.Cache[h2] = &pkgplumbing.CachedBlob{Data: []byte("ab")}

	change := &object.Change{
		From: object.ChangeEntry{Name: "f", TreeEntry: object.TreeEntry{Hash: h1}},
		To:   object.ChangeEntry{Name: "f", TreeEntry: object.TreeEntry{Hash: h2}},
	}
	fd.TreeDiff.Changes = object.Changes{change}

	require.NoError(t, fd.Consume(&analyze.Context{}))

	diff := fd.FileDiffs["f"]
	if len(diff.Diffs) != 1 || diff.Diffs[0].Type != diffmatchpatch.DiffEqual {
		t.Error("expected equal diff with whitespace ignored")
	}

	// Test Misc.
	_, err := fd.Finalize()
	require.NoError(t, err)
	fd.Fork(1)
	fd.Merge(nil)
	require.NoError(t, fd.Serialize(nil, false, nil))
}

func TestBlobCacheAnalyzer_Consume(t *testing.T) {
	bc := &BlobCacheAnalyzer{
		TreeDiff: &TreeDiffAnalyzer{},
	}
	require.NoError(t, bc.Initialize(nil))
}

func TestIdentityDetector_Consume(t *testing.T) {
	id := &IdentityDetector{}
	require.NoError(t, id.Initialize(nil))

	// 1. Author found in dict.
	id.PeopleDict = map[string]int{"dev@example.com": 1}
	commit1 := &object.Commit{
		Author: object.Signature{Name: "Dev", Email: "dev@example.com"},
	}
	require.NoError(t, id.Consume(&analyze.Context{Commit: commit1}))

	if id.AuthorID != 1 {
		t.Errorf("expected author 1, got %d", id.AuthorID)
	}

	// 2. Author not found.
	commit2 := &object.Commit{
		Author: object.Signature{Name: "Unknown", Email: "unk@example.com"},
	}
	require.NoError(t, id.Consume(&analyze.Context{Commit: commit2}))

	if id.AuthorID != identity.AuthorMissing {
		t.Errorf("expected author %d, got %d", identity.AuthorMissing, id.AuthorID)
	}
}

func TestTicksSinceStart_Consume(t *testing.T) {
	ts := &TicksSinceStart{
		TickSize: 24 * time.Hour,
	}
	require.NoError(t, ts.Initialize(nil))

	// Tick 0.
	start := time.Date(2020, 1, 1, 10, 0, 0, 0, time.UTC)
	commit1 := &object.Commit{
		Committer: object.Signature{When: start},
		Hash:      gitplumbing.NewHash("c1"),
	}
	require.NoError(t, ts.Consume(&analyze.Context{Commit: commit1, Index: 0}))

	if ts.Tick != 0 {
		t.Errorf("expected tick 0, got %d", ts.Tick)
	}

	// Tick 1 (25 hours later).
	commit2 := &object.Commit{
		Committer:    object.Signature{When: start.Add(25 * time.Hour)},
		Hash:         gitplumbing.NewHash("c2"),
		ParentHashes: []gitplumbing.Hash{commit1.Hash},
	}
	require.NoError(t, ts.Consume(&analyze.Context{Commit: commit2, Index: 1}))

	if ts.Tick != 1 {
		t.Errorf("expected tick 1, got %d", ts.Tick)
	}
}

func TestFileDiffAnalyzer_Consume(t *testing.T) {
	fd := &FileDiffAnalyzer{
		TreeDiff:  &TreeDiffAnalyzer{},
		BlobCache: &BlobCacheAnalyzer{Cache: map[gitplumbing.Hash]*pkgplumbing.CachedBlob{}},
	}
	require.NoError(t, fd.Initialize(nil))

	hash1 := gitplumbing.NewHash("1111111111111111111111111111111111111111")
	hash2 := gitplumbing.NewHash("2222222222222222222222222222222222222222")

	blob1 := &pkgplumbing.CachedBlob{Data: []byte("line1\n")}
	blob2 := &pkgplumbing.CachedBlob{Data: []byte("line1\nline2\n")}

	fd.BlobCache.Cache[hash1] = blob1
	fd.BlobCache.Cache[hash2] = blob2

	change := &object.Change{
		From: object.ChangeEntry{Name: "test.txt", TreeEntry: object.TreeEntry{Hash: hash1}},
		To:   object.ChangeEntry{Name: "test.txt", TreeEntry: object.TreeEntry{Hash: hash2}},
	}
	fd.TreeDiff.Changes = object.Changes{change}

	require.NoError(t, fd.Consume(&analyze.Context{}))

	diff := fd.FileDiffs["test.txt"]
	if diff.OldLinesOfCode != 1 {
		t.Errorf("expected 1 old line, got %d", diff.OldLinesOfCode)
	}

	if diff.NewLinesOfCode != 2 {
		t.Errorf("expected 2 new lines, got %d", diff.NewLinesOfCode)
	}
}

func TestLinesStatsCalculator_Consume(t *testing.T) {
	ls := &LinesStatsCalculator{
		TreeDiff:  &TreeDiffAnalyzer{},
		BlobCache: &BlobCacheAnalyzer{Cache: map[gitplumbing.Hash]*pkgplumbing.CachedBlob{}},
		FileDiff:  &FileDiffAnalyzer{FileDiffs: map[string]pkgplumbing.FileDiffData{}},
	}
	require.NoError(t, ls.Initialize(nil))

	// 1. Insert.
	hash := gitplumbing.NewHash("1111111111111111111111111111111111111111")
	ls.BlobCache.Cache[hash] = &pkgplumbing.CachedBlob{Data: []byte("line1\n"), Blob: object.Blob{Size: 6}}

	change := &object.Change{
		To: object.ChangeEntry{Name: "new.txt", TreeEntry: object.TreeEntry{Hash: hash}},
	}
	_ = change

	ls.TreeDiff.Changes = object.Changes{change}

	require.NoError(t, ls.Consume(&analyze.Context{}))

	stats := ls.LineStats[change.To]
	if stats.Added != 1 {
		t.Errorf("expected 1 added line, got %d", stats.Added)
	}
}

func TestBlobCacheAnalyzer_Configure(t *testing.T) {
	b := &BlobCacheAnalyzer{}
	facts := map[string]any{
		ConfigBlobCacheFailOnMissingSubmodules: true,
	}

	err := b.Configure(facts)
	if err != nil {
		t.Errorf("Configure failed: %v", err)
	}

	if !b.FailOnMissingSubmodules {
		t.Error("expected FailOnMissingSubmodules true")
	}

	if len(b.ListConfigurationOptions()) == 0 {
		t.Error("expected options")
	}

	if b.Name() == "" || b.Flag() == "" || b.Description() == "" {
		t.Error("metadata missing")
	}
}

func TestFileDiffAnalyzer_Configure(t *testing.T) {
	f := &FileDiffAnalyzer{}
	facts := map[string]any{
		ConfigFileDiffDisableCleanup: true,
		ConfigFileWhitespaceIgnore:   true,
		ConfigFileDiffTimeout:        500,
	}

	err := f.Configure(facts)
	if err != nil {
		t.Errorf("Configure failed: %v", err)
	}

	if !f.CleanupDisabled {
		t.Error("expected CleanupDisabled true")
	}

	if !f.WhitespaceIgnore {
		t.Error("expected WhitespaceIgnore true")
	}

	if f.Timeout != 500*time.Millisecond {
		t.Errorf("expected Timeout 500ms, got %v", f.Timeout)
	}

	if len(f.ListConfigurationOptions()) == 0 {
		t.Error("expected options")
	}
}

func TestIdentityDetector_Configure(t *testing.T) {
	id := &IdentityDetector{}
	facts := map[string]any{
		ConfigIdentityDetectorExactSignatures:           true,
		identity.FactIdentityDetectorPeopleDict:         map[string]int{"dev": 1},
		identity.FactIdentityDetectorReversedPeopleDict: []string{"dev"},
	}

	err := id.Configure(facts)
	if err != nil {
		t.Errorf("Configure failed: %v", err)
	}

	if !id.ExactSignatures {
		t.Error("expected ExactSignatures true")
	}

	if id.PeopleDict["dev"] != 1 {
		t.Error("expected PeopleDict populated")
	}

	if len(id.ListConfigurationOptions()) == 0 {
		t.Error("expected options")
	}

	commits := []*object.Commit{
		{Author: object.Signature{Name: "Dev", Email: "dev@example.com"}},
	}
	id.GeneratePeopleDict(commits)

	if len(id.PeopleDict) == 0 {
		t.Error("expected generated dict")
	}
}

func TestTicksSinceStart_Configure(t *testing.T) {
	ts := &TicksSinceStart{}
	facts := map[string]any{
		ConfigTicksSinceStartTickSize: 12,
	}

	err := ts.Configure(facts)
	if err != nil {
		t.Errorf("Configure failed: %v", err)
	}

	if ts.TickSize != 12*time.Hour {
		t.Errorf("expected 12h, got %v", ts.TickSize)
	}

	if ts.commits == nil {
		t.Error("expected commits initialized")
	}

	if len(ts.ListConfigurationOptions()) == 0 {
		t.Error("expected options")
	}
}

func TestTreeDiffAnalyzer_Configure(t *testing.T) {
	td := &TreeDiffAnalyzer{}
	facts := map[string]any{
		ConfigTreeDiffEnableBlacklist:     true,
		ConfigTreeDiffBlacklistedPrefixes: []string{"vendor/"},
		ConfigTreeDiffLanguages:           []string{"Go"},
		ConfigTreeDiffFilterRegexp:        "^.*$",
	}

	err := td.Configure(facts)
	if err != nil {
		t.Errorf("Configure failed: %v", err)
	}

	if len(td.SkipFiles) != 1 {
		t.Error("expected SkipFiles")
	}

	if !td.Languages["go"] {
		t.Error("expected go language")
	}

	if td.NameFilter == nil {
		t.Error("expected NameFilter")
	}

	if len(td.ListConfigurationOptions()) == 0 {
		t.Error("expected options")
	}
}

func TestLanguagesDetectionAnalyzer(t *testing.T) {
	ld := &LanguagesDetectionAnalyzer{}
	if ld.Name() == "" {
		t.Error("Name empty")
	}

	if len(ld.ListConfigurationOptions()) != 0 {
		t.Error("expected 0 options")
	}

	if ld.Configure(nil) != nil {
		t.Error("Configure failed")
	}

	require.NoError(t, ld.Initialize(nil))

	if ld.Languages == nil {
		ld.Languages = map[gitplumbing.Hash]string{}
	}

	ld.BlobCache = &BlobCacheAnalyzer{Cache: map[gitplumbing.Hash]*pkgplumbing.CachedBlob{}}
	ld.TreeDiff = &TreeDiffAnalyzer{}

	hash := gitplumbing.NewHash("1111111111111111111111111111111111111111")
	ld.BlobCache.Cache[hash] = &pkgplumbing.CachedBlob{Data: []byte("package main\n")}

	change := &object.Change{
		To: object.ChangeEntry{Name: "main.go", TreeEntry: object.TreeEntry{Hash: hash}},
	}
	ld.TreeDiff.Changes = object.Changes{change}

	require.NoError(t, ld.Consume(&analyze.Context{}))

	lang := ld.Languages[hash]
	if lang != "Go" {
		t.Errorf("expected Go, got %s", lang)
	}
}

func TestLinesStatsCalculator_Misc(t *testing.T) {
	ls := &LinesStatsCalculator{}
	if ls.Name() == "" {
		t.Error("Name empty")
	}

	if len(ls.ListConfigurationOptions()) != 0 {
		t.Error("expected 0 options")
	}

	if ls.Configure(nil) != nil {
		t.Error("Configure failed")
	}
}

func TestUASTChangesAnalyzer(t *testing.T) {
	c := &UASTChangesAnalyzer{
		FileDiff:  &FileDiffAnalyzer{FileDiffs: map[string]pkgplumbing.FileDiffData{}},
		BlobCache: &BlobCacheAnalyzer{},
	}
	if c.Name() == "" {
		t.Error("Name empty")
	}

	if len(c.ListConfigurationOptions()) != 0 {
		t.Error("expected 0 options")
	}

	if c.Configure(nil) != nil {
		t.Error("Configure failed")
	}

	err := c.Initialize(nil)
	if err != nil {
		t.Fatal(err)
	}

	// Consume.
	c.FileDiff.FileDiffs["test.go"] = pkgplumbing.FileDiffData{
		Diffs: []diffmatchpatch.Diff{{Type: diffmatchpatch.DiffInsert, Text: "a"}},
	}

	err = c.Consume(&analyze.Context{})
	if err != nil {
		t.Fatal(err)
	}

	if len(c.Changes) != 1 {
		t.Errorf("expected 1 change, got %d", len(c.Changes))
	}

	// Misc.
	_, err = c.Finalize()
	require.NoError(t, err)
	c.Merge(nil)
	require.NoError(t, c.Serialize(nil, false, nil))
	c.Fork(2)
}

func TestBlobCacheAnalyzer_Consume_Full(t *testing.T) {
	// Init repo.
	s := memory.NewStorage()

	r, err := git.Init(s, nil)
	require.NoError(t, err)

	// Manually create blob.
	content := []byte("content")
	obj := r.Storer.NewEncodedObject()
	obj.SetType(gitplumbing.BlobObject)
	obj.SetSize(int64(len(content)))
	w, err := obj.Writer()
	require.NoError(t, err)
	_, err = w.Write(content)
	require.NoError(t, err)
	w.Close()

	blobHash, err := r.Storer.SetEncodedObject(obj)
	require.NoError(t, err)

	bc := &BlobCacheAnalyzer{
		TreeDiff: &TreeDiffAnalyzer{},
	}
	require.NoError(t, bc.Initialize(r))

	// Setup TreeDiff change.
	change := &object.Change{
		To: object.ChangeEntry{Name: "test.txt", TreeEntry: object.TreeEntry{Hash: blobHash}},
	}
	bc.TreeDiff.Changes = object.Changes{change}

	// Consume
	// Mock Context.Commit.File.
	ctx := &analyze.Context{
		Commit: &object.Commit{},
	}
	// Commit.File is used in Consume. But commit is empty.
	// The implementation of Consume uses commit.File as FileGetter?
	// Case merkletrie.Insert: blob, err = b.getBlob(&change.To, commit.File)
	// commit.File is a method on *object.Commit.
	// If commit is empty, calling commit.File("path") might fail if it relies on tree?
	// Yes, commit.File accesses tree.

	// So I need a proper commit object that points to a tree containing the file.
	// Create Tree.
	treeEntry := object.TreeEntry{Name: "test.txt", Mode: 0o100644, Hash: blobHash}
	tree := object.Tree{Entries: []object.TreeEntry{treeEntry}}
	tObj := r.Storer.NewEncodedObject()
	tObj.SetType(gitplumbing.TreeObject)
	require.NoError(t, tree.Encode(tObj))
	treeHash, err := r.Storer.SetEncodedObject(tObj)
	require.NoError(t, err)

	ctx.Commit.TreeHash = treeHash
	// Need to attach repository to commit?
	// Object.Commit doesn't have repository field public?
	// We can use r.CommitObject(hash) to get commit linked to repo.

	cObj := r.Storer.NewEncodedObject()
	cObj.SetType(gitplumbing.CommitObject)

	commit := &object.Commit{
		TreeHash:  treeHash,
		Author:    object.Signature{Name: "Me", When: time.Now()},
		Committer: object.Signature{Name: "Me", When: time.Now()},
		Message:   "msg",
	}
	require.NoError(t, commit.Encode(cObj))
	commitHash, err := r.Storer.SetEncodedObject(cObj)
	require.NoError(t, err)

	realCommit, err := r.CommitObject(commitHash)
	require.NoError(t, err)

	ctx.Commit = realCommit

	err = bc.Consume(ctx)
	if err != nil {
		t.Fatal(err)
	}

	// Check cache.
	cached, ok := bc.Cache[blobHash]
	if !ok {
		t.Error("expected blob cached")
	} else if string(cached.Data) != "content" {
		t.Errorf("expected content, got %s", cached.Data)
	}

	// Test getBlob.
	entry := &object.ChangeEntry{TreeEntry: object.TreeEntry{Hash: blobHash}}

	blob, err := bc.getBlob(entry, nil)
	if err != nil {
		t.Error(err)
	}

	reader, err := blob.Reader()
	require.NoError(t, err)

	buf := new(bytes.Buffer)
	_, err = buf.ReadFrom(reader)
	require.NoError(t, err)

	if buf.String() != "content" {
		t.Error("getBlob mismatch")
	}

	// Misc.
	_, err = bc.Finalize()
	require.NoError(t, err)
	bc.Fork(1)
	bc.Merge(nil)
	require.NoError(t, bc.Serialize(nil, false, nil))
}

func TestFileDiffAnalyzer_Consume_InsertDelete(t *testing.T) {
	fd := &FileDiffAnalyzer{
		TreeDiff:  &TreeDiffAnalyzer{},
		BlobCache: &BlobCacheAnalyzer{Cache: map[gitplumbing.Hash]*pkgplumbing.CachedBlob{}},
	}
	require.NoError(t, fd.Initialize(nil))

	h := gitplumbing.NewHash("1111111111111111111111111111111111111111")
	fd.BlobCache.Cache[h] = &pkgplumbing.CachedBlob{Data: []byte("content")}

	// Insert.
	c1 := &object.Change{To: object.ChangeEntry{Name: "new", TreeEntry: object.TreeEntry{Hash: h}}}

	// Delete.
	c2 := &object.Change{From: object.ChangeEntry{Name: "old", TreeEntry: object.TreeEntry{Hash: h}}}

	fd.TreeDiff.Changes = object.Changes{c1, c2}

	require.NoError(t, fd.Consume(&analyze.Context{}))

	if len(fd.FileDiffs) != 0 {
		t.Errorf("expected 0 diffs, got %d", len(fd.FileDiffs))
	}
}
