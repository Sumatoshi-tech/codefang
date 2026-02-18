package sentiment

import (
	"bytes"
	"context"
	"encoding/json"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
	pkgplumbing "github.com/Sumatoshi-tech/codefang/pkg/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/uast"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

func TestHistoryAnalyzer_Configure(t *testing.T) {
	t.Parallel()

	s := &HistoryAnalyzer{}
	facts := map[string]any{
		ConfigCommentSentimentGap:       float32(0.8),
		ConfigCommentSentimentMinLength: 30,
		pkgplumbing.FactCommitsByTick:   map[int][]gitlib.Hash{},
	}

	err := s.Configure(facts)
	if err != nil {
		t.Fatalf("Configure failed: %v", err)
	}

	if s.Gap != 0.8 {
		t.Errorf("expected gap 0.8, got %f", s.Gap)
	}

	if s.MinCommentLength != 30 {
		t.Errorf("expected min length 30, got %d", s.MinCommentLength)
	}

	if s.commitsByTick == nil {
		t.Error("expected commitsByTick")
	}

	// Test validation logic.
	s.Gap = 2.0
	s.MinCommentLength = 5
	s.validate()

	if s.Gap != DefaultCommentSentimentGap {
		t.Error("expected default gap after validation")
	}

	if s.MinCommentLength != DefaultCommentSentimentCommentMinLength {
		t.Error("expected default min length after validation")
	}
}

func TestHistoryAnalyzer_Initialize(t *testing.T) {
	t.Parallel()

	s := &HistoryAnalyzer{}

	err := s.Initialize(nil)
	if err != nil {
		t.Fatalf("Initialize failed: %v", err)
	}

	if s.commentsByCommit == nil {
		t.Error("expected commentsByCommit initialized")
	}
}

func TestHistoryAnalyzer_Consume(t *testing.T) {
	t.Parallel()

	s := &HistoryAnalyzer{
		UAST:             &plumbing.UASTChangesAnalyzer{},
		Ticks:            &plumbing.TicksSinceStart{},
		MinCommentLength: 10,
	}
	require.NoError(t, s.Initialize(nil))

	// Construct UAST with comment.
	commentNode := &node.Node{
		Type:  node.UASTComment,
		Token: "This is a good comment",
		Pos:   &node.Positions{StartLine: 1, EndLine: 1},
	}

	changes := []uast.Change{
		{
			After: commentNode,
		},
	}
	s.UAST.SetChangesForTest(changes)
	s.Ticks.Tick = 0

	hash1 := gitlib.NewHash("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
	commit1 := gitlib.NewTestCommit(hash1, gitlib.TestSignature("dev", "dev@test.com"), "test")

	err := s.Consume(context.Background(), &analyze.Context{Commit: commit1})
	if err != nil {
		t.Fatalf("Consume failed: %v", err)
	}

	comments := s.commentsByCommit[hash1.String()]
	if len(comments) != 1 {
		t.Errorf("expected 1 comment, got %d", len(comments))
	}

	if comments[0] != "This is a good comment" {
		t.Errorf("expected comment content, got %s", comments[0])
	}

	// Filter logic.
	shortCommentNode := &node.Node{
		Type:  node.UASTComment,
		Token: "bad",
		Pos:   &node.Positions{StartLine: 2, EndLine: 2},
	}
	s.UAST.SetChangesForTest([]uast.Change{{After: shortCommentNode}})
	s.Ticks.Tick = 1

	hash2 := gitlib.NewHash("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")
	commit2 := gitlib.NewTestCommit(hash2, gitlib.TestSignature("dev", "dev@test.com"), "test2")

	require.NoError(t, s.Consume(context.Background(), &analyze.Context{Commit: commit2}))

	if len(s.commentsByCommit[hash2.String()]) != 0 {
		t.Error("expected short comment filtered out")
	}
}

func TestHistoryAnalyzer_Consume_ChildComments(t *testing.T) {
	t.Parallel()

	s := &HistoryAnalyzer{
		UAST:             &plumbing.UASTChangesAnalyzer{},
		Ticks:            &plumbing.TicksSinceStart{},
		MinCommentLength: 10,
	}
	require.NoError(t, s.Initialize(nil))

	root := &node.Node{Type: "Block"}
	child := &node.Node{
		Type:  node.UASTComment,
		Token: "Child comment 123",
		Pos:   &node.Positions{StartLine: 2, EndLine: 2},
	}
	root.Children = []*node.Node{child}

	s.UAST.SetChangesForTest([]uast.Change{{After: root}})

	hash := gitlib.NewHash("cccccccccccccccccccccccccccccccccccccccc")
	commit := gitlib.NewTestCommit(hash, gitlib.TestSignature("dev", "dev@test.com"), "test")

	require.NoError(t, s.Consume(context.Background(), &analyze.Context{Commit: commit}))

	if len(s.commentsByCommit[hash.String()]) != 1 {
		t.Errorf("expected child comment extracted, got %d", len(s.commentsByCommit[hash.String()]))
	}
}

func TestHistoryAnalyzer_Consume_MergeLines(t *testing.T) {
	t.Parallel()

	s := &HistoryAnalyzer{
		UAST:             &plumbing.UASTChangesAnalyzer{},
		Ticks:            &plumbing.TicksSinceStart{},
		MinCommentLength: 10,
	}
	require.NoError(t, s.Initialize(nil))

	// Multi-line comment (consecutive lines).
	c1 := &node.Node{Type: node.UASTComment, Token: "Line 1 is good", Pos: &node.Positions{StartLine: 1, EndLine: 1}}
	c2 := &node.Node{Type: node.UASTComment, Token: "Line 2 is nice", Pos: &node.Positions{StartLine: 2, EndLine: 2}}

	root := &node.Node{Type: "Block", Children: []*node.Node{c1, c2}}
	s.UAST.SetChangesForTest([]uast.Change{{After: root}})

	hash := gitlib.NewHash("dddddddddddddddddddddddddddddddddddddd")
	commit := gitlib.NewTestCommit(hash, gitlib.TestSignature("dev", "dev@test.com"), "test")

	require.NoError(t, s.Consume(context.Background(), &analyze.Context{Commit: commit}))

	comments := s.commentsByCommit[hash.String()]
	if len(comments) != 1 {
		t.Fatalf("expected 1 merged comment, got %d", len(comments))
	}

	if comments[0] != "Line 1 is good\nLine 2 is nice" &&
		comments[0] != "Line 1 is good Line 2 is nice" {
		t.Errorf("expected merged content, got %q", comments[0])
	}
}

func TestHistoryAnalyzer_Finalize(t *testing.T) {
	t.Parallel()

	s := &HistoryAnalyzer{}
	require.NoError(t, s.Initialize(nil))
	s.commentsByCommit["aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"] = []string{"Good"}

	report, err := s.Finalize()
	if err != nil {
		t.Fatalf("Finalize failed: %v", err)
	}

	cbc, ok := report["comments_by_commit"].(map[string][]string)
	require.True(t, ok, "type assertion failed for comments_by_commit")
	require.Len(t, cbc, 1)
}

func TestHistoryAnalyzer_Serialize_JSON(t *testing.T) {
	t.Parallel()

	s := &HistoryAnalyzer{}

	report := analyze.Report{
		"emotions_by_tick": map[int]float32{0: 0.5, 1: 0.8},
		"comments_by_tick": map[int][]string{0: {"Comment"}},
		"commits_by_tick":  map[int][]gitlib.Hash{0: {gitlib.NewHash("c1")}},
	}

	var buf bytes.Buffer

	err := s.Serialize(report, analyze.FormatJSON, &buf)
	require.NoError(t, err)

	var result map[string]any

	err = json.Unmarshal(buf.Bytes(), &result)
	require.NoError(t, err)

	// Verify metrics structure.
	assert.Contains(t, result, "time_series")
	assert.Contains(t, result, "trend")
	assert.Contains(t, result, "low_sentiment_periods")
	assert.Contains(t, result, "aggregate")
}

func TestHistoryAnalyzer_Serialize_YAML(t *testing.T) {
	t.Parallel()

	s := &HistoryAnalyzer{}

	report := analyze.Report{
		"emotions_by_tick": map[int]float32{0: 0.5, 1: 0.8},
		"comments_by_tick": map[int][]string{0: {"Comment"}},
		"commits_by_tick":  map[int][]gitlib.Hash{0: {gitlib.NewHash("c1")}},
	}

	var buf bytes.Buffer

	err := s.Serialize(report, analyze.FormatYAML, &buf)
	require.NoError(t, err)

	output := buf.String()
	assert.Contains(t, output, "time_series:")
	assert.Contains(t, output, "trend:")
	assert.Contains(t, output, "aggregate:")
}

func TestHistoryAnalyzer_Serialize_Default(t *testing.T) {
	t.Parallel()

	s := &HistoryAnalyzer{}

	report := analyze.Report{
		"emotions_by_tick": map[int]float32{0: 0.5},
		"comments_by_tick": map[int][]string{0: {"Comment"}},
		"commits_by_tick":  map[int][]gitlib.Hash{0: {gitlib.NewHash("c1")}},
	}

	// Unsupported format should return validation error.
	var buf bytes.Buffer

	err := s.Serialize(report, "unknown", &buf)
	require.ErrorIs(t, err, analyze.ErrUnsupportedFormat)
}

func TestHistoryAnalyzer_Misc(t *testing.T) {
	t.Parallel()

	s := &HistoryAnalyzer{}
	if s.Name() == "" {
		t.Error("Name empty")
	}

	if len(s.ListConfigurationOptions()) == 0 {
		t.Error("expected options")
	}

	clones := s.Fork(2)
	if len(clones) != 2 {
		t.Error("expected 2 clones")
	}
}

func TestFork_CreatesIndependentCopies(t *testing.T) {
	t.Parallel()

	s := &HistoryAnalyzer{
		MinCommentLength: 20,
		Gap:              0.5,
	}
	require.NoError(t, s.Initialize(nil))

	// Add some state to original.
	s.commentsByCommit["aaa"] = []string{"original comment"}

	forks := s.Fork(2)
	require.Len(t, forks, 2)

	fork1, ok := forks[0].(*HistoryAnalyzer)
	require.True(t, ok)
	fork2, ok := forks[1].(*HistoryAnalyzer)
	require.True(t, ok)

	// Forks should have empty independent maps (not inherit parent state).
	require.Empty(t, fork1.commentsByCommit, "fork should have empty commentsByCommit map")
	require.Empty(t, fork2.commentsByCommit, "fork should have empty commentsByCommit map")

	// Modifying one fork should not affect the other.
	fork1.commentsByCommit["bbb"] = []string{"fork1 comment"}

	require.Len(t, fork1.commentsByCommit, 1)
	require.Empty(t, fork2.commentsByCommit, "fork2 should not see fork1's changes")
}

func TestFork_SharesConfig(t *testing.T) {
	t.Parallel()

	s := &HistoryAnalyzer{
		MinCommentLength: 25,
		Gap:              0.7,
	}
	require.NoError(t, s.Initialize(nil))

	forks := s.Fork(2)
	fork1, ok := forks[0].(*HistoryAnalyzer)
	require.True(t, ok)

	// Config should be shared.
	require.Equal(t, s.MinCommentLength, fork1.MinCommentLength)
	require.InDelta(t, s.Gap, fork1.Gap, 0.001)
}

func TestMerge_CombinesCommentsByCommit(t *testing.T) {
	t.Parallel()

	s := &HistoryAnalyzer{}
	require.NoError(t, s.Initialize(nil))

	// Original has comments for commit aaa.
	s.commentsByCommit["aaa"] = []string{"original comment"}

	// Create a branch with comments for a different commit.
	branch := &HistoryAnalyzer{}
	require.NoError(t, branch.Initialize(nil))
	branch.commentsByCommit["bbb"] = []string{"branch comment"}

	s.Merge([]analyze.HistoryAnalyzer{branch})

	// Should have both commits.
	require.Len(t, s.commentsByCommit, 2)
	require.Len(t, s.commentsByCommit["aaa"], 1)
	require.Len(t, s.commentsByCommit["bbb"], 1)
	require.Equal(t, "original comment", s.commentsByCommit["aaa"][0])
	require.Equal(t, "branch comment", s.commentsByCommit["bbb"][0])
}

func TestMerge_DistinctCommits(t *testing.T) {
	t.Parallel()

	s := &HistoryAnalyzer{}
	require.NoError(t, s.Initialize(nil))

	s.commentsByCommit["aaa"] = []string{"comment 1"}

	// Branch has a different commit.
	branch := &HistoryAnalyzer{}
	require.NoError(t, branch.Initialize(nil))
	branch.commentsByCommit["bbb"] = []string{"comment 2", "comment 3"}

	s.Merge([]analyze.HistoryAnalyzer{branch})

	require.Len(t, s.commentsByCommit, 2)
	require.Len(t, s.commentsByCommit["aaa"], 1)
	require.Len(t, s.commentsByCommit["bbb"], 2)
}

func TestForkMerge_RoundTrip(t *testing.T) {
	t.Parallel()

	s := &HistoryAnalyzer{
		MinCommentLength: 20,
		Gap:              0.5,
	}
	require.NoError(t, s.Initialize(nil))

	// Fork.
	forks := s.Fork(2)
	fork1, ok := forks[0].(*HistoryAnalyzer)
	require.True(t, ok)
	fork2, ok := forks[1].(*HistoryAnalyzer)
	require.True(t, ok)

	// Each fork adds different commits (forks process distinct commits).
	fork1.commentsByCommit["aaa"] = []string{"fork1 commit aaa comment"}
	fork1.commentsByCommit["bbb"] = []string{"fork1 commit bbb comment"}
	fork2.commentsByCommit["ccc"] = []string{"fork2 commit ccc comment"}
	fork2.commentsByCommit["ddd"] = []string{"fork2 commit ddd comment"}

	// Merge.
	s.Merge(forks)

	// Verify all commits are merged.
	require.Len(t, s.commentsByCommit, 4)
	require.Len(t, s.commentsByCommit["aaa"], 1)
	require.Len(t, s.commentsByCommit["bbb"], 1)
	require.Len(t, s.commentsByCommit["ccc"], 1)
	require.Len(t, s.commentsByCommit["ddd"], 1)
}

func TestHistoryAnalyzer_Consume_StoresPerCommitComments(t *testing.T) {
	t.Parallel()

	s := &HistoryAnalyzer{
		UAST:             &plumbing.UASTChangesAnalyzer{},
		Ticks:            &plumbing.TicksSinceStart{},
		MinCommentLength: 10,
	}
	require.NoError(t, s.Initialize(nil))

	commentNode := &node.Node{
		Type:  node.UASTComment,
		Token: "This is a good comment for commit",
		Pos:   &node.Positions{StartLine: 1, EndLine: 1},
	}
	s.UAST.SetChangesForTest([]uast.Change{{After: commentNode}})
	s.Ticks.Tick = 0

	hash := gitlib.NewHash("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
	commit := gitlib.NewTestCommit(hash, gitlib.TestSignature("dev", "dev@test.com"), "test")

	err := s.Consume(context.Background(), &analyze.Context{Commit: commit})
	require.NoError(t, err)

	// Per-commit comments should exist.
	require.Len(t, s.commentsByCommit, 1)
	assert.Contains(t, s.commentsByCommit, hash.String())
	assert.Len(t, s.commentsByCommit[hash.String()], 1)
}

func TestHistoryAnalyzer_Finalize_IncludesCommitComments(t *testing.T) {
	t.Parallel()

	s := &HistoryAnalyzer{}
	require.NoError(t, s.Initialize(nil))

	hashStr := "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
	s.commentsByCommit[hashStr] = []string{"test comment data"}

	report, err := s.Finalize()
	require.NoError(t, err)

	cbc, ok := report["comments_by_commit"].(map[string][]string)
	require.True(t, ok, "report must contain comments_by_commit")
	assert.Len(t, cbc, 1)
	assert.Contains(t, cbc, hashStr)
}

func TestHistoryAnalyzer_Merge_CombinesCommitComments(t *testing.T) {
	t.Parallel()

	s := &HistoryAnalyzer{}
	require.NoError(t, s.Initialize(nil))

	fork1 := &HistoryAnalyzer{}
	require.NoError(t, fork1.Initialize(nil))
	fork1.commentsByCommit["aaa"] = []string{"comment a"}

	fork2 := &HistoryAnalyzer{}
	require.NoError(t, fork2.Initialize(nil))
	fork2.commentsByCommit["bbb"] = []string{"comment b"}

	s.Merge([]analyze.HistoryAnalyzer{fork1, fork2})

	assert.Len(t, s.commentsByCommit, 2)
	assert.Contains(t, s.commentsByCommit, "aaa")
	assert.Contains(t, s.commentsByCommit, "bbb")
}

func TestRegisterTickExtractor_Sentiment(t *testing.T) { //nolint:paralleltest // writes to global map
	report := analyze.Report{
		"emotions_by_tick": map[int]float32{0: 0.7},
		"comments_by_commit": map[string][]string{
			"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa": {"nice work on this function", "great improvement"},
			"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb": {"this is ugly code"},
		},
	}

	result := extractCommitTimeSeries(report)
	require.Len(t, result, 2)

	statsA, ok := result["aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"]
	require.True(t, ok)

	statsMap, ok := statsA.(map[string]any)
	require.True(t, ok)
	assert.Equal(t, 2, statsMap["comment_count"])
	assert.Contains(t, statsMap, "sentiment")
}
