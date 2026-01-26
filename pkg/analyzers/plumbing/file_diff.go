package plumbing

import (
	"fmt"
	"io"
	"runtime"
	"strings"
	"sync"
	"time"

	"github.com/go-git/go-git/v6"
	gitplumbing "github.com/go-git/go-git/v6/plumbing"
	"github.com/go-git/go-git/v6/plumbing/object"
	"github.com/go-git/go-git/v6/utils/merkletrie"
	"github.com/sergi/go-diff/diffmatchpatch"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/pipeline"
	pkgplumbing "github.com/Sumatoshi-tech/codefang/pkg/plumbing"
)

// DefaultValue is the default timeout in milliseconds for file diff operations.
const (
	DefaultValue = 1000
)

// FileDiffAnalyzer computes file-level diffs for each commit.
type FileDiffAnalyzer struct {
	l interface { //nolint:unused // used via dependency injection.
		Warnf(format string, args ...any)
	}
	TreeDiff         *TreeDiffAnalyzer
	BlobCache        *BlobCacheAnalyzer
	FileDiffs        map[string]pkgplumbing.FileDiffData
	Timeout          time.Duration
	Goroutines       int
	CleanupDisabled  bool
	WhitespaceIgnore bool
}

const (
	// ConfigFileDiffDisableCleanup is the configuration key for disabling diff cleanup.
	ConfigFileDiffDisableCleanup = "FileDiff.NoCleanup"
	// ConfigFileWhitespaceIgnore is the configuration key for ignoring whitespace in diffs.
	ConfigFileWhitespaceIgnore = "FileDiff.WhitespaceIgnore"
	// ConfigFileDiffTimeout is the configuration key for the diff computation timeout.
	ConfigFileDiffTimeout = "FileDiff.Timeout"
	// ConfigFileDiffGoroutines is the configuration key for the number of parallel diff goroutines.
	ConfigFileDiffGoroutines = "FileDiff.Goroutines"
)

// Name returns the name of the analyzer.
func (f *FileDiffAnalyzer) Name() string {
	return "FileDiff"
}

// Flag returns the CLI flag for the analyzer.
func (f *FileDiffAnalyzer) Flag() string {
	return "file-diff"
}

// Description returns a human-readable description of the analyzer.
func (f *FileDiffAnalyzer) Description() string {
	return "Calculates the difference of files which were modified."
}

// ListConfigurationOptions returns the configuration options for the analyzer.
func (f *FileDiffAnalyzer) ListConfigurationOptions() []pipeline.ConfigurationOption {
	return []pipeline.ConfigurationOption{
		{
			Name:        ConfigFileDiffDisableCleanup,
			Description: "Do not apply additional heuristics to improve diffs.",
			Flag:        "no-diff-cleanup",
			Type:        pipeline.BoolConfigurationOption,
			Default:     false},
		{
			Name:        ConfigFileWhitespaceIgnore,
			Description: "Ignore whitespace when computing diffs.",
			Flag:        "no-diff-whitespace",
			Type:        pipeline.BoolConfigurationOption,
			Default:     false},
		{
			Name:        ConfigFileDiffTimeout,
			Description: "Maximum time in milliseconds a single diff calculation may elapse.",
			Flag:        "diff-timeout",
			Type:        pipeline.IntConfigurationOption,
			Default:     DefaultValue},
		{
			Name:        ConfigFileDiffGoroutines,
			Description: "Number of goroutines to use for diff calculation.",
			Flag:        "diff-goroutines",
			Type:        pipeline.IntConfigurationOption,
			Default:     runtime.NumCPU()},
	}
}

// Configure sets up the analyzer with the provided facts.
func (f *FileDiffAnalyzer) Configure(facts map[string]any) error {
	if val, exists := facts[ConfigFileDiffDisableCleanup].(bool); exists {
		f.CleanupDisabled = val
	}

	if val, exists := facts[ConfigFileWhitespaceIgnore].(bool); exists {
		f.WhitespaceIgnore = val
	}

	if val, exists := facts[ConfigFileDiffTimeout].(int); exists {
		f.Timeout = time.Duration(val) * time.Millisecond
	}

	if val, exists := facts[ConfigFileDiffGoroutines].(int); exists {
		f.Goroutines = val
	}

	return nil
}

// Initialize prepares the analyzer for processing commits.
func (f *FileDiffAnalyzer) Initialize(_ *git.Repository) error {
	if f.Goroutines <= 0 {
		f.Goroutines = runtime.NumCPU()
	}

	return nil
}

// parallelThreshold is the minimum number of changes to justify spawning worker goroutines.
const parallelThreshold = 50

// Consume processes a single commit with the provided dependency results.
func (f *FileDiffAnalyzer) Consume(_ *analyze.Context) error {
	cache := f.BlobCache.Cache
	treeDiff := f.TreeDiff.Changes

	if len(treeDiff) < parallelThreshold || f.Goroutines <= 1 {
		result, err := f.processChangesSequential(treeDiff, cache)
		if err != nil {
			return err
		}

		f.FileDiffs = result

		return nil
	}

	result, err := f.processChangesParallel(treeDiff, cache)
	if err != nil {
		return err
	}

	f.FileDiffs = result

	return nil
}

