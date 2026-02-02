package plumbing

import (
	"encoding/json"
	"fmt"
	"io"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
	"github.com/Sumatoshi-tech/codefang/pkg/pipeline"
	"github.com/Sumatoshi-tech/codefang/pkg/uast"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

// UASTChangesAnalyzer extracts UAST-level changes between commits.
// It uses lazy parsing - changes are only parsed when Changes() is called.
type UASTChangesAnalyzer struct {
	l interface { //nolint:unused // used via dependency injection.
		Warnf(format string, args ...any)
	}
	TreeDiff  *TreeDiffAnalyzer
	BlobCache *BlobCacheAnalyzer
	parser    *uast.Parser
	changes   []uast.Change
	parsed    bool // tracks whether parsing was done for current commit
}

const (
	// FeatureUast is the feature flag for UAST-based analysis.
	FeatureUast = "uast"
)

// Name returns the name of the analyzer.
func (c *UASTChangesAnalyzer) Name() string {
	return "UASTChanges"
}

// Flag returns the CLI flag for the analyzer.
func (c *UASTChangesAnalyzer) Flag() string {
	return "uast-changes"
}

// Description returns a human-readable description of the analyzer.
func (c *UASTChangesAnalyzer) Description() string {
	return "Extracts UAST changes from file changes in commits."
}

// ListConfigurationOptions returns the configuration options for the analyzer.
func (c *UASTChangesAnalyzer) ListConfigurationOptions() []pipeline.ConfigurationOption {
	return []pipeline.ConfigurationOption{}
}

// Configure sets up the analyzer with the provided facts.
func (c *UASTChangesAnalyzer) Configure(_ map[string]any) error {
	return nil
}

// Initialize prepares the analyzer for processing commits.
func (c *UASTChangesAnalyzer) Initialize(_ *gitlib.Repository) error {
	parser, err := uast.NewParser()
	if err != nil {
		return fmt.Errorf("failed to initialize UAST parser: %w", err)
	}

	c.parser = parser

	return nil
}

// Consume resets state for the new commit. Parsing is deferred until Changes() is called.
func (c *UASTChangesAnalyzer) Consume(_ *analyze.Context) error {
	// Reset state for new commit - parsing is lazy
	c.changes = nil
	c.parsed = false

	return nil
}

// Changes returns parsed UAST changes, parsing lazily on first call per commit.
// This avoids expensive UAST parsing when downstream analyzers don't need it.
func (c *UASTChangesAnalyzer) Changes() []uast.Change {
	if c.parsed {
		return c.changes
	}

	c.parsed = true
	treeChanges := c.TreeDiff.Changes
	cache := c.BlobCache.Cache

	var result []uast.Change

	for _, change := range treeChanges {
		before := c.parseBeforeVersion(change, cache)
		after := c.parseAfterVersion(change, cache)

		if before != nil || after != nil {
			result = append(result, uast.Change{
				Before: before,
				After:  after,
				Change: change,
			})
		}
	}

	c.changes = result

	return c.changes
}

// parseBeforeVersion parses the "before" version for modifications and deletions.
func (c *UASTChangesAnalyzer) parseBeforeVersion(
	change *gitlib.Change,
	cache map[gitlib.Hash]*gitlib.CachedBlob,
) *node.Node {
	if change.Action != gitlib.Modify && change.Action != gitlib.Delete {
		return nil
	}

	return c.parseBlob(change.From.Hash, change.From.Name, cache)
}

// parseAfterVersion parses the "after" version for modifications and insertions.
func (c *UASTChangesAnalyzer) parseAfterVersion(
	change *gitlib.Change,
	cache map[gitlib.Hash]*gitlib.CachedBlob,
) *node.Node {
	if change.Action != gitlib.Modify && change.Action != gitlib.Insert {
		return nil
	}

	return c.parseBlob(change.To.Hash, change.To.Name, cache)
}

// parseBlob parses a blob into a UAST node if the file is supported.
func (c *UASTChangesAnalyzer) parseBlob(
	hash gitlib.Hash,
	filename string,
	cache map[gitlib.Hash]*gitlib.CachedBlob,
) *node.Node {
	blob, ok := cache[hash]
	if !ok {
		return nil
	}

	if !c.parser.IsSupported(filename) {
		return nil
	}

	parsed, err := c.parser.Parse(filename, blob.Data)
	if err != nil {
		return nil
	}

	return parsed
}

// SetChangesForTest sets the changes directly (for testing only).
func (c *UASTChangesAnalyzer) SetChangesForTest(changes []uast.Change) {
	c.changes = changes
	c.parsed = true
}

// Finalize completes the analysis and returns the result.
func (c *UASTChangesAnalyzer) Finalize() (analyze.Report, error) {
	return nil, nil //nolint:nilnil // nil,nil return is intentional.
}

// Fork creates a copy of the analyzer for parallel processing.
func (c *UASTChangesAnalyzer) Fork(n int) []analyze.HistoryAnalyzer {
	res := make([]analyze.HistoryAnalyzer, n)
	for i := range n {
		clone := *c
		// Parser is likely thread safe or reusable.
		res[i] = &clone
	}

	return res
}

// Merge combines results from forked analyzer branches.
func (c *UASTChangesAnalyzer) Merge(_ []analyze.HistoryAnalyzer) {
}

// Serialize writes the analysis result to the given writer.
func (c *UASTChangesAnalyzer) Serialize(report analyze.Report, format string, writer io.Writer) error {
	if format == analyze.FormatJSON {
		err := json.NewEncoder(writer).Encode(report)
		if err != nil {
			return fmt.Errorf("json encode: %w", err)
		}
	}

	return nil
}
