package sentiment //nolint:testpackage // testing internal implementation.

import (
	"bytes"
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

	if s.commentsByTick == nil {
		t.Error("expected commentsByTick initialized")
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

	err := s.Consume(&analyze.Context{})
	if err != nil {
		t.Fatalf("Consume failed: %v", err)
	}

	comments := s.commentsByTick[0]
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

	require.NoError(t, s.Consume(&analyze.Context{}))

	if len(s.commentsByTick[1]) != 0 {
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
		Token: "Child comment 123",                       // Longer to pass filters.
		Pos:   &node.Positions{StartLine: 2, EndLine: 2}, // Different pos from root? Root has nil pos?
	}
	root.Children = []*node.Node{child}

	s.UAST.SetChangesForTest([]uast.Change{{After: root}})

	require.NoError(t, s.Consume(&analyze.Context{}))

	if len(s.commentsByTick[0]) != 1 {
		// Debug if not extracted
		// extractComments recursive on children?
		// History.go: extractComments calls on children.
		// Node.UASTComment type check on root.
		// Root is "Block" -> recursion.
		// Child is "Comment" -> extracted.
		// MergeComments processes extracted.
		// Token "Child comment 123" -> 17 chars > 5.
		// Ratio: 14/17 = 0.82 > 0.6.
		// Should work.
		t.Errorf("expected child comment extracted, got %d", len(s.commentsByTick[0]))
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

	require.NoError(t, s.Consume(&analyze.Context{}))

	comments := s.commentsByTick[0]
	if len(comments) != 1 {
		t.Fatalf("expected 1 merged comment, got %d", len(comments))
	}

	if comments[0] != "Line 1 is good\nLine 2 is nice" {
		// Newlines might be replaced by spaces in filtered output?
		// WhitespaceRE.ReplaceAllString(comment, " ") replaces newlines with space.
		// "Line 1 is good\nLine 2 is nice" -> "Line 1 is good Line 2 is nice".
		if comments[0] == "Line 1 is good Line 2 is nice" { //nolint:revive // empty block is intentional.
			// Accept.
		} else {
			t.Errorf("expected merged content, got %q", comments[0])
		}
	}
}

func TestHistoryAnalyzer_Finalize(t *testing.T) {
	t.Parallel()

	s := &HistoryAnalyzer{}
	require.NoError(t, s.Initialize(nil))
	s.commentsByTick[0] = []string{"Good"}

	report, err := s.Finalize()
	if err != nil {
		t.Fatalf("Finalize failed: %v", err)
	}

	emotions, ok := report["emotions_by_tick"].(map[int]float32)
	require.True(t, ok, "type assertion failed for emotions")

	if _, hasEmotion := emotions[0]; !hasEmotion {
		t.Error("expected emotions for tick 0")
	}
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

	// Verify metrics structure
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

	// Add some state to original
	s.commentsByTick[0] = []string{"original comment"}

	forks := s.Fork(2)
	require.Len(t, forks, 2)

	fork1, ok := forks[0].(*HistoryAnalyzer)
	require.True(t, ok)
	fork2, ok := forks[1].(*HistoryAnalyzer)
	require.True(t, ok)

	// Forks should have empty independent maps (not inherit parent state)
	require.Empty(t, fork1.commentsByTick, "fork should have empty commentsByTick map")
	require.Empty(t, fork2.commentsByTick, "fork should have empty commentsByTick map")

	// Modifying one fork should not affect the other
	fork1.commentsByTick[1] = []string{"fork1 comment"}

	require.Len(t, fork1.commentsByTick, 1)
	require.Empty(t, fork2.commentsByTick, "fork2 should not see fork1's changes")
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

	// Config should be shared
	require.Equal(t, s.MinCommentLength, fork1.MinCommentLength)
	require.InDelta(t, s.Gap, fork1.Gap, 0.001)
}

func TestMerge_CombinesCommentsByTick(t *testing.T) {
	t.Parallel()

	s := &HistoryAnalyzer{}
	require.NoError(t, s.Initialize(nil))

	// Original has comments at tick 0
	s.commentsByTick[0] = []string{"original comment"}

	// Create a branch with comments at different tick
	branch := &HistoryAnalyzer{}
	require.NoError(t, branch.Initialize(nil))
	branch.commentsByTick[1] = []string{"branch comment"}

	s.Merge([]analyze.HistoryAnalyzer{branch})

	// Should have both ticks
	require.Len(t, s.commentsByTick, 2)
	require.Len(t, s.commentsByTick[0], 1)
	require.Len(t, s.commentsByTick[1], 1)
	require.Equal(t, "original comment", s.commentsByTick[0][0])
	require.Equal(t, "branch comment", s.commentsByTick[1][0])
}

func TestMerge_AppendsCommentsAtSameTick(t *testing.T) {
	t.Parallel()

	s := &HistoryAnalyzer{}
	require.NoError(t, s.Initialize(nil))

	// Original has comments at tick 0
	s.commentsByTick[0] = []string{"comment 1"}

	// Branch also has comments at tick 0
	branch := &HistoryAnalyzer{}
	require.NoError(t, branch.Initialize(nil))
	branch.commentsByTick[0] = []string{"comment 2", "comment 3"}

	s.Merge([]analyze.HistoryAnalyzer{branch})

	// Should have all comments at tick 0
	require.Len(t, s.commentsByTick[0], 3)
	require.Contains(t, s.commentsByTick[0], "comment 1")
	require.Contains(t, s.commentsByTick[0], "comment 2")
	require.Contains(t, s.commentsByTick[0], "comment 3")
}

func TestForkMerge_RoundTrip(t *testing.T) {
	t.Parallel()

	s := &HistoryAnalyzer{
		MinCommentLength: 20,
		Gap:              0.5,
	}
	require.NoError(t, s.Initialize(nil))

	// Fork
	forks := s.Fork(2)
	fork1, ok := forks[0].(*HistoryAnalyzer)
	require.True(t, ok)
	fork2, ok := forks[1].(*HistoryAnalyzer)
	require.True(t, ok)

	// Each fork adds different comments
	fork1.commentsByTick[0] = []string{"fork1 tick0 comment"}
	fork1.commentsByTick[1] = []string{"fork1 tick1 comment"}
	fork2.commentsByTick[0] = []string{"fork2 tick0 comment"}
	fork2.commentsByTick[2] = []string{"fork2 tick2 comment"}

	// Merge
	s.Merge(forks)

	// Verify all comments are merged
	require.Len(t, s.commentsByTick, 3)
	require.Len(t, s.commentsByTick[0], 2) // from both forks
	require.Len(t, s.commentsByTick[1], 1) // from fork1
	require.Len(t, s.commentsByTick[2], 1) // from fork2
}
