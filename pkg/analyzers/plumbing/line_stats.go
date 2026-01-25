package plumbing

import (
	"io"
	"unicode/utf8"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/pipeline"
	pkgplumbing "github.com/Sumatoshi-tech/codefang/pkg/plumbing"
	"github.com/go-git/go-git/v6"
	"github.com/go-git/go-git/v6/plumbing/object"
	"github.com/go-git/go-git/v6/utils/merkletrie"
	"github.com/sergi/go-diff/diffmatchpatch"
)

type LinesStatsCalculator struct {
	// Dependencies
	TreeDiff  *TreeDiffAnalyzer
	BlobCache *BlobCacheAnalyzer
	FileDiff  *FileDiffAnalyzer

	// Output
	LineStats map[object.ChangeEntry]pkgplumbing.LineStats

	// Internal
	l interface {
		Warnf(format string, args ...interface{})
	}
}

func (l *LinesStatsCalculator) Name() string {
	return "LinesStats"
}

func (l *LinesStatsCalculator) Flag() string {
	return "lines-stats"
}

func (l *LinesStatsCalculator) Description() string {
	return "Measures line statistics for each text file in the commit."
}

func (l *LinesStatsCalculator) ListConfigurationOptions() []pipeline.ConfigurationOption {
	return []pipeline.ConfigurationOption{}
}

func (l *LinesStatsCalculator) Configure(facts map[string]interface{}) error {
	return nil
}

func (l *LinesStatsCalculator) Initialize(repository *git.Repository) error {
	return nil
}

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
			return err
		}
		switch action {
		case merkletrie.Insert:
			blob := cache[change.To.TreeEntry.Hash]
			if blob == nil {
				continue
			}
			lines, err := blob.CountLines()
			if err != nil {
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
			lines, err := blob.CountLines()
			if err != nil {
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

func (l *LinesStatsCalculator) Finalize() (analyze.Report, error) {
	return nil, nil
}

func (l *LinesStatsCalculator) Fork(n int) []analyze.HistoryAnalyzer {
	res := make([]analyze.HistoryAnalyzer, n)
	for i := 0; i < n; i++ {
		clone := *l
		res[i] = &clone
	}
	return res
}

func (l *LinesStatsCalculator) Merge(branches []analyze.HistoryAnalyzer) {
}

func (l *LinesStatsCalculator) Serialize(result analyze.Report, binary bool, writer io.Writer) error {
	return nil
}
