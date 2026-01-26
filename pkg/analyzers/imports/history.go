package imports

import (
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"sort"
	"sync"
	"time"

	"github.com/go-git/go-git/v6"
	gitplumbing "github.com/go-git/go-git/v6/plumbing"
	"github.com/go-git/go-git/v6/plumbing/object"
	"github.com/go-git/go-git/v6/utils/merkletrie"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/identity"
	"github.com/Sumatoshi-tech/codefang/pkg/importmodel"
	"github.com/Sumatoshi-tech/codefang/pkg/pipeline"
	pkgplumbing "github.com/Sumatoshi-tech/codefang/pkg/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/uast"
)

const (
	defaultGoroutines       = 4
	defaultMaxFileSizeShift = 20
	defaultTickHours        = 24
)

// ImportsMap maps file paths to their import lists.
type ImportsMap = map[int]map[string]map[string]map[int]int64

// ImportsHistoryAnalyzer tracks import usage across commit history.
type ImportsHistoryAnalyzer struct {
	l interface { //nolint:unused // used via dependency injection.
		Warnf(format string, args ...any)
		Errorf(format string, args ...any)
	}
	TreeDiff           *plumbing.TreeDiffAnalyzer
	BlobCache          *plumbing.BlobCacheAnalyzer
	Identity           *plumbing.IdentityDetector
	Ticks              *plumbing.TicksSinceStart
	imports            ImportsMap
	parser             *uast.Parser
	reversedPeopleDict []string
	TickSize           time.Duration
	Goroutines         int
	MaxFileSize        int
}

// Name returns the name of the analyzer.
func (h *ImportsHistoryAnalyzer) Name() string {
	return "ImportsPerDeveloper"
}

// Flag returns the CLI flag for the analyzer.
func (h *ImportsHistoryAnalyzer) Flag() string {
	return "imports-per-dev"
}

// Description returns a human-readable description of the analyzer.
func (h *ImportsHistoryAnalyzer) Description() string {
	return "Whenever a file is changed or added, we extract the imports from it and increment their usage for the commit author."
}

// ListConfigurationOptions returns the configuration options for the analyzer.
func (h *ImportsHistoryAnalyzer) ListConfigurationOptions() []pipeline.ConfigurationOption {
	return []pipeline.ConfigurationOption{
		{
			Name:        "Imports.Goroutines",
			Description: "Specifies the number of goroutines to run in parallel for the imports extraction.",
			Flag:        "import-goroutines",
			Type:        pipeline.IntConfigurationOption,
			Default:     defaultGoroutines,
		},
		{
			Name:        "Imports.MaxFileSize",
			Description: "Specifies the file size threshold. Files that exceed it are ignored.",
			Flag:        "import-max-file-size",
			Type:        pipeline.IntConfigurationOption,
			Default:     1 << defaultMaxFileSizeShift,
		},
	}
}

// Configure sets up the analyzer with the provided facts.
func (h *ImportsHistoryAnalyzer) Configure(facts map[string]any) error {
	if val, exists := facts[identity.FactIdentityDetectorReversedPeopleDict].([]string); exists {
		h.reversedPeopleDict = val
	}

	if val, exists := facts[pkgplumbing.FactTickSize].(time.Duration); exists {
		h.TickSize = val
	}

	if val, exists := facts["Imports.Goroutines"].(int); exists {
		h.Goroutines = val
	}

	if val, exists := facts["Imports.MaxFileSize"].(int); exists {
		h.MaxFileSize = val
	}

	return nil
}

// Initialize prepares the analyzer for processing commits.
func (h *ImportsHistoryAnalyzer) Initialize(_ *git.Repository) error {
	h.imports = ImportsMap{}
	if h.TickSize == 0 {
		h.TickSize = time.Hour * defaultTickHours
	}

	if h.Goroutines < 1 {
		h.Goroutines = defaultGoroutines
	}

	if h.MaxFileSize == 0 {
		h.MaxFileSize = 1 << defaultMaxFileSizeShift
	}

	// Initialize UAST parser.
	var err error

	h.parser, err = uast.NewParser()
	if err != nil {
		return fmt.Errorf("failed to initialize UAST parser: %w", err)
	}

	return nil
}

func (h *ImportsHistoryAnalyzer) extractImports(name string, data []byte) (*importmodel.File, error) {
	if h.parser == nil {
		return nil, errors.New("parser not initialized") //nolint:err113 // simple guard, no sentinel needed
	}

	// Check if supported.
	if !h.parser.IsSupported(name) {
		return nil, fmt.Errorf("unsupported language for %s", name) //nolint:err113 // dynamic error is acceptable here.
	}

	// Parse.
	root, err := h.parser.Parse(name, data)
	if err != nil {
		return nil, err
	}

	// Extract imports using logic from analyzer.go (in same package).
	imports := extractImportsFromUAST(root)

	// Determine language.
	lang := h.parser.GetLanguage(name)
	if lang == "" {
		lang = "uast"
	}

	return &importmodel.File{
		Lang:    lang,
		Imports: imports,
	}, nil
}

