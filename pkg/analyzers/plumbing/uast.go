package plumbing

import (
	"encoding/json"
	"fmt"
	"io"
	"runtime"
	"sync"

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
	TreeDiff   *TreeDiffAnalyzer
	BlobCache  *BlobCacheAnalyzer
	Goroutines int
	parser     *uast.Parser
	changes    []uast.Change
	parsed     bool // tracks whether parsing was done for current commit
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

// Consume resets state for the new commit. Parsing is deferred until Changes() is called.
// Releases previous commit's UAST trees back to the node/positions pools.
// If the context contains pre-computed UAST changes from the pipeline, they are used directly.
func (c *UASTChangesAnalyzer) Consume(ctx *analyze.Context) error {
	// Release previous commit's UAST trees back to pools for reuse.
	for _, ch := range c.changes {
		node.ReleaseTree(ch.Before)
		node.ReleaseTree(ch.After)
	}

	// Use pre-computed UAST changes from the pipeline if available.
	if ctx.UASTChanges != nil {
		c.changes = ctx.UASTChanges
		c.parsed = true
	} else {
		c.changes = nil
		c.parsed = false
	}

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

	if len(treeChanges) <= 1 || c.Goroutines <= 1 {
		c.changes = c.changesSequential(treeChanges, cache)
	} else {
		c.changes = c.changesParallel(treeChanges, cache)
	}

	return c.changes
}

// changesSequential parses UAST changes one file at a time.
func (c *UASTChangesAnalyzer) changesSequential(
	treeChanges gitlib.Changes,
	cache map[gitlib.Hash]*gitlib.CachedBlob,
) []uast.Change {
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
				before := c.parseBeforeVersion(change, cache)
				after := c.parseAfterVersion(change, cache)

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
