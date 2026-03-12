package plumbing

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"unicode/utf8"

	"github.com/sergi/go-diff/diffmatchpatch"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	pkgplumbing "github.com/Sumatoshi-tech/codefang/internal/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
	"github.com/Sumatoshi-tech/codefang/pkg/pipeline"
)

// LinesStatsCalculator computes line-level statistics for file diffs.
type LinesStatsCalculator struct {
	// Dependencies.
	TreeDiff  *TreeDiffAnalyzer
	BlobCache *BlobCacheAnalyzer
	FileDiff  *FileDiffAnalyzer

	// Output.
	LineStats map[gitlib.ChangeEntry]pkgplumbing.LineStats
}

// Name returns the name of the analyzer.
func (l *LinesStatsCalculator) Name() string {
	return "LinesStats"
}

// Flag returns the CLI flag for the analyzer.
func (l *LinesStatsCalculator) Flag() string {
	return "lines-stats"
}

// Description returns a human-readable description of the analyzer.
func (l *LinesStatsCalculator) Description() string {
	return l.Descriptor().Description
}

// Descriptor returns stable analyzer metadata.
func (l *LinesStatsCalculator) Descriptor() analyze.Descriptor {
	return analyze.NewDescriptor(
		analyze.ModeHistory,
		l.Name(),
		"Measures line statistics for each text file in the commit.",
	)
}

// ListConfigurationOptions returns the configuration options for the analyzer.
func (l *LinesStatsCalculator) ListConfigurationOptions() []pipeline.ConfigurationOption {
	return []pipeline.ConfigurationOption{}
}

// Configure sets up the analyzer with the provided facts.
func (l *LinesStatsCalculator) Configure(_ map[string]any) error {
	return nil
}

// Initialize prepares the analyzer for processing commits.
func (l *LinesStatsCalculator) Initialize(_ *gitlib.Repository) error {
	return nil
}

// Consume processes a single commit with the provided dependency results.
func (l *LinesStatsCalculator) Consume(_ context.Context, ac *analyze.Context) (analyze.TC, error) {
	result := map[gitlib.ChangeEntry]pkgplumbing.LineStats{}

	if ac.IsMerge {
		l.LineStats = result

		return analyze.TC{}, nil
	}

	treeDiff := l.TreeDiff.Changes
	cache := l.BlobCache.Cache
	fileDiffs := l.FileDiff.FileDiffs

	for _, change := range treeDiff {
		switch change.Action {
		case gitlib.Insert:
			computeInsertStats(change, cache, result)
		case gitlib.Delete:
			computeDeleteStats(change, cache, result)
		case gitlib.Modify:
			computeModifyStats(change, fileDiffs, result)
		}
	}

	l.LineStats = result

	return analyze.TC{}, nil
}

func computeInsertStats(
	change *gitlib.Change, cache map[gitlib.Hash]*gitlib.CachedBlob,
	result map[gitlib.ChangeEntry]pkgplumbing.LineStats,
) {
	blob := cache[change.To.Hash]
	if blob == nil {
		return
	}

	lines, countErr := blob.CountLines()
	if countErr != nil {
		return
	}

	result[change.To] = pkgplumbing.LineStats{
		Added:   lines,
		Removed: 0,
		Changed: 0,
	}
}

func computeDeleteStats(
	change *gitlib.Change, cache map[gitlib.Hash]*gitlib.CachedBlob,
	result map[gitlib.ChangeEntry]pkgplumbing.LineStats,
) {
	blob := cache[change.From.Hash]
	if blob == nil {
		return
	}

	lines, countErr := blob.CountLines()
	if countErr != nil {
		return
	}

	result[change.From] = pkgplumbing.LineStats{
		Added:   0,
		Removed: lines,
		Changed: 0,
	}
}

func computeModifyStats(
	change *gitlib.Change, fileDiffs map[string]pkgplumbing.FileDiffData,
	result map[gitlib.ChangeEntry]pkgplumbing.LineStats,
) {
	thisDiffs, ok := fileDiffs[change.To.Name]
	if !ok {
		return
	}

	added, removed, changed := computeDiffLineStats(thisDiffs.Diffs)

	result[change.To] = pkgplumbing.LineStats{
		Added:   added,
		Removed: removed,
		Changed: changed,
	}
}

func computeDiffLineStats(diffs []diffmatchpatch.Diff) (added, removed, changed int) {
	var removedPending int

	for _, edit := range diffs {
		switch edit.Type {
		case diffmatchpatch.DiffEqual:
			removed += removedPending
			removedPending = 0
		case diffmatchpatch.DiffInsert:
			delta := utf8.RuneCountInString(edit.Text)
			if removedPending > delta {
				changed += delta
				removed += removedPending - delta
			} else {
				changed += removedPending
				added += delta - removedPending
			}

			removedPending = 0
		case diffmatchpatch.DiffDelete:
			removedPending = utf8.RuneCountInString(edit.Text)
		}
	}

	removed += removedPending

	return added, removed, changed
}

// Fork creates a copy of the analyzer for parallel processing.
func (l *LinesStatsCalculator) Fork(n int) []analyze.HistoryAnalyzer {
	res := make([]analyze.HistoryAnalyzer, n)
	for i := range n {
		clone := *l
		res[i] = &clone
	}

	return res
}

// Merge combines results from forked analyzer branches.
func (l *LinesStatsCalculator) Merge(_ []analyze.HistoryAnalyzer) {
}

// Serialize writes the analysis result to the given writer.
func (l *LinesStatsCalculator) Serialize(report analyze.Report, format string, writer io.Writer) error {
	if format == analyze.FormatJSON {
		err := json.NewEncoder(writer).Encode(report)
		if err != nil {
			return fmt.Errorf("json encode: %w", err)
		}
	}

	return nil
}

// WorkingStateSize returns 0 — plumbing analyzers are excluded from budget planning.
func (l *LinesStatsCalculator) WorkingStateSize() int64 { return 0 }

// AvgTCSize returns 0 — plumbing analyzers do not emit meaningful TC payloads.
func (l *LinesStatsCalculator) AvgTCSize() int64 { return 0 }

// NewAggregator returns nil — plumbing analyzers do not aggregate.
func (l *LinesStatsCalculator) NewAggregator(_ analyze.AggregatorOptions) analyze.Aggregator {
	return nil
}

// SerializeTICKs returns ErrNotImplemented — plumbing analyzers do not produce TICKs.
func (l *LinesStatsCalculator) SerializeTICKs(_ []analyze.TICK, _ string, _ io.Writer) error {
	return analyze.ErrNotImplemented
}

// ReportFromTICKs returns ErrNotImplemented — plumbing analyzers do not produce reports.
func (l *LinesStatsCalculator) ReportFromTICKs(_ context.Context, _ []analyze.TICK) (analyze.Report, error) {
	return nil, analyze.ErrNotImplemented
}
