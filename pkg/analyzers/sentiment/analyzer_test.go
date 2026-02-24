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

func TestAnalyzer_Configure(t *testing.T) {
	t.Parallel()

	s := NewAnalyzer()
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

func TestAnalyzer_Initialize(t *testing.T) {
	t.Parallel()

	s := NewAnalyzer()

	err := s.Initialize(nil)
	require.NoError(t, err)
}

func TestAnalyzer_Consume_ReturnsTCWithComments(t *testing.T) {
	t.Parallel()

	s := &Analyzer{
		UAST:             &plumbing.UASTChangesAnalyzer{},
		Ticks:            &plumbing.TicksSinceStart{},
		MinCommentLength: 10,
	}
	require.NoError(t, s.Initialize(nil))

	commentNode := &node.Node{
		Type:  node.UASTComment,
		Token: "This is a good comment",
		Pos:   &node.Positions{StartLine: 1, EndLine: 1},
	}
	s.UAST.SetChangesForTest([]uast.Change{{After: commentNode}})
	s.Ticks.Tick = 0

	hash1 := gitlib.NewHash("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
	commit1 := gitlib.NewTestCommit(hash1, gitlib.TestSignature("dev", "dev@test.com"), "test")

	tc, err := s.Consume(context.Background(), &analyze.Context{Commit: commit1})
	require.NoError(t, err)

	// TC should contain commit hash and comment data.
	assert.Equal(t, hash1, tc.CommitHash)

	cr, ok := tc.Data.(*CommitResult)
	require.True(t, ok, "TC.Data should be *CommitResult")
	require.Len(t, cr.Comments, 1)
	assert.Equal(t, "This is a good comment", cr.Comments[0])
}

func TestAnalyzer_Consume_FiltersShortComments(t *testing.T) {
	t.Parallel()

	s := &Analyzer{
		UAST:             &plumbing.UASTChangesAnalyzer{},
		Ticks:            &plumbing.TicksSinceStart{},
		MinCommentLength: 10,
	}
	require.NoError(t, s.Initialize(nil))

	shortCommentNode := &node.Node{
		Type:  node.UASTComment,
		Token: "bad",
		Pos:   &node.Positions{StartLine: 2, EndLine: 2},
	}
	s.UAST.SetChangesForTest([]uast.Change{{After: shortCommentNode}})
	s.Ticks.Tick = 1

	hash2 := gitlib.NewHash("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")
	commit2 := gitlib.NewTestCommit(hash2, gitlib.TestSignature("dev", "dev@test.com"), "test2")

	tc, err := s.Consume(context.Background(), &analyze.Context{Commit: commit2})
	require.NoError(t, err)

	cr, ok := tc.Data.(*CommitResult)
	require.True(t, ok)
	assert.Empty(t, cr.Comments, "short comment should be filtered out")
}

func TestAnalyzer_Consume_ChildComments(t *testing.T) {
	t.Parallel()

	s := &Analyzer{
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

	tc, err := s.Consume(context.Background(), &analyze.Context{Commit: commit})
	require.NoError(t, err)

	cr, ok := tc.Data.(*CommitResult)
	require.True(t, ok)
	require.Len(t, cr.Comments, 1, "child comment should be extracted")
}

func TestAnalyzer_Consume_MergeLines(t *testing.T) {
	t.Parallel()

	s := &Analyzer{
		UAST:             &plumbing.UASTChangesAnalyzer{},
		Ticks:            &plumbing.TicksSinceStart{},
		MinCommentLength: 10,
	}
	require.NoError(t, s.Initialize(nil))

	c1 := &node.Node{Type: node.UASTComment, Token: "Line 1 is good", Pos: &node.Positions{StartLine: 1, EndLine: 1}}
	c2 := &node.Node{Type: node.UASTComment, Token: "Line 2 is nice", Pos: &node.Positions{StartLine: 2, EndLine: 2}}

	root := &node.Node{Type: "Block", Children: []*node.Node{c1, c2}}
	s.UAST.SetChangesForTest([]uast.Change{{After: root}})

	hash := gitlib.NewHash("dddddddddddddddddddddddddddddddddddddd")
	commit := gitlib.NewTestCommit(hash, gitlib.TestSignature("dev", "dev@test.com"), "test")

	tc, err := s.Consume(context.Background(), &analyze.Context{Commit: commit})
	require.NoError(t, err)

	cr, ok := tc.Data.(*CommitResult)
	require.True(t, ok)
	require.Len(t, cr.Comments, 1, "adjacent comments should be merged")

	assert.Contains(t, cr.Comments[0], "Line 1 is good")
	assert.Contains(t, cr.Comments[0], "Line 2 is nice")
}

func TestAnalyzer_Consume_NoAccumulation(t *testing.T) {
	t.Parallel()

	s := &Analyzer{
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

	hash := gitlib.NewHash("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
	commit := gitlib.NewTestCommit(hash, gitlib.TestSignature("dev", "dev@test.com"), "test")

	tc, err := s.Consume(context.Background(), &analyze.Context{Commit: commit})
	require.NoError(t, err)

	cr, ok := tc.Data.(*CommitResult)
	require.True(t, ok)
	assert.Len(t, cr.Comments, 1, "TC should contain the comment")

	// Analyzer should have no accumulated internal state.
}

func TestAnalyzer_Serialize_JSON(t *testing.T) {
	t.Parallel()

	s := NewAnalyzer()

	report := analyze.Report{
		"comments_by_commit": map[string][]string{"c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1": {"Comment"}},
		"commits_by_tick":    map[int][]gitlib.Hash{0: {gitlib.NewHash("c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1")}},
	}

	var buf bytes.Buffer

	err := s.Serialize(report, analyze.FormatJSON, &buf)
	require.NoError(t, err)

	var result map[string]any

	err = json.Unmarshal(buf.Bytes(), &result)
	require.NoError(t, err)

	assert.Contains(t, result, "time_series")
	assert.Contains(t, result, "trend")
	assert.Contains(t, result, "low_sentiment_periods")
	assert.Contains(t, result, "aggregate")
}

func TestAnalyzer_Serialize_YAML(t *testing.T) {
	t.Parallel()

	s := NewAnalyzer()

	report := analyze.Report{
		"comments_by_commit": map[string][]string{"c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1": {"Comment"}},
		"commits_by_tick":    map[int][]gitlib.Hash{0: {gitlib.NewHash("c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1")}},
	}

	var buf bytes.Buffer

	err := s.Serialize(report, analyze.FormatYAML, &buf)
	require.NoError(t, err)

	output := buf.String()
	assert.Contains(t, output, "time_series:")
	assert.Contains(t, output, "trend:")
	assert.Contains(t, output, "aggregate:")
}

func TestAnalyzer_Serialize_UnsupportedFormat(t *testing.T) {
	t.Parallel()

	s := NewAnalyzer()

	report := analyze.Report{
		"comments_by_commit": map[string][]string{"c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1": {"Comment"}},
		"commits_by_tick":    map[int][]gitlib.Hash{0: {gitlib.NewHash("c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1")}},
	}

	var buf bytes.Buffer

	err := s.Serialize(report, "unknown", &buf)
	require.ErrorIs(t, err, analyze.ErrUnsupportedFormat)
}

func TestAnalyzer_Misc(t *testing.T) {
	t.Parallel()

	s := NewAnalyzer()
	assert.NotEmpty(t, s.Name())
	assert.NotEmpty(t, s.ListConfigurationOptions())

	clones := s.Fork(2)
	assert.Len(t, clones, 2)
}

func TestFork_CreatesIndependentCopies(t *testing.T) {
	t.Parallel()

	s := &Analyzer{
		MinCommentLength: 20,
		Gap:              0.5,
	}
	require.NoError(t, s.Initialize(nil))

	forks := s.Fork(2)
	require.Len(t, forks, 2)

	fork1, ok := forks[0].(*Analyzer)
	require.True(t, ok)
	fork2, ok := forks[1].(*Analyzer)
	require.True(t, ok)

	// Forks should have independent plumbing state.
	assert.NotSame(t, fork1.UAST, fork2.UAST)
	assert.NotSame(t, fork1.Ticks, fork2.Ticks)
}

func TestFork_SharesConfig(t *testing.T) {
	t.Parallel()

	s := &Analyzer{
		MinCommentLength: 25,
		Gap:              0.7,
	}
	require.NoError(t, s.Initialize(nil))

	forks := s.Fork(2)
	fork1, ok := forks[0].(*Analyzer)
	require.True(t, ok)

	require.Equal(t, s.MinCommentLength, fork1.MinCommentLength)
	require.InDelta(t, s.Gap, fork1.Gap, 0.001)
}

func TestMerge_IsNoOp(t *testing.T) {
	t.Parallel()

	s := NewAnalyzer()
	require.NoError(t, s.Initialize(nil))

	branch := &Analyzer{}
	require.NoError(t, branch.Initialize(nil))

	// Merge should not panic.
	s.Merge([]analyze.HistoryAnalyzer{branch})
}

func TestForkMerge_RoundTrip(t *testing.T) {
	t.Parallel()

	s := &Analyzer{
		UAST:             &plumbing.UASTChangesAnalyzer{},
		Ticks:            &plumbing.TicksSinceStart{},
		MinCommentLength: 20,
		Gap:              0.5,
	}
	require.NoError(t, s.Initialize(nil))

	forks := s.Fork(2)
	require.Len(t, forks, 2)

	// Merge is a no-op since TCs carry the data now.
	s.Merge(forks)
}

func TestSerializeTICKs_JSON(t *testing.T) {
	t.Parallel()

	s := NewAnalyzer()
	c1 := "c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1"
	s.commitsByTick = map[int][]gitlib.Hash{
		0: {gitlib.NewHash(c1)},
	}

	ticks := []analyze.TICK{
		{
			Tick: 0,
			Data: &TickData{
				CommentsByCommit: map[string][]string{c1: {"good work here"}},
			},
		},
	}

	var buf bytes.Buffer

	err := s.SerializeTICKs(ticks, analyze.FormatJSON, &buf)
	require.NoError(t, err)

	var result map[string]any

	err = json.Unmarshal(buf.Bytes(), &result)
	require.NoError(t, err)

	assert.Contains(t, result, "time_series")
	assert.Contains(t, result, "aggregate")
}

func TestSerializeTICKs_YAML(t *testing.T) {
	t.Parallel()

	s := NewAnalyzer()
	c1 := "c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1"
	s.commitsByTick = map[int][]gitlib.Hash{
		0: {gitlib.NewHash(c1)},
	}

	ticks := []analyze.TICK{
		{
			Tick: 0,
			Data: &TickData{
				CommentsByCommit: map[string][]string{c1: {"good work here"}},
			},
		},
	}

	var buf bytes.Buffer

	err := s.SerializeTICKs(ticks, analyze.FormatYAML, &buf)
	require.NoError(t, err)

	output := buf.String()
	assert.Contains(t, output, "time_series:")
	assert.Contains(t, output, "aggregate:")
}

func TestSerializeTICKs_UnsupportedFormat(t *testing.T) {
	t.Parallel()

	s := NewAnalyzer()

	var buf bytes.Buffer

	err := s.SerializeTICKs(nil, "unknown", &buf)
	require.ErrorIs(t, err, analyze.ErrUnsupportedFormat)
}

func TestFilterComments_BasicFiltering(t *testing.T) {
	t.Parallel()

	const minLen = 10

	t.Run("english_accepted", func(t *testing.T) {
		t.Parallel()

		r := filterComments([]string{"This function handles validation correctly"}, minLen)
		assert.Len(t, r, 1)
	})

	t.Run("short_filtered", func(t *testing.T) {
		t.Parallel()

		r := filterComments([]string{"bad"}, minLen)
		assert.Empty(t, r)
	})

	t.Run("license_filtered", func(t *testing.T) {
		t.Parallel()

		r := filterComments([]string{"Copyright 2024 Acme Corp Licensed under MIT"}, minLen)
		assert.Empty(t, r)
	})
}

func TestFilterComments_MultilingualSupport(t *testing.T) {
	t.Parallel()

	const minLen = 10

	comments := map[string]string{
		"chinese":  "\u8fd9\u4e2a\u51fd\u6570\u5904\u7406\u8f93\u5165\u9a8c\u8bc1\u903b\u8f91",
		"japanese": "\u3053\u306e\u95a2\u6570\u306f\u5165\u529b\u3092\u51e6\u7406\u3057\u3066\u6b63\u3057\u3044",
		"korean":   "\uc774 \ud568\uc218\ub294 \uc785\ub825 \uc720\ud6a8\uc131 \uac80\uc0ac\ub97c \ucc98\ub9ac",
		"cyrillic": "Эта функция",
		"arabic":   "\u0647\u0630\u0647 \u0627\u0644\u062f\u0627\u0644\u0629 \u062a\u062a\u0639\u0627\u0645\u0644 \u0645\u0639",
	}

	for lang, comment := range comments {
		t.Run(lang, func(t *testing.T) {
			t.Parallel()

			r := filterComments([]string{comment}, minLen)
			assert.Len(t, r, 1, "%s should be included", lang)
		})
	}
}

//nolint:misspell // testing UK spelling detection
func TestFilterComments_LicenceUKSpelling(t *testing.T) {
	t.Parallel()

	const minLen = 10

	comment := "This code is under the Licence agreement terms"
	r := filterComments([]string{comment}, minLen)
	assert.Empty(t, r, "UK licence should be filtered")
}

func TestLicenseRegex(t *testing.T) {
	t.Parallel()

	t.Run("license_us", func(t *testing.T) {
		t.Parallel()

		assert.True(t, licenseRE.MatchString("Licensed under MIT License"))
	})

	t.Run("copyright", func(t *testing.T) {
		t.Parallel()

		assert.True(t, licenseRE.MatchString("Copyright 2024 Acme Corp"))
	})

	t.Run("copyright_symbol", func(t *testing.T) {
		t.Parallel()

		assert.True(t, licenseRE.MatchString("\u00a9 2024 All rights reserved"))
	})

	t.Run("no_match", func(t *testing.T) {
		t.Parallel()

		assert.False(t, licenseRE.MatchString("This function processes data"))
	})
}

//nolint:misspell // testing UK spelling detection
func TestLicenseRegex_UKSpelling(t *testing.T) {
	t.Parallel()

	assert.True(t, licenseRE.MatchString("This Licence covers all usage"))
}

func TestStripCommentDelimiters(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name     string
		input    string
		expected string
	}{
		{"go_single_line", "// This is a comment", "This is a comment"},
		{"go_multiline_open", "/* Block comment */", "Block comment"},
		{"python_hash", "# Python comment", "Python comment"},
		{"triple_slash", "/// Doc comment", "Doc comment"},
		{"rust_doc", "//! Module doc", "Module doc"},
		{"sql_dash", "-- SQL comment", "SQL comment"},
		{"semicolon", "; Lisp comment", "Lisp comment"},
		{"no_prefix", "Normal text", "Normal text"},
		{"multiline_go", "// Line 1\n// Line 2", "Line 1 Line 2"},
		{"empty", "", ""},
		{"only_prefix", "//", ""},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			result := stripCommentDelimiters(tt.input)
			assert.Equal(t, tt.expected, result)
		})
	}
}

func TestExtractCommitTimeSeries_Sentiment(t *testing.T) {
	t.Parallel()

	s := NewAnalyzer()

	report := analyze.Report{
		"comments_by_commit": map[string][]string{
			"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa": {"nice work on this function", "great improvement"},
			"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb": {"this is ugly code"},
		},
	}

	result := s.ExtractCommitTimeSeries(report)
	require.Len(t, result, 2)

	statsA, ok := result["aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"]
	require.True(t, ok)

	statsMap, ok := statsA.(map[string]any)
	require.True(t, ok)
	assert.Equal(t, 2, statsMap["comment_count"])
	assert.Contains(t, statsMap, "sentiment")
}
