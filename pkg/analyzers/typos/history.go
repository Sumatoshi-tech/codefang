// Package typos provides typos functionality.
package typos

import (
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"unicode/utf8"

	"github.com/sergi/go-diff/diffmatchpatch"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
	"github.com/Sumatoshi-tech/codefang/pkg/levenshtein"
	"github.com/Sumatoshi-tech/codefang/pkg/pipeline"
	"github.com/Sumatoshi-tech/codefang/pkg/uast"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

// HistoryAnalyzer detects typo-fix identifier pairs across commit history.
type HistoryAnalyzer struct {
	l interface { //nolint:unused // used via dependency injection.
		Warnf(format string, args ...any)
	}
	UAST                   *plumbing.UASTChangesAnalyzer
	FileDiff               *plumbing.FileDiffAnalyzer
	BlobCache              *plumbing.BlobCacheAnalyzer
	lcontext               *levenshtein.Context
	typos                  []Typo
	MaximumAllowedDistance int
}

// Typo represents a detected typo-fix pair in source code.
type Typo struct {
	Wrong   string
	Correct string
	File    string
	Commit  gitlib.Hash
	Line    int
}

const (
	// DefaultMaximumAllowedTypoDistance is the default maximum Levenshtein distance for typo detection.
	DefaultMaximumAllowedTypoDistance = 4
	// ConfigTyposDatasetMaximumAllowedDistance is the configuration key for the maximum Levenshtein distance.
	ConfigTyposDatasetMaximumAllowedDistance = "TyposDatasetBuilder.MaximumAllowedDistance"
)

// Name returns the name of the analyzer.
func (t *HistoryAnalyzer) Name() string {
	return "TyposDataset"
}

// Flag returns the CLI flag for the analyzer.
func (t *HistoryAnalyzer) Flag() string {
	return "typos-dataset"
}

// Description returns a human-readable description of the analyzer.
func (t *HistoryAnalyzer) Description() string {
	return "Extracts typo-fix identifier pairs from source code in commit diffs."
}

// ListConfigurationOptions returns the configuration options for the analyzer.
func (t *HistoryAnalyzer) ListConfigurationOptions() []pipeline.ConfigurationOption {
	return []pipeline.ConfigurationOption{
		{
			Name:        ConfigTyposDatasetMaximumAllowedDistance,
			Description: "Maximum Levenshtein distance between two identifiers to consider them a typo-fix pair.",
			Flag:        "typos-max-distance",
			Type:        pipeline.IntConfigurationOption,
			Default:     DefaultMaximumAllowedTypoDistance,
		},
	}
}

// Configure sets up the analyzer with the provided facts.
func (t *HistoryAnalyzer) Configure(facts map[string]any) error {
	if val, exists := facts[ConfigTyposDatasetMaximumAllowedDistance].(int); exists {
		t.MaximumAllowedDistance = val
	}

	if t.MaximumAllowedDistance <= 0 {
		t.MaximumAllowedDistance = DefaultMaximumAllowedTypoDistance
	}

	return nil
}

// Initialize prepares the analyzer for processing commits.
func (t *HistoryAnalyzer) Initialize(_ *gitlib.Repository) error {
	t.lcontext = &levenshtein.Context{}
	if t.MaximumAllowedDistance <= 0 {
		t.MaximumAllowedDistance = DefaultMaximumAllowedTypoDistance
	}

	return nil
}

type candidate struct {
	Before int
	After  int
}

// typoCandidateResult holds the output of findTypoCandidates.
type typoCandidateResult struct {
	candidates         []candidate
	focusedLinesBefore map[int]bool
	focusedLinesAfter  map[int]bool
}

// findTypoCandidates scans the diff edits and identifies line pairs where the before/after
// lines are within the maximum allowed Levenshtein distance, indicating a potential typo fix.
//
//nolint:gocognit // complexity is inherent to diff-based line pair matching with distance calculation.
func (t *HistoryAnalyzer) findTypoCandidates(
	diffs []diffmatchpatch.Diff,
	linesBefore, linesAfter [][]byte,
) typoCandidateResult {
	var (
		lineNumBefore int
		lineNumAfter  int
		removedSize   int
		candidates    []candidate
	)

	focusedLinesBefore := map[int]bool{}
	focusedLinesAfter := map[int]bool{}

	for _, edit := range diffs {
		size := utf8.RuneCountInString(edit.Text)

		switch edit.Type {
		case diffmatchpatch.DiffDelete:
			lineNumBefore += size
			removedSize = size
		case diffmatchpatch.DiffInsert:
			if size == removedSize {
				for i := range size {
					lb := lineNumBefore - size + i
					la := lineNumAfter + i

					if lb < len(linesBefore) && la < len(linesAfter) {
						dist := t.lcontext.Distance(string(linesBefore[lb]), string(linesAfter[la]))
						if dist <= t.MaximumAllowedDistance {
							candidates = append(candidates, candidate{lb, la})
							focusedLinesBefore[lb] = true
							focusedLinesAfter[la] = true
						}
					}
				}
			}

			lineNumAfter += size
			removedSize = 0
		case diffmatchpatch.DiffEqual:
			lineNumBefore += size
			lineNumAfter += size
			removedSize = 0
		}
	}

	return typoCandidateResult{
		candidates:         candidates,
		focusedLinesBefore: focusedLinesBefore,
		focusedLinesAfter:  focusedLinesAfter,
	}
}

// matchTypoIdentifiers extracts identifiers from the before/after UAST nodes that fall on
// the focused lines, and returns the matched typo pairs from the given candidates.
func (t *HistoryAnalyzer) matchTypoIdentifiers(
	change uast.Change,
	result typoCandidateResult,
	commit gitlib.Hash,
) []Typo {
	removedIdentifiers := collectIdentifiersOnLines(change.Before, result.focusedLinesBefore)
	addedIdentifiers := collectIdentifiersOnLines(change.After, result.focusedLinesAfter)

	var typos []Typo

	for _, cand := range result.candidates {
		nodesBefore := removedIdentifiers[cand.Before]
		nodesAfter := addedIdentifiers[cand.After]

		if len(nodesBefore) == 1 && len(nodesAfter) == 1 {
			typos = append(typos, Typo{
				Wrong:   nodesBefore[0].Token,
				Correct: nodesAfter[0].Token,
				Commit:  commit,
				File:    change.Change.To.Name,
				Line:    cand.After,
			})
		}
	}

	return typos
}

// collectIdentifiersOnLines extracts identifiers from the UAST root whose start line
// (converted to 0-based) is present in the focusedLines set.
func collectIdentifiersOnLines(root *node.Node, focusedLines map[int]bool) map[int][]*node.Node {
	result := map[int][]*node.Node{}

	if root == nil {
		return result
	}

	identifiers := extractIdentifiers(root)
	for _, id := range identifiers {
		if id.Pos != nil && focusedLines[int(id.Pos.StartLine)-1] { //nolint:gosec // security concern is acceptable here.
			line := int(id.Pos.StartLine) - 1 //nolint:gosec // security concern is acceptable here.
			result[line] = append(result[line], id)
		}
	}

	return result
}

// Consume processes a single commit with the provided dependency results.
func (t *HistoryAnalyzer) Consume(ctx *analyze.Context) error {
	commit := ctx.Commit.Hash()

	changes := t.UAST.Changes()
	cache := t.BlobCache.Cache
	diffs := t.FileDiff.FileDiffs

	for _, change := range changes {
		if change.Before == nil || change.After == nil {
			continue
		}

		blobBefore := cache[change.Change.From.Hash]
		blobAfter := cache[change.Change.To.Hash]

		linesBefore := bytes.Split(blobBefore.Data, []byte{'\n'})
		linesAfter := bytes.Split(blobAfter.Data, []byte{'\n'})

		diff, ok := diffs[change.Change.To.Name]
		if !ok {
			continue
		}

		result := t.findTypoCandidates(diff.Diffs, linesBefore, linesAfter)
		if len(result.candidates) == 0 {
			continue
		}

		t.typos = append(t.typos, t.matchTypoIdentifiers(change, result, commit)...)
	}

	return nil
}

func extractIdentifiers(root *node.Node) []*node.Node {
	var identifiers []*node.Node

	root.VisitPreOrder(func(n *node.Node) {
		if n.Type == node.UASTIdentifier {
			identifiers = append(identifiers, n)
		}
	})

	return identifiers
}

// Finalize completes the analysis and returns the result.
func (t *HistoryAnalyzer) Finalize() (analyze.Report, error) {
	// Deduplicate.
	typos := make([]Typo, 0, len(t.typos))
	pairs := map[string]bool{}

	for _, typo := range t.typos {
		id := typo.Wrong + "|" + typo.Correct
		if _, exists := pairs[id]; !exists {
			pairs[id] = true

			typos = append(typos, typo)
		}
	}

	return analyze.Report{
		"typos": typos,
	}, nil
}

// Fork creates a copy of the analyzer for parallel processing.
func (t *HistoryAnalyzer) Fork(n int) []analyze.HistoryAnalyzer {
	res := make([]analyze.HistoryAnalyzer, n)
	for i := range n {
		res[i] = t // Shared state.
	}

	return res
}

// Merge combines results from forked analyzer branches.
func (t *HistoryAnalyzer) Merge(_ []analyze.HistoryAnalyzer) {
}

// Serialize writes the analysis result to the given writer.
func (t *HistoryAnalyzer) Serialize(result analyze.Report, format string, writer io.Writer) error {
	if format == analyze.FormatPlot {
		return t.generatePlot(result, writer)
	}

	if format == analyze.FormatJSON {
		err := json.NewEncoder(writer).Encode(result)
		if err != nil {
			return fmt.Errorf("json encode: %w", err)
		}

		return nil
	}

	typos, ok := result["typos"].([]Typo)
	if !ok {
		return errors.New("expected []Typo for typos") //nolint:err113 // descriptive error for type assertion failure.
	}

	for _, ty := range typos {
		fmt.Fprintf(writer, "  - wrong: %s\n", ty.Wrong)
		fmt.Fprintf(writer, "    correct: %s\n", ty.Correct)
		fmt.Fprintf(writer, "    commit: %s\n", ty.Commit.String())
		fmt.Fprintf(writer, "    file: %s\n", ty.File)
		fmt.Fprintf(writer, "    line: %d\n", ty.Line)
	}

	return nil
}

// FormatReport writes the formatted analysis report to the given writer.
func (t *HistoryAnalyzer) FormatReport(report analyze.Report, writer io.Writer) error {
	return t.Serialize(report, analyze.FormatYAML, writer)
}
