package plumbing

import (
	"io"
	"runtime"
	"strings"
	"sync"
	"time"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/pipeline"
	pkgplumbing "github.com/Sumatoshi-tech/codefang/pkg/plumbing"
	"github.com/go-git/go-git/v6"
	gitplumbing "github.com/go-git/go-git/v6/plumbing"
	"github.com/go-git/go-git/v6/plumbing/object"
	"github.com/go-git/go-git/v6/utils/merkletrie"
	"github.com/sergi/go-diff/diffmatchpatch"
)

type FileDiffAnalyzer struct {
	// Configuration
	CleanupDisabled  bool
	WhitespaceIgnore bool
	Timeout          time.Duration
	Goroutines       int

	// Dependencies
	TreeDiff  *TreeDiffAnalyzer
	BlobCache *BlobCacheAnalyzer

	// Output
	FileDiffs map[string]pkgplumbing.FileDiffData

	// Internal
	l interface {
		Warnf(format string, args ...interface{})
	}
}

const (
	ConfigFileDiffDisableCleanup = "FileDiff.NoCleanup"
	ConfigFileWhitespaceIgnore   = "FileDiff.WhitespaceIgnore"
	ConfigFileDiffTimeout        = "FileDiff.Timeout"
	ConfigFileDiffGoroutines     = "FileDiff.Goroutines"
)

func (f *FileDiffAnalyzer) Name() string {
	return "FileDiff"
}

func (f *FileDiffAnalyzer) Flag() string {
	return "file-diff"
}

func (f *FileDiffAnalyzer) Description() string {
	return "Calculates the difference of files which were modified."
}

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
			Default:     1000},
		{
			Name:        ConfigFileDiffGoroutines,
			Description: "Number of goroutines to use for diff calculation.",
			Flag:        "diff-goroutines",
			Type:        pipeline.IntConfigurationOption,
			Default:     runtime.NumCPU()},
	}
}

func (f *FileDiffAnalyzer) Configure(facts map[string]interface{}) error {
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

func (f *FileDiffAnalyzer) Initialize(repository *git.Repository) error {
	if f.Goroutines <= 0 {
		f.Goroutines = runtime.NumCPU()
	}
	return nil
}

func (f *FileDiffAnalyzer) Consume(ctx *analyze.Context) error {
	result := map[string]pkgplumbing.FileDiffData{}
	cache := f.BlobCache.Cache
	treeDiff := f.TreeDiff.Changes
	
	// If trivial number of changes, don't spin up workers
	// 50 is heuristic
	if len(treeDiff) < 50 || f.Goroutines <= 1 {
		for _, change := range treeDiff {
			if err := f.processChange(change, cache, result, nil); err != nil {
				return err
			}
		}
		f.FileDiffs = result
		return nil
	}

	jobs := make(chan *object.Change, len(treeDiff))
	results := make(chan struct {
		name string
		data pkgplumbing.FileDiffData
		err  error
	}, len(treeDiff))

	wg := sync.WaitGroup{}
	wg.Add(f.Goroutines)

	for i := 0; i < f.Goroutines; i++ {
		go func() {
			defer wg.Done()
			for change := range jobs {
				res := struct {
					name string
					data pkgplumbing.FileDiffData
					err  error
				}{}
				
				tempMap := map[string]pkgplumbing.FileDiffData{}
				if err := f.processChange(change, cache, tempMap, nil); err != nil {
					res.err = err
				} else if len(tempMap) > 0 {
					for k, v := range tempMap {
						res.name = k
						res.data = v
						break
					}
				}
				results <- res
			}
		}()
	}

	for _, change := range treeDiff {
		action, err := change.Action()
		if err != nil {
			return err
		}
		if action == merkletrie.Modify {
			jobs <- change
		}
	}
	close(jobs)
	wg.Wait()
	close(results)

	for res := range results {
		if res.err != nil {
			return res.err
		}
		if res.name != "" {
			result[res.name] = res.data
		}
	}

	f.FileDiffs = result
	return nil
}

func (f *FileDiffAnalyzer) processChange(change *object.Change, cache map[gitplumbing.Hash]*pkgplumbing.CachedBlob, result map[string]pkgplumbing.FileDiffData, mu *sync.Mutex) error {
	action, err := change.Action()
	if err != nil {
		return err
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
		return strings.Replace(str, " ", "", -1)
	}
	return str
}

func (f *FileDiffAnalyzer) Finalize() (analyze.Report, error) {
	return nil, nil
}

func (f *FileDiffAnalyzer) Fork(n int) []analyze.HistoryAnalyzer {
	res := make([]analyze.HistoryAnalyzer, n)
	for i := 0; i < n; i++ {
		clone := *f
		res[i] = &clone
	}
	return res
}

func (f *FileDiffAnalyzer) Merge(branches []analyze.HistoryAnalyzer) {
}

func (f *FileDiffAnalyzer) Serialize(result analyze.Report, binary bool, writer io.Writer) error {
	return nil
}
