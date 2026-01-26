// Package typos provides typos functionality.
package typos

import (
	"bytes"
	"errors"
	"fmt"
	"io"
	"unicode/utf8"

	"github.com/go-git/go-git/v6"
	gitplumbing "github.com/go-git/go-git/v6/plumbing"
	"github.com/sergi/go-diff/diffmatchpatch"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/levenshtein"
	"github.com/Sumatoshi-tech/codefang/pkg/pipeline"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

// TyposHistoryAnalyzer detects typo-fix identifier pairs across commit history.
type TyposHistoryAnalyzer struct {
	l interface { //nolint:unused // used via dependency injection.
		Warnf(format string, args ...any)
	}
	UASTChanges            *plumbing.UASTChangesAnalyzer
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
	Commit  gitplumbing.Hash
	Line    int
}

const (
	// DefaultMaximumAllowedTypoDistance is the default maximum Levenshtein distance for typo detection.
	DefaultMaximumAllowedTypoDistance = 4
	// ConfigTyposDatasetMaximumAllowedDistance is the configuration key for the maximum Levenshtein distance.
	ConfigTyposDatasetMaximumAllowedDistance = "TyposDatasetBuilder.MaximumAllowedDistance"
)

// Name returns the name of the analyzer.
func (t *TyposHistoryAnalyzer) Name() string {
	return "TyposDataset"
}

// Flag returns the CLI flag for the analyzer.
func (t *TyposHistoryAnalyzer) Flag() string {
	return "typos-dataset"
}

// Description returns a human-readable description of the analyzer.
func (t *TyposHistoryAnalyzer) Description() string {
	return "Extracts typo-fix identifier pairs from source code in commit diffs."
}

// ListConfigurationOptions returns the configuration options for the analyzer.
func (t *TyposHistoryAnalyzer) ListConfigurationOptions() []pipeline.ConfigurationOption {
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
func (t *TyposHistoryAnalyzer) Configure(facts map[string]any) error {
	if val, exists := facts[ConfigTyposDatasetMaximumAllowedDistance].(int); exists {
		t.MaximumAllowedDistance = val
	}

	if t.MaximumAllowedDistance <= 0 {
		t.MaximumAllowedDistance = DefaultMaximumAllowedTypoDistance
	}

	return nil
}

// Initialize prepares the analyzer for processing commits.
func (t *TyposHistoryAnalyzer) Initialize(_ *git.Repository) error {
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

// Consume processes a single commit with the provided dependency results.
//
//nolint:cyclop,funlen,gocognit,gocyclo // complex function.
func (t *TyposHistoryAnalyzer) Consume(ctx *analyze.Context) error {
	commit := ctx.Commit.Hash

	changes := t.UASTChanges.Changes
	cache := t.BlobCache.Cache
	diffs := t.FileDiff.FileDiffs

	for _, change := range changes {
		if change.Before == nil || change.After == nil {
			continue
		}

		blobBefore := cache[change.Change.From.TreeEntry.Hash]
		blobAfter := cache[change.Change.To.TreeEntry.Hash]

		// Lines split.
		linesBefore := bytes.Split(blobBefore.Data, []byte{'\n'})
		linesAfter := bytes.Split(blobAfter.Data, []byte{'\n'})

		diff, ok := diffs[change.Change.To.Name]
		if !ok {
			continue
		}

		var lineNumBefore, lineNumAfter int

		var candidates []candidate

		focusedLinesBefore := map[int]bool{}
		focusedLinesAfter := map[int]bool{}
		removedSize := 0

		for _, edit := range diff.Diffs {
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

		if len(candidates) == 0 {
			continue
		}

		addedIdentifiers := map[int][]*node.Node{}
		removedIdentifiers := map[int][]*node.Node{}

		if change.Before != nil {
			identifiers := extractIdentifiers(change.Before)
			for _, id := range identifiers {
				if id.Pos != nil && focusedLinesBefore[int(id.Pos.StartLine)-1] { //nolint:gosec // security concern is acceptable here.
					line := int(id.Pos.StartLine) - 1 //nolint:gosec // security concern is acceptable here.
					removedIdentifiers[line] = append(removedIdentifiers[line], id)
				}
			}
		}

		if change.After != nil {
			identifiers := extractIdentifiers(change.After)
			for _, id := range identifiers {
				if id.Pos != nil && focusedLinesAfter[int(id.Pos.StartLine)-1] { //nolint:gosec // security concern is acceptable here.
					line := int(id.Pos.StartLine) - 1 //nolint:gosec // security concern is acceptable here.
					addedIdentifiers[line] = append(addedIdentifiers[line], id)
				}
			}
		}

		for _, candidate := range candidates {
			nodesBefore := removedIdentifiers[candidate.Before]

			nodesAfter := addedIdentifiers[candidate.After]
			if len(nodesBefore) == 1 && len(nodesAfter) == 1 {
				idBefore := nodesBefore[0].Token
				idAfter := nodesAfter[0].Token
				t.typos = append(t.typos, Typo{
					Wrong:   idBefore,
					Correct: idAfter,
					Commit:  commit,
					File:    change.Change.To.Name,
					Line:    candidate.After,
				})
			}
		}
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
func (t *TyposHistoryAnalyzer) Finalize() (analyze.Report, error) {
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
func (t *TyposHistoryAnalyzer) Fork(n int) []analyze.HistoryAnalyzer {
	res := make([]analyze.HistoryAnalyzer, n)
	for i := range n {
		res[i] = t // Shared state.
	}

	return res
}

// Merge combines results from forked analyzer branches.
func (t *TyposHistoryAnalyzer) Merge(_ []analyze.HistoryAnalyzer) {
}

// Serialize writes the analysis result to the given writer.
func (t *TyposHistoryAnalyzer) Serialize(result analyze.Report, _ bool, writer io.Writer) error {
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
func (t *TyposHistoryAnalyzer) FormatReport(report analyze.Report, writer io.Writer) error {
	return t.Serialize(report, false, writer)
}
