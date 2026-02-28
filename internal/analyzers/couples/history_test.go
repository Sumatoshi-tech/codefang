package couples

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"testing"
	"time"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/plumbing"
	"github.com/Sumatoshi-tech/codefang/internal/identity"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
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

	c := &HistoryAnalyzer{
		PeopleNumber:       1,
		reversedPeopleDict: []string{"dev"},
	}

	err := c.Initialize(nil)
	if err != nil {
		t.Fatalf("Initialize failed: %v", err)
	}

	require.NotNil(t, c.seenFiles)
	require.NotNil(t, c.merges)
}

func TestHistoryAnalyzer_Consume_ReturnsTC(t *testing.T) {
	t.Parallel()

	c := &HistoryAnalyzer{
		PeopleNumber: 1,
		Identity:     &plumbing.IdentityDetector{},
		TreeDiff:     &plumbing.TreeDiffAnalyzer{},
	}
	require.NoError(t, c.Initialize(nil))

	hash1 := gitlib.NewHash("1111111111111111111111111111111111111111")
	change1 := &gitlib.Change{Action: gitlib.Insert, To: gitlib.ChangeEntry{Name: "f1", Hash: hash1}}
	change2 := &gitlib.Change{Action: gitlib.Insert, To: gitlib.ChangeEntry{Name: "f2", Hash: hash1}}

	c.TreeDiff.Changes = gitlib.Changes{change1, change2}
	c.Identity.AuthorID = 0

	commit := gitlib.NewTestCommit(
		gitlib.NewHash("c100000000000000000000000000000000000001"),
		gitlib.Signature{When: time.Now()},
		"insert",
	)

	tc, err := c.Consume(context.Background(), &analyze.Context{Commit: commit})
	require.NoError(t, err)

	cd, ok := tc.Data.(*CommitData)
	require.True(t, ok, "expected *CommitData")
	assert.ElementsMatch(t, []string{"f1", "f2"}, cd.CouplingFiles)
	assert.Equal(t, 1, cd.AuthorFiles["f1"]) // Touch count = 1 per file per commit.
	assert.Equal(t, 1, cd.AuthorFiles["f2"])
	assert.True(t, cd.CommitCounted)
	assert.Empty(t, cd.Renames)

	// seenFiles should be updated.
	assert.True(t, c.seenFiles.Test([]byte("f1")))
	assert.True(t, c.seenFiles.Test([]byte("f2")))
}

func TestHistoryAnalyzer_Consume_Delete(t *testing.T) {
	t.Parallel()

	c := &HistoryAnalyzer{
		PeopleNumber: 1,
		Identity:     &plumbing.IdentityDetector{},
		TreeDiff:     &plumbing.TreeDiffAnalyzer{},
	}
	require.NoError(t, c.Initialize(nil))

	hash1 := gitlib.NewHash("1111111111111111111111111111111111111111")
	change := &gitlib.Change{
		Action: gitlib.Delete,
		From:   gitlib.ChangeEntry{Name: "f1", Hash: hash1},
	}

	c.TreeDiff.Changes = gitlib.Changes{change}
	c.Identity.AuthorID = 0

	commit := gitlib.NewTestCommit(
		gitlib.NewHash("c200000000000000000000000000000000000002"),
		gitlib.Signature{When: time.Now()},
		"delete",
	)

	tc, err := c.Consume(context.Background(), &analyze.Context{Commit: commit})
	require.NoError(t, err)

	cd, ok := tc.Data.(*CommitData)
	require.True(t, ok)
	// Deletes don't add to coupling context.
	assert.Empty(t, cd.CouplingFiles)
	assert.Equal(t, 1, cd.AuthorFiles["f1"]) // Touch count = 1 (deletes still record author touch).
}

