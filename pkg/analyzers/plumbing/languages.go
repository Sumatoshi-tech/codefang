package plumbing

import (
	"errors"
	"fmt"
	"io"
	"path"

	"github.com/go-git/go-git/v6"
	gitplumbing "github.com/go-git/go-git/v6/plumbing"
	"github.com/go-git/go-git/v6/utils/merkletrie"
	"github.com/src-d/enry/v2"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/pipeline"
	pkgplumbing "github.com/Sumatoshi-tech/codefang/pkg/plumbing"
)

// LanguagesDetectionAnalyzer detects programming languages of changed files.
type LanguagesDetectionAnalyzer struct {
	// Dependencies.
	TreeDiff  *TreeDiffAnalyzer
	BlobCache *BlobCacheAnalyzer

	// Output.
	Languages map[gitplumbing.Hash]string

	// Internal. //nolint:unused // used via reflection or external caller.
	l interface { //nolint:unused // acknowledged.
		Warnf(format string, args ...any)
	}
}

const (
	// ConfigLanguagesDetection is the configuration key for language detection settings.
	ConfigLanguagesDetection = "LanguagesDetection"
)

// Name returns the name of the analyzer.
func (l *LanguagesDetectionAnalyzer) Name() string {
	return "LanguagesDetection"
}

// Flag returns the CLI flag for the analyzer.
func (l *LanguagesDetectionAnalyzer) Flag() string {
	return "detect-languages"
}

// Description returns a human-readable description of the analyzer.
func (l *LanguagesDetectionAnalyzer) Description() string {
	return "Run programming language detection over the changed files."
}

// ListConfigurationOptions returns the configuration options for the analyzer.
func (l *LanguagesDetectionAnalyzer) ListConfigurationOptions() []pipeline.ConfigurationOption {
	return []pipeline.ConfigurationOption{}
}

// Configure sets up the analyzer with the provided facts.
func (l *LanguagesDetectionAnalyzer) Configure(_ map[string]any) error {
	return nil
}

// Initialize prepares the analyzer for processing commits.
func (l *LanguagesDetectionAnalyzer) Initialize(_ *git.Repository) error {
	return nil
}

// Consume processes a single commit with the provided dependency results.
func (l *LanguagesDetectionAnalyzer) Consume(_ *analyze.Context) error {
	changes := l.TreeDiff.Changes
	cache := l.BlobCache.Cache
	result := map[gitplumbing.Hash]string{}

	for _, change := range changes {
		action, err := change.Action()
		if err != nil {
			return fmt.Errorf("consume: %w", err)
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
	if errors.Is(err, pkgplumbing.ErrBinary) {
		return ""
	}

	lang := enry.GetLanguage(path.Base(name), blob.Data)

	return lang
}

// Finalize completes the analysis and returns the result.
func (l *LanguagesDetectionAnalyzer) Finalize() (analyze.Report, error) {
	return nil, nil //nolint:nilnil // nil,nil return is intentional.
}

// Fork creates a copy of the analyzer for parallel processing.
func (l *LanguagesDetectionAnalyzer) Fork(n int) []analyze.HistoryAnalyzer {
	res := make([]analyze.HistoryAnalyzer, n)
	for i := range n {
		clone := *l
		res[i] = &clone
	}

	return res
}

// Merge combines results from forked analyzer branches.
func (l *LanguagesDetectionAnalyzer) Merge(_ []analyze.HistoryAnalyzer) {
}

// Serialize writes the analysis result to the given writer.
func (l *LanguagesDetectionAnalyzer) Serialize(_ analyze.Report, _ bool, _ io.Writer) error {
	return nil
}
