package plumbing

import (
	"fmt"
	"io"
	"unicode/utf8"

	"github.com/go-git/go-git/v6"
	"github.com/go-git/go-git/v6/plumbing/object"
	"github.com/go-git/go-git/v6/utils/merkletrie"
	"github.com/sergi/go-diff/diffmatchpatch"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/pipeline"
	pkgplumbing "github.com/Sumatoshi-tech/codefang/pkg/plumbing"
)

// LinesStatsCalculator computes line-level statistics for file diffs.
type LinesStatsCalculator struct {
	// Dependencies.
	TreeDiff  *TreeDiffAnalyzer
	BlobCache *BlobCacheAnalyzer
	FileDiff  *FileDiffAnalyzer

	// Output.
	LineStats map[object.ChangeEntry]pkgplumbing.LineStats

	// Internal. //nolint:unused // used via reflection or external caller.
	l interface { //nolint:unused // acknowledged.
		Warnf(format string, args ...any)
	}
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
	return "Measures line statistics for each text file in the commit."
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
func (l *LinesStatsCalculator) Initialize(_ *git.Repository) error {
	return nil
}

// Consume processes a single commit with the provided dependency results.
//
//nolint:cyclop,funlen,gocognit,gocyclo // complex function.
func (l *LinesStatsCalculator) Consume(ctx *analyze.Context) error {
	result := map[object.ChangeEntry]pkgplumbing.LineStats{}

	if ctx.IsMerge {
		l.LineStats = result

		return nil
	}

	treeDiff := l.TreeDiff.Changes
	cache := l.BlobCache.Cache
	fileDiffs := l.FileDiff.FileDiffs

	for _, change := range treeDiff {
		action, err := change.Action()
		if err != nil {
			return fmt.Errorf("consume: %w", err)
		}

		switch action {
		case merkletrie.Insert:
			blob := cache[change.To.TreeEntry.Hash]
			if blob == nil {
				continue
			}

			lines, countErr := blob.CountLines()
			if countErr != nil {
				continue
			}

			result[change.To] = pkgplumbing.LineStats{
				Added:   lines,
				Removed: 0,
				Changed: 0,
			}
		case merkletrie.Delete:
			blob := cache[change.From.TreeEntry.Hash]
			if blob == nil {
				continue
			}

			lines, countErr := blob.CountLines()
			if countErr != nil {
				continue
			}

			result[change.From] = pkgplumbing.LineStats{
				Added:   0,
				Removed: lines,
				Changed: 0,
			}
		case merkletrie.Modify:
			thisDiffs, ok := fileDiffs[change.To.Name]
			if !ok {
				continue
			}

			var added, removed, changed, removedPending int

			for _, edit := range thisDiffs.Diffs {
				switch edit.Type {
				case diffmatchpatch.DiffEqual:
					if removedPending > 0 {
						removed += removedPending
					}

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

			if removedPending > 0 {
				removed += removedPending
			}

			result[change.To] = pkgplumbing.LineStats{
				Added:   added,
				Removed: removed,
				Changed: changed,
			}
		}
	}

	l.LineStats = result

	return nil
}

// Finalize completes the analysis and returns the result.
func (l *LinesStatsCalculator) Finalize() (analyze.Report, error) {
	return nil, nil //nolint:nilnil // nil,nil return is intentional.
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
func (l *LinesStatsCalculator) Serialize(_ analyze.Report, _ bool, _ io.Writer) error {
	return nil
}
