package burndown //nolint:testpackage // testing internal implementation.

import (
	"testing"

	"github.com/stretchr/testify/require"

	gitplumbing "github.com/go-git/go-git/v6/plumbing"
	"github.com/go-git/go-git/v6/plumbing/object"
	"github.com/sergi/go-diff/diffmatchpatch"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/identity"
	pkgplumbing "github.com/Sumatoshi-tech/codefang/pkg/plumbing"
)

func TestBurndownHistoryAnalyzer_Configure_NegativePeople(t *testing.T) {
	b := &BurndownHistoryAnalyzer{}
	facts := map[string]any{
		ConfigBurndownTrackPeople:                true,
		identity.FactIdentityDetectorPeopleCount: -1,
	}

	err := b.Configure(facts)
	if err == nil {
		t.Fatal("expected error for negative people count")
	}
}

func TestBurndownHistoryAnalyzer_Initialize_NegativePeople(t *testing.T) {
	b := &BurndownHistoryAnalyzer{
		PeopleNumber: -1,
	}

	err := b.Initialize(nil)
	if err == nil {
		t.Fatal("expected error for negative people count")
	}
}

func TestBurndownHistoryAnalyzer_Consume_Insert_Exists(t *testing.T) {
	b := &BurndownHistoryAnalyzer{
		TrackFiles: true,
	}
	require.NoError(t, b.Initialize(nil))

	// Create file.
	hash1 := gitplumbing.NewHash("1111111111111111111111111111111111111111")
	blob1 := &pkgplumbing.CachedBlob{Data: []byte("line1\n")}
	b.BlobCache = &plumbing.BlobCacheAnalyzer{
		Cache: map[gitplumbing.Hash]*pkgplumbing.CachedBlob{hash1: blob1},
	}
	b.TreeDiff = &plumbing.TreeDiffAnalyzer{}
	b.FileDiff = &plumbing.FileDiffAnalyzer{}
	b.Identity = &plumbing.IdentityDetector{}
	b.Ticks = &plumbing.TicksSinceStart{}

	name := "test.txt"
	shard := b.getShard(name)
	f, err := b.newFile(shard, hash1, name, 0, 0, 1)
	require.NoError(t, err)

	shard.files[name] = f

	// Try to insert again.
	change := &object.Change{
		To: object.ChangeEntry{Name: name, TreeEntry: object.TreeEntry{Hash: hash1}},
	}
	b.TreeDiff.Changes = object.Changes{change}

	err = b.Consume(&analyze.Context{})
	if err == nil {
		t.Fatal("expected error for existing file")
	}

	if err.Error() != "file test.txt already exists" {
		t.Errorf("unexpected error: %v", err)
	}
}

func TestBurndownHistoryAnalyzer_Modify_Binary(t *testing.T) {
	b := &BurndownHistoryAnalyzer{TrackFiles: true}
	require.NoError(t, b.Initialize(nil))

	hashText := gitplumbing.NewHash("1111111111111111111111111111111111111111")
	blobText := &pkgplumbing.CachedBlob{Data: []byte("line1\n")}

	hashBin := gitplumbing.NewHash("2222222222222222222222222222222222222222")
	blobBin := &pkgplumbing.CachedBlob{Data: []byte{0x00, 0x01}}

	b.BlobCache = &plumbing.BlobCacheAnalyzer{
		Cache: map[gitplumbing.Hash]*pkgplumbing.CachedBlob{
			hashText: blobText,
			hashBin:  blobBin,
		},
	}
	b.TreeDiff = &plumbing.TreeDiffAnalyzer{}
	b.FileDiff = &plumbing.FileDiffAnalyzer{}
	b.Identity = &plumbing.IdentityDetector{}
	b.Ticks = &plumbing.TicksSinceStart{}

	name1 := "test.txt"
	shard1 := b.getShard(name1)
	f1, err := b.newFile(shard1, hashText, name1, 0, 0, 1)
	require.NoError(t, err)

	shard1.files[name1] = f1

	change1 := &object.Change{
		From: object.ChangeEntry{Name: name1, TreeEntry: object.TreeEntry{Hash: hashText}},
		To:   object.ChangeEntry{Name: name1, TreeEntry: object.TreeEntry{Hash: hashBin}},
	}
	b.TreeDiff.Changes = object.Changes{change1}

	require.NoError(t, b.Consume(&analyze.Context{}))

	if _, exists := shard1.files[name1]; exists {
		t.Error("expected file deletion (text->binary)")
	}

	name2 := "test2.txt"
	shard2 := b.getShard(name2)
	change2 := &object.Change{
		From: object.ChangeEntry{Name: name2, TreeEntry: object.TreeEntry{Hash: hashBin}},
		To:   object.ChangeEntry{Name: name2, TreeEntry: object.TreeEntry{Hash: hashText}},
	}
	b.TreeDiff.Changes = object.Changes{change2}

	require.NoError(t, b.Consume(&analyze.Context{}))

	if _, exists := shard2.files[name2]; !exists {
		t.Error("expected file insertion (binary->text)")
	}

	name3 := "test3.txt"
	change3 := &object.Change{
		From: object.ChangeEntry{Name: name3, TreeEntry: object.TreeEntry{Hash: hashBin}},
		To:   object.ChangeEntry{Name: name3, TreeEntry: object.TreeEntry{Hash: hashBin}},
	}
	b.TreeDiff.Changes = object.Changes{change3}

	err = b.Consume(&analyze.Context{})
	if err != nil {
		t.Errorf("expected no error for binary->binary, got %v", err)
	}
}

