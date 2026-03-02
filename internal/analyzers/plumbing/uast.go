package plumbing

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"iter"
	"os"
	"runtime"
	"sync"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
	"github.com/Sumatoshi-tech/codefang/pkg/pipeline"
	"github.com/Sumatoshi-tech/codefang/pkg/uast"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

// UASTChangesAnalyzer extracts UAST-level changes between commits.
// It uses lazy parsing - changes are only parsed when Changes() is called.
type UASTChangesAnalyzer struct {
	TreeDiff   *TreeDiffAnalyzer
	BlobCache  *BlobCacheAnalyzer
	Goroutines int
	parser     *uast.Parser
	changes    []uast.Change
	parsed     bool   // tracks whether parsing was done for current commit.
	spillPath  string // path to spill file from current commit (for cleanup on next Consume).
}

const (
	// FeatureUast is the feature flag for UAST-based analysis.
	FeatureUast = "uast"

	// ConfigUASTChangesGoroutines is the configuration key for parallel UAST parsing.
	ConfigUASTChangesGoroutines = "UASTChanges.Goroutines"

	// defaultGoroutineDivisor is used to derive default goroutine count from NumCPU.
	defaultGoroutineDivisor = 4
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
	return c.Descriptor().Description
}

// Descriptor returns stable analyzer metadata.
func (c *UASTChangesAnalyzer) Descriptor() analyze.Descriptor {
	return analyze.NewDescriptor(
		analyze.ModeHistory,
		c.Name(),
		"Extracts UAST changes from file changes in commits.",
	)
}

// ListConfigurationOptions returns the configuration options for the analyzer.
func (c *UASTChangesAnalyzer) ListConfigurationOptions() []pipeline.ConfigurationOption {
	return []pipeline.ConfigurationOption{
		{
			Name:        ConfigUASTChangesGoroutines,
			Description: "Number of goroutines to use for parallel UAST parsing (fallback when pipeline is not available).",
			Flag:        "uast-changes-goroutines",
			Type:        pipeline.IntConfigurationOption,
			Default:     max(runtime.NumCPU()/defaultGoroutineDivisor, 1),
		},
	}
}

// Configure sets up the analyzer with the provided facts.
func (c *UASTChangesAnalyzer) Configure(facts map[string]any) error {
	if val, exists := facts[ConfigUASTChangesGoroutines].(int); exists {
		c.Goroutines = val
	}

	return nil
}

// Initialize prepares the analyzer for processing commits.
func (c *UASTChangesAnalyzer) Initialize(_ *gitlib.Repository) error {
	parser, err := uast.NewParser()
	if err != nil {
		return fmt.Errorf("failed to initialize UAST parser: %w", err)
	}

	c.parser = parser

	if c.Goroutines <= 0 {
		c.Goroutines = max(runtime.NumCPU()/defaultGoroutineDivisor, 1)
	}

	return nil
}

// Flush releases internal UAST trees and temporary files.
func (c *UASTChangesAnalyzer) Flush() {
	// Release previous commit's UAST trees back to pools for reuse.
	for _, ch := range c.changes {
		node.ReleaseTree(ch.Before)
		node.ReleaseTree(ch.After)
	}

	c.changes = nil
	c.parsed = false

	// Cleanup previous commit's spill file.
	if c.spillPath != "" {
		os.Remove(c.spillPath)
		c.spillPath = ""
	}
}

// Consume resets state for the new commit. Parsing is deferred until Changes() is called.
// Releases previous commit's UAST trees back to the node/positions pools.
// If the context contains pre-computed UAST changes from the pipeline, they are used directly.
// If the context contains a spill path, the changes are deserialized eagerly from disk
// to avoid race conditions when Fork() creates clones sharing this analyzer.
func (c *UASTChangesAnalyzer) Consume(_ context.Context, ac *analyze.Context) (analyze.TC, error) {
	c.Flush()

	switch {
	case ac.UASTChanges != nil:
		// Use pre-computed UAST changes from the pipeline if available.
		c.changes = ac.UASTChanges
		c.parsed = true
	case ac.UASTSpillPath != "":
		// Spilled UAST changes (large commits) — store the path to stream lazily.
		c.spillPath = ac.UASTSpillPath
		c.parsed = true
	default:
		c.changes = nil
		c.parsed = false
	}

	return analyze.TC{}, nil
}

// Changes returns parsed UAST changes, parsing lazily on first call per commit.
// This avoids expensive UAST parsing when downstream analyzers don't need it.
func (c *UASTChangesAnalyzer) Changes(ctx context.Context) iter.Seq[uast.Change] {
	if c.spillPath != "" {
		return analyze.StreamUASTChanges(c.spillPath, c.TreeDiff.Changes)
	}

	if !c.parsed {
		c.parsed = true
		treeChanges := c.TreeDiff.Changes
		cache := c.BlobCache.Cache

		if len(treeChanges) <= 1 || c.Goroutines <= 1 {
			c.changes = c.changesSequential(ctx, treeChanges, cache)
		} else {
			c.changes = c.changesParallel(ctx, treeChanges, cache)
		}
	}

	return func(yield func(uast.Change) bool) {
		for _, ch := range c.changes {
			if !yield(ch) {
				break
			}
		}
	}
}