func TestHistoryAnalyzer_Consume_Rename(t *testing.T) {
	t.Parallel()

	c := &HistoryAnalyzer{
		PeopleNumber: 1,
		Identity:     &plumbing.IdentityDetector{},
		TreeDiff:     &plumbing.TreeDiffAnalyzer{},
	}
	require.NoError(t, c.Initialize(nil))

	hash1 := gitlib.NewHash("1111111111111111111111111111111111111111")
	change := &gitlib.Change{
		Action: gitlib.Modify,
		From:   gitlib.ChangeEntry{Name: "old.go", Hash: hash1},
		To:     gitlib.ChangeEntry{Name: "new.go", Hash: hash1},
	}

	c.TreeDiff.Changes = gitlib.Changes{change}
	c.Identity.AuthorID = 0

	commit := gitlib.NewTestCommit(
		gitlib.NewHash("c300000000000000000000000000000000000003"),
		gitlib.Signature{When: time.Now()},
		"rename",
	)

	tc, err := c.Consume(context.Background(), &analyze.Context{Commit: commit})
	require.NoError(t, err)

	cd, ok := tc.Data.(*CommitData)
	require.True(t, ok)
	assert.Len(t, cd.Renames, 1)
	assert.Equal(t, "old.go", cd.Renames[0].FromName)
	assert.Equal(t, "new.go", cd.Renames[0].ToName)
	assert.Contains(t, cd.CouplingFiles, "new.go")
}

func TestHistoryAnalyzer_Consume_MergeDedup(t *testing.T) {
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

	c.TreeDiff.Changes = gitlib.Changes{}
	c.Identity.AuthorID = 0

	// First pass: shouldConsume=true.
	tc1, err := c.Consume(context.Background(), &analyze.Context{Commit: commit})
	require.NoError(t, err)

	cd1, ok := tc1.Data.(*CommitData)
	require.True(t, ok)
	assert.True(t, cd1.CommitCounted)
	assert.True(t, c.merges.SeenOrAdd(commit.Hash()), "merge commit should already be tracked")

	// Second pass: shouldConsume=false (duplicate merge).
	tc2, err := c.Consume(context.Background(), &analyze.Context{Commit: commit})
	require.NoError(t, err)

	// Empty TC since nothing meaningful happened.
	cd2, ok := tc2.Data.(*CommitData)
	require.True(t, ok)
	assert.False(t, cd2.CommitCounted)
}

func TestHistoryAnalyzer_Consume_MergeMode(t *testing.T) {
	t.Parallel()

	c := &HistoryAnalyzer{
		PeopleNumber: 1,
		Identity:     &plumbing.IdentityDetector{},
		TreeDiff:     &plumbing.TreeDiffAnalyzer{},
	}
	require.NoError(t, c.Initialize(nil))

	// Pre-populate seenFiles.
	c.seenFiles.Add([]byte("existing.go"))

	hash := gitlib.NewHash("1111111111111111111111111111111111111111")
	change1 := &gitlib.Change{Action: gitlib.Insert, To: gitlib.ChangeEntry{Name: "existing.go", Hash: hash}}
	change2 := &gitlib.Change{Action: gitlib.Insert, To: gitlib.ChangeEntry{Name: "new.go", Hash: hash}}

	c.TreeDiff.Changes = gitlib.Changes{change1, change2}
	c.Identity.AuthorID = 0

	commit := gitlib.NewTestCommit(
		gitlib.NewHash("c400000000000000000000000000000000000004"),
		gitlib.Signature{When: time.Now()},
		"merge_mode",
	)

	tc, err := c.Consume(context.Background(), &analyze.Context{Commit: commit, IsMerge: true})
	require.NoError(t, err)

	cd, ok := tc.Data.(*CommitData)
	require.True(t, ok)
	// existing.go should be filtered from coupling context (already seen in merge mode),
	// but author attribution should still be recorded for ownership tracking.
	assert.Equal(t, []string{"new.go"}, cd.CouplingFiles)
	assert.Equal(t, 1, cd.AuthorFiles["new.go"])      // Touch count = 1 per file per commit.
	assert.Equal(t, 1, cd.AuthorFiles["existing.go"]) // Author touch recorded even for seen files.
}

func TestHistoryAnalyzer_Fork_WorkingStateOnly(t *testing.T) {
	t.Parallel()

	c := &HistoryAnalyzer{
		PeopleNumber:       2,
		reversedPeopleDict: []string{"alice", "bob"},
		seenFiles:          newSeenFilesFilter(),
		merges:             analyze.NewMergeTracker(),
	}
	require.NoError(t, c.Initialize(nil))

	clones := c.Fork(2)
	require.Len(t, clones, 2)

	for _, clone := range clones {
		cc, ok := clone.(*HistoryAnalyzer)
		require.True(t, ok)
		assert.NotNil(t, cc.seenFiles)
		assert.NotNil(t, cc.merges)
		assert.Equal(t, 2, cc.PeopleNumber)
		assert.Equal(t, []string{"alice", "bob"}, cc.reversedPeopleDict)
	}
}