func TestBurndownHistoryAnalyzer_Consume_Hibernated(t *testing.T) {
	b := &BurndownHistoryAnalyzer{TrackFiles: true}
	require.NoError(t, b.Initialize(nil))

	name := "test.txt"
	shard := b.getShard(name)
	f, err := b.newFile(shard, gitplumbing.ZeroHash, name, 0, 0, 1)
	require.NoError(t, err)

	shard.files[name] = f

	b.shardedAllocator.Hibernate()

	defer func() {
		if r := recover(); r == nil {
			// Now that hibernation is supported, we expect no panic when using hibernated allocator
			// because ShardedAllocator handles it transparently?
			// Actually, ShardedAllocator.malloc() calls shards[i].malloc() which panics if storage is nil.
			// So we still expect panic if we try to access it.
			t.Error("expected panic on consumed hibernated")
		}
	}()

	require.NoError(t, b.Consume(&analyze.Context{}))
}

func TestBurndownHistoryAnalyzer_Finalize_Empty(t *testing.T) {
	b := &BurndownHistoryAnalyzer{PeopleNumber: 1}
	require.NoError(t, b.Initialize(nil))

	report, err := b.Finalize()
	if err != nil {
		t.Fatalf("Finalize failed: %v", err)
	}

	if len(report) == 0 {
		t.Error("expected report")
	}
}

func TestBurndownHistoryAnalyzer_Consume_Merge_Paths(t *testing.T) {
	b := &BurndownHistoryAnalyzer{TrackFiles: true}
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

	// 1. Insertion during merge.
	change1 := &object.Change{
		To: object.ChangeEntry{Name: "merge_insert.txt", TreeEntry: object.TreeEntry{Hash: hash1}},
	}
	b.TreeDiff.Changes = object.Changes{change1}

	require.NoError(t, b.Consume(&analyze.Context{IsMerge: true}))

	if !b.mergedFiles["merge_insert.txt"] {
		t.Error("expected merge_insert.txt to be in mergedFiles")
	}

	// 2. Modification during merge.
	name2 := "modify.txt"
	shard2 := b.getShard(name2)
	f2, err := b.newFile(shard2, hash1, name2, 0, 0, 1)
	require.NoError(t, err)

	shard2.files[name2] = f2

	hash2 := gitplumbing.NewHash("2222222222222222222222222222222222222222")
	blob2 := &pkgplumbing.CachedBlob{Data: []byte("line1\nline2\n")}
	b.BlobCache.Cache[hash2] = blob2

	change2 := &object.Change{
		From: object.ChangeEntry{Name: name2, TreeEntry: object.TreeEntry{Hash: hash1}},
		To:   object.ChangeEntry{Name: name2, TreeEntry: object.TreeEntry{Hash: hash2}},
	}
	b.TreeDiff.Changes = object.Changes{change2}
	b.FileDiff.FileDiffs = map[string]pkgplumbing.FileDiffData{
		name2: {
			OldLinesOfCode: 1, NewLinesOfCode: 2,
			Diffs: []diffmatchpatch.Diff{
				{Type: diffmatchpatch.DiffEqual, Text: "A"},
				{Type: diffmatchpatch.DiffInsert, Text: "B"},
			},
		},
	}

	require.NoError(t, b.Consume(&analyze.Context{IsMerge: true}))

	if !b.mergedFiles[name2] {
		t.Error("expected modify.txt to be in mergedFiles")
	}

	// 3. Deletion during merge.
	name3 := "delete.txt"
	shard3 := b.getShard(name3)
	f3, err := b.newFile(shard3, hash1, name3, 0, 0, 1)
	require.NoError(t, err)

	shard3.files[name3] = f3

	change3 := &object.Change{
		From: object.ChangeEntry{Name: name3, TreeEntry: object.TreeEntry{Hash: hash1}},
	}
	b.TreeDiff.Changes = object.Changes{change3}

	require.NoError(t, b.Consume(&analyze.Context{IsMerge: true}))

	if b.mergedFiles[name3] {
		t.Error("expected delete.txt NOT to be true in mergedFiles")
	}

	if val, ok := b.mergedFiles[name3]; !ok || val {
		t.Errorf("expected delete.txt false in mergedFiles, got %v %v", ok, val)
	}

	// 4. Rename during merge.
	name4old := "old.txt"
	name4new := "new.txt"
	shard4old := b.getShard(name4old)
	f4, err := b.newFile(shard4old, hash1, name4old, 0, 0, 1)
	require.NoError(t, err)

	shard4old.files[name4old] = f4

	change4 := &object.Change{
		From: object.ChangeEntry{Name: name4old, TreeEntry: object.TreeEntry{Hash: hash1}},
		To:   object.ChangeEntry{Name: name4new, TreeEntry: object.TreeEntry{Hash: hash1}},
	}
	b.TreeDiff.Changes = object.Changes{change4}
	b.FileDiff.FileDiffs = map[string]pkgplumbing.FileDiffData{
		name4new: {OldLinesOfCode: 1, NewLinesOfCode: 1, Diffs: []diffmatchpatch.Diff{{Type: diffmatchpatch.DiffEqual, Text: "A"}}},
	}

	require.NoError(t, b.Consume(&analyze.Context{IsMerge: true}))

	if val, ok := b.mergedFiles[name4old]; !ok || val {
		t.Errorf("expected old.txt false in mergedFiles, got %v %v", ok, val)
	}

	if !b.mergedFiles[name4new] {
		t.Error("expected new.txt to be in mergedFiles")
	}
}