// changesSequential parses UAST changes one file at a time.
func (c *UASTChangesAnalyzer) changesSequential(
	ctx context.Context,
	treeChanges gitlib.Changes,
	cache map[gitlib.Hash]*gitlib.CachedBlob,
) []uast.Change {
	var result []uast.Change

	for _, change := range treeChanges {
		before := c.parseBeforeVersion(ctx, change, cache)
		after := c.parseAfterVersion(ctx, change, cache)

		if before != nil || after != nil {
			result = append(result, uast.Change{
				Before: before,
				After:  after,
				Change: change,
			})
		}
	}

	return result
}

// uastParseResult holds the result of parsing a single file change.
type uastParseResult struct {
	before *node.Node
	after  *node.Node
	change *gitlib.Change
}

// changesParallel parses UAST changes across multiple goroutines.
// Each file's before/after parsing is independent and thread-safe.
func (c *UASTChangesAnalyzer) changesParallel(
	ctx context.Context,
	treeChanges gitlib.Changes,
	cache map[gitlib.Hash]*gitlib.CachedBlob,
) []uast.Change {
	jobs := make(chan *gitlib.Change, len(treeChanges))
	results := make(chan uastParseResult, len(treeChanges))

	var wg sync.WaitGroup

	wg.Add(c.Goroutines)

	for range c.Goroutines {
		go func() {
			defer wg.Done()

			for change := range jobs {
				before := c.parseBeforeVersion(ctx, change, cache)
				after := c.parseAfterVersion(ctx, change, cache)

				if before != nil || after != nil {
					results <- uastParseResult{before, after, change}
				}
			}
		}()
	}

	for _, change := range treeChanges {
		jobs <- change
	}

	close(jobs)
	wg.Wait()
	close(results)

	var changes []uast.Change

	for r := range results {
		changes = append(changes, uast.Change{
			Before: r.before,
			After:  r.after,
			Change: r.change,
		})
	}

	return changes
}

// parseBeforeVersion parses the "before" version for modifications and deletions.
func (c *UASTChangesAnalyzer) parseBeforeVersion(
	ctx context.Context,
	change *gitlib.Change,
	cache map[gitlib.Hash]*gitlib.CachedBlob,
) *node.Node {
	if change.Action != gitlib.Modify && change.Action != gitlib.Delete {
		return nil
	}

	return c.parseBlob(ctx, change.From.Hash, change.From.Name, cache)
}

// parseAfterVersion parses the "after" version for modifications and insertions.
func (c *UASTChangesAnalyzer) parseAfterVersion(
	ctx context.Context,
	change *gitlib.Change,
	cache map[gitlib.Hash]*gitlib.CachedBlob,
) *node.Node {
	if change.Action != gitlib.Modify && change.Action != gitlib.Insert {
		return nil
	}

	return c.parseBlob(ctx, change.To.Hash, change.To.Name, cache)
}

// maxUASTBlobSize is the maximum blob size (in bytes) for UAST parsing.
// Files larger than this are skipped — they are typically generated code
// whose tree-sitter parse trees consume hundreds of MB of CGO memory.
const maxUASTBlobSize = 256 * 1024 // 256 KiB.

// parseBlob parses a blob into a UAST node if the file is supported.
func (c *UASTChangesAnalyzer) parseBlob(
	ctx context.Context,
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

	if len(blob.Data) > maxUASTBlobSize {
		return nil
	}

	parsed, err := c.parser.Parse(ctx, filename, blob.Data)
	if err != nil {
		return nil
	}

	return parsed
}

// SetChanges sets the changes directly, marking them as parsed.
func (c *UASTChangesAnalyzer) SetChanges(changes []uast.Change) {
	c.changes = changes
	c.parsed = true
}

// SetChangesForTest sets the changes directly (for testing only).
func (c *UASTChangesAnalyzer) SetChangesForTest(changes []uast.Change) {
	c.SetChanges(changes)
}

// TransferChanges returns the current changes and clears the internal reference
// without releasing the UAST trees. The caller takes ownership of the returned
// changes and is responsible for releasing them.
func (c *UASTChangesAnalyzer) TransferChanges() []uast.Change {
	ch := c.changes
	c.changes = nil

	return ch
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

// GetLanguage returns the language name for the given filename, or empty string if unsupported.
// This delegates to the underlying parser without performing any tree-sitter parsing.
func (c *UASTChangesAnalyzer) GetLanguage(filename string) string {
	if c.parser == nil {
		return ""
	}

	return c.parser.GetLanguage(filename)
}

// WorkingStateSize returns 0 — plumbing analyzers are excluded from budget planning.
func (c *UASTChangesAnalyzer) WorkingStateSize() int64 { return 0 }

// AvgTCSize returns 0 — plumbing analyzers do not emit meaningful TC payloads.
func (c *UASTChangesAnalyzer) AvgTCSize() int64 { return 0 }

// NewAggregator returns nil — plumbing analyzers do not aggregate.
func (c *UASTChangesAnalyzer) NewAggregator(_ analyze.AggregatorOptions) analyze.Aggregator {
	return nil
}

// SerializeTICKs returns ErrNotImplemented — plumbing analyzers do not produce TICKs.
func (c *UASTChangesAnalyzer) SerializeTICKs(_ []analyze.TICK, _ string, _ io.Writer) error {
	return analyze.ErrNotImplemented
}

// ReportFromTICKs returns ErrNotImplemented — plumbing analyzers do not produce reports.
func (c *UASTChangesAnalyzer) ReportFromTICKs(_ context.Context, _ []analyze.TICK) (analyze.Report, error) {
	return nil, analyze.ErrNotImplemented
}