func TestHistoryAnalyzer_Merge_WorkingState(t *testing.T) {
	t.Parallel()

	main := &HistoryAnalyzer{PeopleNumber: 1}
	require.NoError(t, main.Initialize(nil))

	hash1 := gitlib.NewHash("1111111111111111111111111111111111111111")
	main.merges.SeenOrAdd(hash1)
	main.seenFiles.Add([]byte("a.go"))

	branch := &HistoryAnalyzer{PeopleNumber: 1}
	require.NoError(t, branch.Initialize(nil))

	hash2 := gitlib.NewHash("2222222222222222222222222222222222222222")
	branch.merges.SeenOrAdd(hash2)
	branch.seenFiles.Add([]byte("b.go"))

	commit := gitlib.NewTestCommit(
		gitlib.NewHash("c500000000000000000000000000000000000005"),
		gitlib.Signature{When: time.Now()},
		"last",
	)
	branch.lastCommit = commit

	main.Merge([]analyze.HistoryAnalyzer{branch})

	assert.True(t, main.merges.SeenOrAdd(hash1), "hash1 should still be in main's tracker")
	// Merge trackers are not combined — each fork processes disjoint commits.
	assert.False(t, main.merges.SeenOrAdd(hash2), "hash2 should NOT be in main's tracker (independent forks)")
	assert.True(t, main.seenFiles.Test([]byte("a.go")))
	// seenFiles also stays independent — each fork gets its own filter.
	assert.False(t, main.seenFiles.Test([]byte("b.go")))
	assert.Equal(t, commit, main.lastCommit)
}

func TestHistoryAnalyzer_Serialize_JSON(t *testing.T) {
	t.Parallel()

	c := NewHistoryAnalyzer()

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

	assert.Contains(t, result, "file_coupling")
	assert.Contains(t, result, "developer_coupling")
	assert.Contains(t, result, "file_ownership")
	assert.Contains(t, result, "aggregate")
}

func TestHistoryAnalyzer_Serialize_YAML(t *testing.T) {
	t.Parallel()

	c := NewHistoryAnalyzer()

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
	assert.Contains(t, output, "file_coupling:")
	assert.Contains(t, output, "developer_coupling:")
	assert.Contains(t, output, "file_ownership:")
	assert.Contains(t, output, "aggregate:")
}

func TestHistoryAnalyzer_Serialize_Binary(t *testing.T) {
	t.Parallel()

	c := NewHistoryAnalyzer()

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
		t.Error("expected output for binary format")
	}
}

func TestHistoryAnalyzer_Consume_OversizedChangeset(t *testing.T) {
	t.Parallel()

	c := &HistoryAnalyzer{
		PeopleNumber: 1,
		Identity:     &plumbing.IdentityDetector{},
		TreeDiff:     &plumbing.TreeDiffAnalyzer{},
	}
	require.NoError(t, c.Initialize(nil))

	// Create a changeset larger than the maximum meaningful context size.
	hash := gitlib.NewHash("1111111111111111111111111111111111111111")
	changes := make(gitlib.Changes, CouplesMaximumMeaningfulContextSize+1)

	for i := range changes {
		changes[i] = &gitlib.Change{
			Action: gitlib.Insert,
			To:     gitlib.ChangeEntry{Name: fmt.Sprintf("f%d.go", i), Hash: hash},
		}
	}

	c.TreeDiff.Changes = changes
	c.Identity.AuthorID = 0

	commit := gitlib.NewTestCommit(
		gitlib.NewHash("c600000000000000000000000000000000000006"),
		gitlib.Signature{When: time.Now()},
		"mass_change",
	)

	tc, err := c.Consume(context.Background(), &analyze.Context{Commit: commit})
	require.NoError(t, err)

	cd, ok := tc.Data.(*CommitData)
	require.True(t, ok)
	// Oversized changeset should produce empty coupling data.
	assert.Empty(t, cd.CouplingFiles)
	assert.Empty(t, cd.AuthorFiles)
	assert.True(t, cd.CommitCounted)
}

