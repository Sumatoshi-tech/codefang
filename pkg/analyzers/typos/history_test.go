package typos //nolint:testpackage // testing internal implementation.

import (
	"bytes"
	"strings"
	"testing"

	"github.com/stretchr/testify/require"

	gitplumbing "github.com/go-git/go-git/v6/plumbing"
	"github.com/go-git/go-git/v6/plumbing/object"
	"github.com/sergi/go-diff/diffmatchpatch"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/plumbing"
	pkgplumbing "github.com/Sumatoshi-tech/codefang/pkg/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/uast"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

func TestHistoryAnalyzer_Configure(t *testing.T) {
	t.Parallel()

	a := &HistoryAnalyzer{}

	err := a.Configure(map[string]any{
		ConfigTyposDatasetMaximumAllowedDistance: 2,
	})
	if err != nil {
		t.Fatal(err)
	}

	if a.MaximumAllowedDistance != 2 {
		t.Errorf("expected max distance 2, got %d", a.MaximumAllowedDistance)
	}

	a = &HistoryAnalyzer{}
	require.NoError(t, a.Configure(nil))

	if a.MaximumAllowedDistance != DefaultMaximumAllowedTypoDistance {
		t.Error("expected default max distance")
	}
}

func TestHistoryAnalyzer_Initialize(t *testing.T) {
	t.Parallel()

	a := &HistoryAnalyzer{}

	err := a.Initialize(nil)
	if err != nil {
		t.Fatal(err)
	}

	if a.lcontext == nil {
		t.Error("expected lcontext initialized")
	}
}

func TestHistoryAnalyzer_Consume(t *testing.T) {
	t.Parallel()

	a := &HistoryAnalyzer{
		UASTChanges:            &plumbing.UASTChangesAnalyzer{},
		FileDiff:               &plumbing.FileDiffAnalyzer{},
		BlobCache:              &plumbing.BlobCacheAnalyzer{},
		MaximumAllowedDistance: 2,
	}
	require.NoError(t, a.Initialize(nil))

	// Setup Blobs.
	hashBefore := gitplumbing.NewHash("aaa")
	hashAfter := gitplumbing.NewHash("bbb")

	// Content:
	// Line 1: func test() {
	// Line 2:   var variabl = 1  (Typo)
	// Line 3: }
	//
	// New Content:
	// Line 1: func test() {
	// Line 2:   var variable = 1 (Fixed)
	// Line 3: }.

	contentBefore := "func test() {\n  var variabl = 1\n}\n"
	contentAfter := "func test() {\n  var variable = 1\n}\n"

	a.BlobCache.Cache = map[gitplumbing.Hash]*pkgplumbing.CachedBlob{
		hashBefore: {Data: []byte(contentBefore)},
		hashAfter:  {Data: []byte(contentAfter)},
	}

	// UAST
	// Identifier nodes needed on line 2 (0-indexed lines in blob split, code uses line-1 for array access).
	// In Consume:
	// linesBefore := bytes.Split(blobBefore.Data, []byte{'\n'})
	// Line := int(id.Pos.StartLine) - 1.
	// NodesBefore := removedIdentifiers[c.Before].
	// Field c.Before is index in linesBefore.

	// StartLine 2.

	rootBefore := &node.Node{Type: "File", Children: []*node.Node{
		{Type: node.UASTIdentifier, Token: "variabl", Pos: &node.Positions{StartLine: 2, EndLine: 2}},
	}}
	rootAfter := &node.Node{Type: "File", Children: []*node.Node{
		{Type: node.UASTIdentifier, Token: "variable", Pos: &node.Positions{StartLine: 2, EndLine: 2}},
	}}

	a.UASTChanges.Changes = []uast.Change{{
		Change: &object.Change{
			From: object.ChangeEntry{Name: "file.go", TreeEntry: object.TreeEntry{Hash: hashBefore}},
			To:   object.ChangeEntry{Name: "file.go", TreeEntry: object.TreeEntry{Hash: hashAfter}},
		},
		Before: rootBefore,
		After:  rootAfter,
	}}

	// FileDiff
	// Diffs should reflect line changes.
	// Line 1: Equal (1 line)
	// Line 2: Modify -> Delete 1 line, Insert 1 line
	// Line 3: Equal (1 line) (plus empty line after last \n?)

	// Bytes.Split("...}\n", \n) -> ["...}", ""] if ends with \n?
	// Go bytes.Split("a\n", "\n") -> ["a", ""].
	// ContentBefore has 3 lines + empty?
	// "func...\n" "var...\n" "}\n"
	// Split -> "func...", "var...", "}", "".

	// If I use runes as lines:
	// Equal: 1
	// Delete: 1
	// Insert: 1
	// Equal: 2 (} and empty).

	// Wait, Consume logic:
	// case diffmatchpatch.DiffDelete: lineNumBefore += size; removedSize = size
	// DiffInsert case: if size == removedSize, checks distance.

	// So I need Delete 1, then Insert 1.

	a.FileDiff.FileDiffs = map[string]pkgplumbing.FileDiffData{
		"file.go": {
			Diffs: []diffmatchpatch.Diff{
				{Type: diffmatchpatch.DiffEqual, Text: "\x00"},     // Line 1.
				{Type: diffmatchpatch.DiffDelete, Text: "\x01"},    // Line 2 Old.
				{Type: diffmatchpatch.DiffInsert, Text: "\x01"},    // Line 2 New.
				{Type: diffmatchpatch.DiffEqual, Text: "\x02\x03"}, // Line 3 and 4.
			},
		},
	}

	ctx := &analyze.Context{Commit: &object.Commit{Hash: gitplumbing.NewHash("commit")}}

	err := a.Consume(ctx)
	if err != nil {
		t.Fatal(err)
	}

	if len(a.typos) != 1 {
		t.Fatalf("expected 1 typo, got %d", len(a.typos))
	}

	typo := a.typos[0]
	if typo.Wrong != "variabl" || typo.Correct != "variable" {
		t.Errorf("unexpected typo: %v", typo)
	}
}

func TestHistoryAnalyzer_FinalizeAndSerialize(t *testing.T) {
	t.Parallel()

	a := &HistoryAnalyzer{}
	a.typos = []Typo{
		{Wrong: "a", Correct: "b", File: "f", Line: 1},
		{Wrong: "a", Correct: "b", File: "f", Line: 1}, // Duplicate.
		{Wrong: "x", Correct: "y", File: "f", Line: 2},
	}

	report, err := a.Finalize()
	if err != nil {
		t.Fatal(err)
	}

	typos, ok := report["typos"].([]Typo)
	require.True(t, ok, "type assertion failed for typos")

	if len(typos) != 2 {
		t.Errorf("expected 2 unique typos, got %d", len(typos))
	}

	// Serialize Text.
	var buf bytes.Buffer

	err = a.Serialize(report, false, &buf)
	if err != nil {
		t.Fatal(err)
	}

	if !strings.Contains(buf.String(), "wrong: a") {
		t.Error("expected output to contain wrong: a")
	}

	// Serialize Binary.
	var buf2 bytes.Buffer

	err = a.Serialize(report, true, &buf2)
	if err != nil {
		t.Fatal(err)
	}

	// FormatReport.
	var buf3 bytes.Buffer

	err = a.FormatReport(report, &buf3)
	if err != nil {
		t.Fatal(err)
	}
}

func TestHistoryAnalyzer_Misc(t *testing.T) {
	t.Parallel()

	a := &HistoryAnalyzer{}
	if a.Name() == "" {
		t.Error("Name empty")
	}

	if a.Flag() == "" {
		t.Error("Flag empty")
	}

	if a.Description() == "" {
		t.Error("Description empty")
	}

	if len(a.ListConfigurationOptions()) == 0 {
		t.Error("expected options")
	}

	clones := a.Fork(1)
	if len(clones) != 1 {
		t.Error("expected 1 clone")
	}

	a.Merge(nil)
}
