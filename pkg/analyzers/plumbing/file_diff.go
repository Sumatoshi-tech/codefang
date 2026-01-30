package plumbing

import (
	"encoding/json"
	"fmt"
	"io"
	"runtime"
	"strings"
	"sync"
	"time"

	"github.com/sergi/go-diff/diffmatchpatch"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
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
func (f *FileDiffAnalyzer) Initialize(_ *gitlib.Repository) error {
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
		result := f.processChangesSequential(treeDiff, cache)
		f.FileDiffs = result

		return nil
	}

	result := f.processChangesParallel(treeDiff, cache)
	f.FileDiffs = result

	return nil
}

func (f *FileDiffAnalyzer) processChangesSequential(
	treeDiff gitlib.Changes, cache map[gitlib.Hash]*gitlib.CachedBlob,
) map[string]pkgplumbing.FileDiffData {
	result := map[string]pkgplumbing.FileDiffData{}

	for _, change := range treeDiff {
		f.processChange(change, cache, result, nil)
	}

	return result
}

type fileDiffResult struct {
	name string
	data pkgplumbing.FileDiffData
}

func (f *FileDiffAnalyzer) processChangesParallel(
	treeDiff gitlib.Changes, cache map[gitlib.Hash]*gitlib.CachedBlob,
) map[string]pkgplumbing.FileDiffData {
	jobs := make(chan *gitlib.Change, len(treeDiff))
	results := make(chan fileDiffResult, len(treeDiff))

	wg := sync.WaitGroup{}
	wg.Add(f.Goroutines)

	for range f.Goroutines {
		go func() {
			defer wg.Done()

			for change := range jobs {
				res := f.processOneChange(change, cache)
				if res.name != "" {
					results <- res
				}
			}
		}()
	}

	for _, change := range treeDiff {
		if change.Action == gitlib.Modify {
			jobs <- change
		}
	}

	close(jobs)
	wg.Wait()
	close(results)

	return collectFileDiffResults(results)
}

func (f *FileDiffAnalyzer) processOneChange(
	change *gitlib.Change, cache map[gitlib.Hash]*gitlib.CachedBlob,
) fileDiffResult {
	tempMap := map[string]pkgplumbing.FileDiffData{}

	f.processChange(change, cache, tempMap, nil)

	for k, v := range tempMap {
		return fileDiffResult{name: k, data: v}
	}

	return fileDiffResult{}
}

func collectFileDiffResults(results chan fileDiffResult) map[string]pkgplumbing.FileDiffData {
	result := map[string]pkgplumbing.FileDiffData{}

	for res := range results {
		if res.name != "" {
			result[res.name] = res.data
		}
	}

	return result
}

func (f *FileDiffAnalyzer) processChange(
	change *gitlib.Change,
	cache map[gitlib.Hash]*gitlib.CachedBlob,
	result map[string]pkgplumbing.FileDiffData,
	mu *sync.Mutex,
) {
	if change.Action != gitlib.Modify {
		return
	}

	blobFrom := cache[change.From.Hash]
	blobTo := cache[change.To.Hash]

	if blobFrom == nil || blobTo == nil {
		return
	}

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
func (f *FileDiffAnalyzer) Serialize(report analyze.Report, format string, writer io.Writer) error {
	if format == analyze.FormatJSON {
		err := json.NewEncoder(writer).Encode(report)
		if err != nil {
			return fmt.Errorf("json encode: %w", err)
		}
	}

	return nil
}
