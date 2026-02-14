// Package typos provides typos functionality.
package typos

import (
	"bytes"
	"encoding/json"
	"fmt"
	"io"
	"unicode/utf8"

	"github.com/sergi/go-diff/diffmatchpatch"
	"gopkg.in/yaml.v3"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/common/reportutil"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
	"github.com/Sumatoshi-tech/codefang/pkg/levenshtein"
	"github.com/Sumatoshi-tech/codefang/pkg/pipeline"
	"github.com/Sumatoshi-tech/codefang/pkg/safeconv"
	"github.com/Sumatoshi-tech/codefang/pkg/uast"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

// HistoryAnalyzer detects typo-fix identifier pairs across commit history.
type HistoryAnalyzer struct {
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

// NeedsUAST returns true because typo detection requires UAST parsing.
func (t *HistoryAnalyzer) NeedsUAST() bool { return true }

// Flag returns the CLI flag for the analyzer.
func (t *HistoryAnalyzer) Flag() string {
	return "typos-dataset"
}

// Description returns a human-readable description of the analyzer.
func (t *HistoryAnalyzer) Description() string {
	return t.Descriptor().Description
}

// Descriptor returns stable analyzer metadata.
func (t *HistoryAnalyzer) Descriptor() analyze.Descriptor {
	return analyze.Descriptor{
		ID:          "history/typos",
		Description: "Extracts typo-fix identifier pairs from source code in commit diffs.",
		Mode:        analyze.ModeHistory,
	}
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
				candidates = t.matchDeleteInsertPairs(
					lineNumBefore, lineNumAfter, size,
					linesBefore, linesAfter,
					candidates, focusedLinesBefore, focusedLinesAfter,
				)
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

// matchDeleteInsertPairs checks each line pair in a delete/insert hunk for typo candidates.
func (t *HistoryAnalyzer) matchDeleteInsertPairs(
	lineNumBefore, lineNumAfter, size int,
	linesBefore, linesAfter [][]byte,
	candidates []candidate,
	focusedBefore, focusedAfter map[int]bool,
) []candidate {
	for i := range size {
		lb := lineNumBefore - size + i
		la := lineNumAfter + i

		if lb >= len(linesBefore) || la >= len(linesAfter) {
			continue
		}

		dist := t.lcontext.Distance(string(linesBefore[lb]), string(linesAfter[la]))
		if dist <= t.MaximumAllowedDistance {
			candidates = append(candidates, candidate{lb, la})
			focusedBefore[lb] = true
			focusedAfter[la] = true
		}
	}

	return candidates
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
		if id.Pos != nil && focusedLines[safeconv.MustUintToInt(id.Pos.StartLine)-1] {
			line := safeconv.MustUintToInt(id.Pos.StartLine) - 1
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
		clone := &HistoryAnalyzer{
			UAST:                   &plumbing.UASTChangesAnalyzer{},
			BlobCache:              &plumbing.BlobCacheAnalyzer{},
			FileDiff:               &plumbing.FileDiffAnalyzer{},
			MaximumAllowedDistance: t.MaximumAllowedDistance,
		}
		clone.lcontext = &levenshtein.Context{}
		clone.typos = nil // Fresh accumulator for this fork.
		res[i] = clone
	}

	return res
}

// Merge combines results from forked analyzer branches.
func (t *HistoryAnalyzer) Merge(branches []analyze.HistoryAnalyzer) {
	for _, branch := range branches {
		other, ok := branch.(*HistoryAnalyzer)
		if !ok {
			continue
		}

		t.typos = append(t.typos, other.typos...)
	}
}

// SequentialOnly returns false because typo detection can be parallelized.
func (t *HistoryAnalyzer) SequentialOnly() bool { return false }

// CPUHeavy returns true because typo detection performs UAST processing per commit.
func (t *HistoryAnalyzer) CPUHeavy() bool { return true }

// SnapshotPlumbing captures the current plumbing output state for parallel execution.
func (t *HistoryAnalyzer) SnapshotPlumbing() analyze.PlumbingSnapshot {
	return plumbing.Snapshot{
		UASTChanges: t.UAST.TransferChanges(),
		BlobCache:   t.BlobCache.Cache,
		FileDiffs:   t.FileDiff.FileDiffs,
	}
}

// ApplySnapshot restores plumbing state from a previously captured snapshot.
func (t *HistoryAnalyzer) ApplySnapshot(snap analyze.PlumbingSnapshot) {
	ss, ok := snap.(plumbing.Snapshot)
	if !ok {
		return
	}

	t.UAST.SetChanges(ss.UASTChanges)
	t.BlobCache.Cache = ss.BlobCache
	t.FileDiff.FileDiffs = ss.FileDiffs
}

// ReleaseSnapshot releases UAST trees owned by the snapshot.
func (t *HistoryAnalyzer) ReleaseSnapshot(snap analyze.PlumbingSnapshot) {
	ss, ok := snap.(plumbing.Snapshot)
	if !ok {
		return
	}

	for _, ch := range ss.UASTChanges {
		node.ReleaseTree(ch.Before)
		node.ReleaseTree(ch.After)
	}
}

// Serialize writes the analysis result to the given writer.
func (t *HistoryAnalyzer) Serialize(result analyze.Report, format string, writer io.Writer) error {
	switch format {
	case analyze.FormatJSON:
		return t.serializeJSON(result, writer)
	case analyze.FormatYAML:
		return t.serializeYAML(result, writer)
	case analyze.FormatPlot:
		return t.generatePlot(result, writer)
	case analyze.FormatBinary:
		return t.serializeBinary(result, writer)
	default:
		return fmt.Errorf("%w: %s", analyze.ErrUnsupportedFormat, format)
	}
}

func (t *HistoryAnalyzer) serializeJSON(result analyze.Report, writer io.Writer) error {
	metrics, err := ComputeAllMetrics(result)
	if err != nil {
		metrics = &ComputedMetrics{}
	}

	jsonData, err := json.MarshalIndent(metrics, "", "  ")
	if err != nil {
		return fmt.Errorf("json marshal: %w", err)
	}

	_, err = fmt.Fprint(writer, string(jsonData))
	if err != nil {
		return fmt.Errorf("write json: %w", err)
	}

	return nil
}

func (t *HistoryAnalyzer) serializeYAML(result analyze.Report, writer io.Writer) error {
	metrics, err := ComputeAllMetrics(result)
	if err != nil {
		metrics = &ComputedMetrics{}
	}

	data, err := yaml.Marshal(metrics)
	if err != nil {
		return fmt.Errorf("yaml marshal: %w", err)
	}

	_, err = writer.Write(data)
	if err != nil {
		return fmt.Errorf("write yaml: %w", err)
	}

	return nil
}

func (t *HistoryAnalyzer) serializeBinary(result analyze.Report, writer io.Writer) error {
	metrics, err := ComputeAllMetrics(result)
	if err != nil {
		metrics = &ComputedMetrics{}
	}

	err = reportutil.EncodeBinaryEnvelope(metrics, writer)
	if err != nil {
		return fmt.Errorf("binary encode: %w", err)
	}

	return nil
}

// FormatReport writes the formatted analysis report to the given writer.
func (t *HistoryAnalyzer) FormatReport(report analyze.Report, writer io.Writer) error {
	return t.Serialize(report, analyze.FormatYAML, writer)
}