func TestHistoryAnalyzer_Consume_ExactMaxChangeset(t *testing.T) {
	t.Parallel()

	c := &HistoryAnalyzer{
		PeopleNumber: 1,
		Identity:     &plumbing.IdentityDetector{},
		TreeDiff:     &plumbing.TreeDiffAnalyzer{},
	}
	require.NoError(t, c.Initialize(nil))

	// Exactly at the limit — should still be processed.
	hash := gitlib.NewHash("1111111111111111111111111111111111111111")
	changes := make(gitlib.Changes, CouplesMaximumMeaningfulContextSize)

	for i := range changes {
		changes[i] = &gitlib.Change{
			Action: gitlib.Insert,
			To:     gitlib.ChangeEntry{Name: fmt.Sprintf("f%d.go", i), Hash: hash},
		}
	}

	c.TreeDiff.Changes = changes
	c.Identity.AuthorID = 0

	commit := gitlib.NewTestCommit(
		gitlib.NewHash("c700000000000000000000000000000000000007"),
		gitlib.Signature{When: time.Now()},
		"exact_limit",
	)

	tc, err := c.Consume(context.Background(), &analyze.Context{Commit: commit})
	require.NoError(t, err)

	cd, ok := tc.Data.(*CommitData)
	require.True(t, ok)
	// Exactly at limit should still be processed.
	assert.Len(t, cd.CouplingFiles, CouplesMaximumMeaningfulContextSize)
}

func TestHistoryAnalyzer_Misc(t *testing.T) {
	t.Parallel()

	c := &HistoryAnalyzer{}
	assert.NotEmpty(t, c.Name())
	assert.NotEmpty(t, c.Flag())
	assert.NotEmpty(t, c.Description())

	require.NoError(t, c.Initialize(nil))

	clones := c.Fork(2)
	assert.Len(t, clones, 2)
}

func TestHistoryAnalyzer_NewAggregator(t *testing.T) {
	t.Parallel()

	c := &HistoryAnalyzer{
		PeopleNumber:       2,
		reversedPeopleDict: []string{"alice", "bob"},
		seenFiles:          newSeenFilesFilter(),
		merges:             analyze.NewMergeTracker(),
	}
	require.NoError(t, c.Initialize(nil))

	agg := c.NewAggregator(analyze.AggregatorOptions{})
	require.NotNil(t, agg)
	require.NoError(t, agg.Close())
}

func TestHistoryAnalyzer_SerializeTICKs(t *testing.T) {
	t.Parallel()

	ticks := []analyze.TICK{
		{
			Tick: 0,
			Data: &TickData{
				Files: map[string]map[string]int{
					"a.go": {"b.go": 3, "a.go": 5},
					"b.go": {"a.go": 3, "b.go": 5},
				},
				People:        []map[string]int{{"a.go": 10, "b.go": 5}, {}},
				PeopleCommits: []int{15, 0},
			},
		},
	}

	c := NewHistoryAnalyzer()
	c.PeopleNumber = 1
	c.reversedPeopleDict = []string{"dev"}

	var buf bytes.Buffer

	err := c.SerializeTICKs(ticks, analyze.FormatJSON, &buf)
	require.NoError(t, err)

	var result map[string]any
	require.NoError(t, json.Unmarshal(buf.Bytes(), &result))
	assert.Contains(t, result, "file_coupling")
}

func TestExtractCommitTimeSeries(t *testing.T) {
	t.Parallel()

	hashA := "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
	hashB := "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"

	report := analyze.Report{
		"commit_stats": map[string]*CouplesCommitSummary{
			hashA: {FilesTouched: 5, AuthorID: 0},
			hashB: {FilesTouched: 3, AuthorID: 1},
		},
	}

	c := &HistoryAnalyzer{}
	result := c.ExtractCommitTimeSeries(report)

	require.Len(t, result, 2)

	entryA, ok := result[hashA].(map[string]any)
	require.True(t, ok)
	assert.Equal(t, 5, entryA["files_touched"])
	assert.Equal(t, 0, entryA["author_id"])

	entryB, ok := result[hashB].(map[string]any)
	require.True(t, ok)
	assert.Equal(t, 3, entryB["files_touched"])
	assert.Equal(t, 1, entryB["author_id"])
}

func TestExtractCommitTimeSeries_Empty(t *testing.T) {
	t.Parallel()

	c := &HistoryAnalyzer{}

	assert.Nil(t, c.ExtractCommitTimeSeries(analyze.Report{}))
	assert.Nil(t, c.ExtractCommitTimeSeries(analyze.Report{
		"commit_stats": map[string]*CouplesCommitSummary{},
	}))
}