// extractImportsParallel spins up a worker pool to parse changed files in
// parallel and returns per-blob import results.
//
//nolint:gocognit // complexity is inherent to parallel worker pool with mutex coordination.
func (h *ImportsHistoryAnalyzer) extractImportsParallel(
	changes object.Changes,
	cache map[gitplumbing.Hash]*pkgplumbing.CachedBlob,
) map[gitplumbing.Hash]importmodel.File {
	extracted := map[gitplumbing.Hash]importmodel.File{}
	jobs := make(chan *object.Change, h.Goroutines)

	var (
		mu sync.Mutex
		wg sync.WaitGroup
	)

	wg.Add(h.Goroutines)

	for range h.Goroutines {
		go func() {
			defer wg.Done()

			for change := range jobs {
				blob := cache[change.To.TreeEntry.Hash]
				if blob.Size > int64(h.MaxFileSize) {
					continue
				}

				file, err := h.extractImports(change.To.TreeEntry.Name, blob.Data)
				if err == nil {
					mu.Lock()

					extracted[change.To.TreeEntry.Hash] = *file

					mu.Unlock()
				}
			}
		}()
	}

	for _, change := range changes {
		action, err := change.Action()
		if err != nil {
			continue
		}

		switch action {
		case merkletrie.Modify, merkletrie.Insert:
			jobs <- change
		case merkletrie.Delete:
			continue
		}
	}

	close(jobs)
	wg.Wait()

	return extracted
}

// aggregateImports folds the per-blob import data into the analyzer's
// cumulative imports map, keyed by author and tick.
func (h *ImportsHistoryAnalyzer) aggregateImports(
	extractedImports map[gitplumbing.Hash]importmodel.File,
	author, tick int,
) {
	aimps := h.imports[author]
	if aimps == nil {
		aimps = map[string]map[string]map[int]int64{}
		h.imports[author] = aimps
	}

	for _, file := range extractedImports {
		limps := aimps[file.Lang]
		if limps == nil {
			limps = map[string]map[int]int64{}
			aimps[file.Lang] = limps
		}

		for _, imp := range file.Imports {
			timps, exists := limps[imp]
			if !exists {
				timps = map[int]int64{}
				limps[imp] = timps
			}

			timps[tick]++
		}
	}
}

// Consume processes a single commit with the provided dependency results.
func (h *ImportsHistoryAnalyzer) Consume(_ *analyze.Context) error {
	extracted := h.extractImportsParallel(h.TreeDiff.Changes, h.BlobCache.Cache)
	h.aggregateImports(extracted, h.Identity.AuthorID, h.Ticks.Tick)

	return nil
}

// Finalize completes the analysis and returns the result.
func (h *ImportsHistoryAnalyzer) Finalize() (analyze.Report, error) {
	return analyze.Report{
		"imports":      h.imports,
		"author_index": h.reversedPeopleDict,
		"tick_size":    h.TickSize,
	}, nil
}

// Fork creates a copy of the analyzer for parallel processing.
func (h *ImportsHistoryAnalyzer) Fork(n int) []analyze.HistoryAnalyzer {
	forks := make([]analyze.HistoryAnalyzer, n)
	for i := range n {
		// Use shared state legacy behavior for now.
		forks[i] = h
	}

	return forks
}

// Merge combines results from forked analyzer branches.
func (h *ImportsHistoryAnalyzer) Merge(_ []analyze.HistoryAnalyzer) {
}

// Serialize writes the analysis result to the given writer.
func (h *ImportsHistoryAnalyzer) Serialize(result analyze.Report, _ bool, writer io.Writer) error {
	imports, ok := result["imports"].(ImportsMap)
	if !ok {
		return errors.New("expected ImportsMap for imports") //nolint:err113 // descriptive error for type assertion failure.
	}

	reversedPeopleDict, ok := result["author_index"].([]string)
	if !ok {
		return errors.New("expected []string for reversedPeopleDict") //nolint:err113 // descriptive error for type assertion failure.
	}

	tickSize, ok := result["tick_size"].(time.Duration)
	if !ok {
		return errors.New("expected time.Duration for tickSize") //nolint:err113 // descriptive error for type assertion failure.
	}

	devs := make([]int, 0, len(imports))
	for dev := range imports {
		devs = append(devs, dev)
	}

	sort.Ints(devs)
	fmt.Fprintln(writer, "  tick_size:", int(tickSize.Seconds()))
	fmt.Fprintln(writer, "  imports:")

	for _, dev := range devs {
		imps := imports[dev]

		obj, err := json.Marshal(imps)
		if err != nil {
			return fmt.Errorf("serialize: %w", err)
		}

		devName := fmt.Sprintf("dev_%d", dev)
		if dev >= 0 && dev < len(reversedPeopleDict) {
			devName = reversedPeopleDict[dev]
		}

		fmt.Fprintf(writer, "    %s: %s\n", devName, string(obj))
	}

	return nil
}

// FormatReport writes the formatted analysis report to the given writer.
func (h *ImportsHistoryAnalyzer) FormatReport(report analyze.Report, writer io.Writer) error {
	return h.Serialize(report, false, writer)
}
