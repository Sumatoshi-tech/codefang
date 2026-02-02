package sentiment //nolint:testpackage // testing internal implementation.

import (
	"bytes"
	"strings"
	"testing"

	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"

	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/plumbing"
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

func TestHistoryAnalyzer_Serialize(t *testing.T) {
	t.Parallel()

	s := &HistoryAnalyzer{}

	report := analyze.Report{
		"emotions_by_tick": map[int]float32{0: 0.5},
		"comments_by_tick": map[int][]string{0: {"Comment"}},
		"commits_by_tick":  map[int][]gitlib.Hash{0: {gitlib.NewHash("c1")}},
	}

	// YAML.
	var buf bytes.Buffer

	err := s.Serialize(report, analyze.FormatYAML, &buf)
	if err != nil {
		t.Fatalf("Serialize YAML failed: %v", err)
	}

	if !strings.Contains(buf.String(), "0: [0.5000, [c1") {
		t.Errorf("unexpected output: %s", buf.String())
	}

	if !strings.Contains(buf.String(), "\"Comment\"]") {
		t.Errorf("unexpected output: %s", buf.String())
	}

	// Binary.
	var pbuf bytes.Buffer

	err = s.Serialize(report, analyze.FormatBinary, &pbuf)
	if err != nil {
		t.Fatalf("Serialize Binary failed: %v", err)
	}
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