func (f *FileDiffAnalyzer) processChangesSequential(
	treeDiff object.Changes, cache map[gitplumbing.Hash]*pkgplumbing.CachedBlob,
) (map[string]pkgplumbing.FileDiffData, error) {
	result := map[string]pkgplumbing.FileDiffData{}

	for _, change := range treeDiff {
		err := f.processChange(change, cache, result, nil)
		if err != nil {
			return nil, err
		}
	}

	return result, nil
}

type fileDiffResult struct {
	err  error
	name string
	data pkgplumbing.FileDiffData
}

func (f *FileDiffAnalyzer) processChangesParallel(
	treeDiff object.Changes, cache map[gitplumbing.Hash]*pkgplumbing.CachedBlob,
) (map[string]pkgplumbing.FileDiffData, error) {
	jobs := make(chan *object.Change, len(treeDiff))
	results := make(chan fileDiffResult, len(treeDiff))

	wg := sync.WaitGroup{}
	wg.Add(f.Goroutines)

	for range f.Goroutines {
		go func() {
			defer wg.Done()

			for change := range jobs {
				results <- f.processOneChange(change, cache)
			}
		}()
	}

	for _, change := range treeDiff {
		action, err := change.Action()
		if err != nil {
			return nil, fmt.Errorf("consume: %w", err)
		}

		if action == merkletrie.Modify {
			jobs <- change
		}
	}

	close(jobs)
	wg.Wait()
	close(results)

	return collectFileDiffResults(results)
}

func (f *FileDiffAnalyzer) processOneChange(
	change *object.Change, cache map[gitplumbing.Hash]*pkgplumbing.CachedBlob,
) fileDiffResult {
	tempMap := map[string]pkgplumbing.FileDiffData{}

	err := f.processChange(change, cache, tempMap, nil)
	if err != nil {
		return fileDiffResult{err: err}
	}

	for k, v := range tempMap {
		return fileDiffResult{name: k, data: v}
	}

	return fileDiffResult{}
}

func collectFileDiffResults(results chan fileDiffResult) (map[string]pkgplumbing.FileDiffData, error) {
	result := map[string]pkgplumbing.FileDiffData{}

	for res := range results {
		if res.err != nil {
			return nil, res.err
		}

		if res.name != "" {
			result[res.name] = res.data
		}
	}

	return result, nil
}

func (f *FileDiffAnalyzer) processChange(
	change *object.Change,
	cache map[gitplumbing.Hash]*pkgplumbing.CachedBlob,
	result map[string]pkgplumbing.FileDiffData,
	mu *sync.Mutex,
) error {
	action, err := change.Action()
	if err != nil {
		return fmt.Errorf("processChange: %w", err)
	}

	if action != merkletrie.Modify {
		return nil
	}

	blobFrom := cache[change.From.TreeEntry.Hash]
	blobTo := cache[change.To.TreeEntry.Hash]
	strFrom, strTo := string(blobFrom.Data), string(blobTo.Data)
	dmp := diffmatchpatch.New()
	dmp.DiffTimeout = f.Timeout
	src, dst, _ := dmp.DiffLinesToRunes(stripWhitespace(strFrom, f.WhitespaceIgnore), stripWhitespace(strTo, f.WhitespaceIgnore))

	diffs := dmp.DiffMainRunes(src, dst, false)
	if !f.CleanupDisabled {
		diffs = dmp.DiffCleanupMerge(dmp.DiffCleanupSemanticLossless(diffs))
	}

	data := pkgplumbing.FileDiffData{
		OldLinesOfCode: len(src),
		NewLinesOfCode: len(dst),
		Diffs:          diffs,
	}

	if mu != nil {
		mu.Lock()

		result[change.To.Name] = data

		mu.Unlock()
	} else {
		result[change.To.Name] = data
	}

	return nil
}

func stripWhitespace(str string, ignoreWhitespace bool) string {
	if ignoreWhitespace {
		return strings.ReplaceAll(str, " ", "")
	}

	return str
}

// Finalize completes the analysis and returns the result.
func (f *FileDiffAnalyzer) Finalize() (analyze.Report, error) {
	return nil, nil //nolint:nilnil // nil,nil return is intentional.
}

// Fork creates a copy of the analyzer for parallel processing.
func (f *FileDiffAnalyzer) Fork(n int) []analyze.HistoryAnalyzer {
	res := make([]analyze.HistoryAnalyzer, n)
	for i := range n {
		clone := *f
		res[i] = &clone
	}

	return res
}

// Merge combines results from forked analyzer branches.
func (f *FileDiffAnalyzer) Merge(_ []analyze.HistoryAnalyzer) {
}

// Serialize writes the analysis result to the given writer.
func (f *FileDiffAnalyzer) Serialize(_ analyze.Report, _ bool, _ io.Writer) error {
	return nil
}
