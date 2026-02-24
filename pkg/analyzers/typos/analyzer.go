// Package typos provides typos functionality.
package typos

import (
	"bytes"
	"context"
	"unicode/utf8"

	"github.com/sergi/go-diff/diffmatchpatch"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
	"github.com/Sumatoshi-tech/codefang/pkg/levenshtein"
	"github.com/Sumatoshi-tech/codefang/pkg/pipeline"
	"github.com/Sumatoshi-tech/codefang/pkg/safeconv"
	"github.com/Sumatoshi-tech/codefang/pkg/uast"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

// Typo represents a detected typo-fix pair in source code.
type Typo struct {
	Wrong   string
	Correct string
	File    string
	Commit  gitlib.Hash
	Line    int
}

// TickData is the aggregated payload stored in analyze.TICK.Data.
type TickData struct {
	Typos []Typo
}

const (
	// DefaultMaximumAllowedTypoDistance is the default maximum Levenshtein distance for typo detection.
	DefaultMaximumAllowedTypoDistance = 4
	// ConfigTyposDatasetMaximumAllowedDistance is the configuration key for the maximum Levenshtein distance.
	ConfigTyposDatasetMaximumAllowedDistance = "TyposDatasetBuilder.MaximumAllowedDistance"
)

// Analyzer detects typo-fix identifier pairs across commit history.
type Analyzer struct {
	*analyze.BaseHistoryAnalyzer[*ComputedMetrics]

	UAST                   *plumbing.UASTChangesAnalyzer
	FileDiff               *plumbing.FileDiffAnalyzer
	BlobCache              *plumbing.BlobCacheAnalyzer
	lcontext               *levenshtein.Context
	MaximumAllowedDistance int
}

// NewAnalyzer creates a new typos analyzer.
func NewAnalyzer() *Analyzer {
	a := &Analyzer{}
	a.BaseHistoryAnalyzer = &analyze.BaseHistoryAnalyzer[*ComputedMetrics]{
		Desc: analyze.Descriptor{
			ID:          "history/typos",
			Description: "Extracts typo-fix identifier pairs from source code in commit diffs.",
			Mode:        analyze.ModeHistory,
		},
		Sequential: false,
		ConfigOptions: []pipeline.ConfigurationOption{
			{
				Name:        ConfigTyposDatasetMaximumAllowedDistance,
				Description: "Maximum Levenshtein distance between two identifiers to consider them a typo-fix pair.",
				Flag:        "typos-max-distance",
				Type:        pipeline.IntConfigurationOption,
				Default:     DefaultMaximumAllowedTypoDistance,
			},
		},
		ComputeMetricsFn: computeMetricsSafe,
		AggregatorFn:     newAggregator,
	}

	a.TicksToReportFn = ticksToReport

	return a
}

func computeMetricsSafe(report analyze.Report) (*ComputedMetrics, error) {
	if len(report) == 0 {
		return &ComputedMetrics{}, nil
	}

	return ComputeAllMetrics(report)
}

// Configure sets up the analyzer with the provided facts.
func (t *Analyzer) Configure(facts map[string]any) error {
	if val, exists := facts[ConfigTyposDatasetMaximumAllowedDistance].(int); exists {
		t.MaximumAllowedDistance = val
	}

	if t.MaximumAllowedDistance <= 0 {
		t.MaximumAllowedDistance = DefaultMaximumAllowedTypoDistance
	}

	return nil
}

// Initialize prepares the analyzer for processing commits.
func (t *Analyzer) Initialize(_ *gitlib.Repository) error {
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
func (t *Analyzer) findTypoCandidates(
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
func (t *Analyzer) matchDeleteInsertPairs(
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
func (t *Analyzer) matchTypoIdentifiers(
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

// Consume processes a single commit and returns a TC with per-commit typo data.
// The analyzer does not retain any per-commit state; all output is in the TC.
func (t *Analyzer) Consume(ctx context.Context, ac *analyze.Context) (analyze.TC, error) {
	commit := ac.Commit.Hash()

	changes := t.UAST.Changes(ctx)
	cache := t.BlobCache.Cache
	diffs := t.FileDiff.FileDiffs

	var typos []Typo

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

		typos = append(typos, t.matchTypoIdentifiers(change, result, commit)...)
	}

	if len(typos) == 0 {
		return analyze.TC{}, nil
	}

	return analyze.TC{
		Data:       typos,
		CommitHash: commit,
	}, nil
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

// Fork creates independent copies of the analyzer for parallel processing.
func (t *Analyzer) Fork(n int) []analyze.HistoryAnalyzer {
	res := make([]analyze.HistoryAnalyzer, n)

	for i := range n {
		clone := &Analyzer{
			UAST:                   &plumbing.UASTChangesAnalyzer{},
			BlobCache:              &plumbing.BlobCacheAnalyzer{},
			FileDiff:               &plumbing.FileDiffAnalyzer{},
			MaximumAllowedDistance: t.MaximumAllowedDistance,
			lcontext:               &levenshtein.Context{},
		}
		res[i] = clone
	}

	return res
}

// Merge is a no-op. Per-commit results are emitted as TCs.
func (t *Analyzer) Merge(_ []analyze.HistoryAnalyzer) {}

// CPUHeavy returns true because typo detection performs UAST processing per commit.
func (t *Analyzer) CPUHeavy() bool { return true }

// NeedsUAST returns true to enable the UAST pipeline.
func (t *Analyzer) NeedsUAST() bool { return true }

// SnapshotPlumbing captures the current plumbing output state for parallel execution.
func (t *Analyzer) SnapshotPlumbing() analyze.PlumbingSnapshot {
	return plumbing.Snapshot{
		UASTChanges: t.UAST.TransferChanges(),
		BlobCache:   t.BlobCache.Cache,
		FileDiffs:   t.FileDiff.FileDiffs,
	}
}

// ApplySnapshot restores plumbing state from a previously captured snapshot.
func (t *Analyzer) ApplySnapshot(snap analyze.PlumbingSnapshot) {
	ss, ok := snap.(plumbing.Snapshot)
	if !ok {
		return
	}

	t.UAST.SetChanges(ss.UASTChanges)
	t.BlobCache.Cache = ss.BlobCache
	t.FileDiff.FileDiffs = ss.FileDiffs
}

// ReleaseSnapshot releases UAST trees owned by the snapshot.
func (t *Analyzer) ReleaseSnapshot(snap analyze.PlumbingSnapshot) {
	ss, ok := snap.(plumbing.Snapshot)
	if !ok {
		return
	}

	for _, ch := range ss.UASTChanges {
		node.ReleaseTree(ch.Before)
		node.ReleaseTree(ch.After)
	}
}

// NewAggregator creates an aggregator for this analyzer.
func (t *Analyzer) NewAggregator(opts analyze.AggregatorOptions) analyze.Aggregator {
	return t.AggregatorFn(opts)
}

// ReportFromTICKs converts aggregated TICKs into a Report.
func (t *Analyzer) ReportFromTICKs(ctx context.Context, ticks []analyze.TICK) (analyze.Report, error) {
	return t.TicksToReportFn(ctx, ticks), nil
}

// Extract properties for GenericAggregator.

const typoEntryOverhead = 80

func extractTC(tc analyze.TC, byTick map[int]*TickData) error {
	typos, ok := tc.Data.([]Typo)
	if !ok || len(typos) == 0 {
		return nil
	}

	state, ok := byTick[tc.Tick]
	if !ok || state == nil {
		state = &TickData{}
		byTick[tc.Tick] = state
	}

	state.Typos = append(state.Typos, typos...)

	return nil
}

func mergeState(existing, incoming *TickData) *TickData {
	if existing == nil {
		return incoming
	}

	if incoming == nil {
		return existing
	}

	existing.Typos = append(existing.Typos, incoming.Typos...)

	return existing
}

func sizeState(state *TickData) int64 {
	if state == nil {
		return 0
	}

	return int64(len(state.Typos)) * typoEntryOverhead
}

func buildTick(tick int, state *TickData) (analyze.TICK, error) {
	if state == nil || len(state.Typos) == 0 {
		return analyze.TICK{Tick: tick}, nil
	}

	// Deduplicate inside tick.
	state.Typos = deduplicateTypos(state.Typos)

	return analyze.TICK{
		Tick: tick,
		Data: state,
	}, nil
}

func newAggregator(opts analyze.AggregatorOptions) analyze.Aggregator {
	return analyze.NewGenericAggregator[*TickData, *TickData](
		opts,
		extractTC,
		mergeState,
		sizeState,
		buildTick,
	)
}

func deduplicateTypos(typos []Typo) []Typo {
	seen := make(map[string]bool, len(typos))
	result := make([]Typo, 0, len(typos))

	for _, t := range typos {
		key := t.Wrong + "|" + t.Correct
		if seen[key] {
			continue
		}

		seen[key] = true

		result = append(result, t)
	}

	return result
}

func ticksToReport(_ context.Context, ticks []analyze.TICK) analyze.Report {
	var allTypos []Typo

	for _, tick := range ticks {
		td, ok := tick.Data.(*TickData)
		if !ok || td == nil {
			continue
		}

		allTypos = append(allTypos, td.Typos...)
	}

	// Cross-tick deduplication.
	allTypos = deduplicateTypos(allTypos)

	return analyze.Report{
		"typos": allTypos,
	}
}
