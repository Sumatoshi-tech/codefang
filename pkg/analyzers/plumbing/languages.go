package plumbing

import (
	"io"
	"path"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/pipeline"
	pkgplumbing "github.com/Sumatoshi-tech/codefang/pkg/plumbing"
	"github.com/go-git/go-git/v6"
	gitplumbing "github.com/go-git/go-git/v6/plumbing"
	"github.com/go-git/go-git/v6/utils/merkletrie"
	"github.com/src-d/enry/v2"
)

type LanguagesDetectionAnalyzer struct {
	// Dependencies
	TreeDiff  *TreeDiffAnalyzer
	BlobCache *BlobCacheAnalyzer

	// Output
	Languages map[gitplumbing.Hash]string

	// Internal
	l interface {
		Warnf(format string, args ...interface{})
	}
}

const (
	ConfigLanguagesDetection = "LanguagesDetection"
)

func (l *LanguagesDetectionAnalyzer) Name() string {
	return "LanguagesDetection"
}

func (l *LanguagesDetectionAnalyzer) Flag() string {
	return "detect-languages"
}

func (l *LanguagesDetectionAnalyzer) Description() string {
	return "Run programming language detection over the changed files."
}

func (l *LanguagesDetectionAnalyzer) ListConfigurationOptions() []pipeline.ConfigurationOption {
	return []pipeline.ConfigurationOption{}
}

func (l *LanguagesDetectionAnalyzer) Configure(facts map[string]interface{}) error {
	return nil
}

func (l *LanguagesDetectionAnalyzer) Initialize(repository *git.Repository) error {
	return nil
}

func (l *LanguagesDetectionAnalyzer) Consume(ctx *analyze.Context) error {
	changes := l.TreeDiff.Changes
	cache := l.BlobCache.Cache
	result := map[gitplumbing.Hash]string{}

	for _, change := range changes {
		action, err := change.Action()
		if err != nil {
			return err
		}
		switch action {
		case merkletrie.Insert:
			result[change.To.TreeEntry.Hash] = l.detectLanguage(
				change.To.Name, cache[change.To.TreeEntry.Hash])
		case merkletrie.Delete:
			result[change.From.TreeEntry.Hash] = l.detectLanguage(
				change.From.Name, cache[change.From.TreeEntry.Hash])
		case merkletrie.Modify:
			result[change.To.TreeEntry.Hash] = l.detectLanguage(
				change.To.Name, cache[change.To.TreeEntry.Hash])
			result[change.From.TreeEntry.Hash] = l.detectLanguage(
				change.From.Name, cache[change.From.TreeEntry.Hash])
		}
	}
	l.Languages = result
	return nil
}

func (l *LanguagesDetectionAnalyzer) detectLanguage(name string, blob *pkgplumbing.CachedBlob) string {
	if blob == nil {
		return ""
	}
	_, err := blob.CountLines()
	if err == pkgplumbing.ErrorBinary {
		return ""
	}
	lang := enry.GetLanguage(path.Base(name), blob.Data)
	return lang
}

func (l *LanguagesDetectionAnalyzer) Finalize() (analyze.Report, error) {
	return nil, nil
}

func (l *LanguagesDetectionAnalyzer) Fork(n int) []analyze.HistoryAnalyzer {
	res := make([]analyze.HistoryAnalyzer, n)
	for i := 0; i < n; i++ {
		clone := *l
		res[i] = &clone
	}
	return res
}

func (l *LanguagesDetectionAnalyzer) Merge(branches []analyze.HistoryAnalyzer) {
}

func (l *LanguagesDetectionAnalyzer) Serialize(result analyze.Report, binary bool, writer io.Writer) error {
	return nil
}
