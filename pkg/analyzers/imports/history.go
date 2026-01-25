package imports

import (
	"encoding/json"
	"fmt"
	"io"
	"sort"
	"sync"
	"time"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/identity"
	"github.com/Sumatoshi-tech/codefang/pkg/importmodel"
	"github.com/Sumatoshi-tech/codefang/pkg/pipeline"
	pkgplumbing "github.com/Sumatoshi-tech/codefang/pkg/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/uast"
	"github.com/go-git/go-git/v6"
	gitplumbing "github.com/go-git/go-git/v6/plumbing"
	"github.com/go-git/go-git/v6/plumbing/object"
	"github.com/go-git/go-git/v6/utils/merkletrie"
)

type ImportsMap = map[int]map[string]map[string]map[int]int64

type ImportsHistoryAnalyzer struct {
	// Configuration
	TickSize    time.Duration
	Goroutines  int
	MaxFileSize int

	// Dependencies
	TreeDiff  *plumbing.TreeDiffAnalyzer
	BlobCache *plumbing.BlobCacheAnalyzer
	Identity  *plumbing.IdentityDetector
	Ticks     *plumbing.TicksSinceStart

	// State
	imports            ImportsMap
	reversedPeopleDict []string
	parser             *uast.Parser

	// Internal
	l interface {
		Warnf(format string, args ...interface{})
		Errorf(format string, args ...interface{})
	}
}

func (h *ImportsHistoryAnalyzer) Name() string {
	return "ImportsPerDeveloper"
}

func (h *ImportsHistoryAnalyzer) Flag() string {
	return "imports-per-dev"
}

func (h *ImportsHistoryAnalyzer) Description() string {
	return "Whenever a file is changed or added, we extract the imports from it and increment their usage for the commit author."
}

func (h *ImportsHistoryAnalyzer) ListConfigurationOptions() []pipeline.ConfigurationOption {
	return []pipeline.ConfigurationOption{
		{
			Name:        "Imports.Goroutines",
			Description: "Specifies the number of goroutines to run in parallel for the imports extraction.",
			Flag:        "import-goroutines",
			Type:        pipeline.IntConfigurationOption,
			Default:     4,
		},
		{
			Name:        "Imports.MaxFileSize",
			Description: "Specifies the file size threshold. Files that exceed it are ignored.",
			Flag:        "import-max-file-size",
			Type:        pipeline.IntConfigurationOption,
			Default:     1 << 20,
		},
	}
}

func (h *ImportsHistoryAnalyzer) Configure(facts map[string]interface{}) error {
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

func (h *ImportsHistoryAnalyzer) Initialize(repository *git.Repository) error {
	h.imports = ImportsMap{}
	if h.TickSize == 0 {
		h.TickSize = time.Hour * 24
	}
	if h.Goroutines < 1 {
		h.Goroutines = 4
	}
	if h.MaxFileSize == 0 {
		h.MaxFileSize = 1 << 20
	}

	// Initialize UAST parser
	var err error
	h.parser, err = uast.NewParser()
	if err != nil {
		return fmt.Errorf("failed to initialize UAST parser: %w", err)
	}

	return nil
}

func (h *ImportsHistoryAnalyzer) extractImports(name string, data []byte) (*importmodel.File, error) {
	if h.parser == nil {
		return nil, fmt.Errorf("parser not initialized")
	}

	// Check if supported
	if !h.parser.IsSupported(name) {
		return nil, fmt.Errorf("unsupported language for %s", name)
	}

	// Parse
	root, err := h.parser.Parse(name, data)
	if err != nil {
		return nil, err
	}

	// Extract imports using logic from analyzer.go (in same package)
	imports := extractImportsFromUAST(root)

	// Determine language
	lang := h.parser.GetLanguage(name)
	if lang == "" {
		lang = "uast"
	}

	return &importmodel.File{
		Lang:    lang,
		Imports: imports,
	}, nil
}

func (h *ImportsHistoryAnalyzer) Consume(ctx *analyze.Context) error {
	changes := h.TreeDiff.Changes
	cache := h.BlobCache.Cache

	extractedImports := map[gitplumbing.Hash]importmodel.File{}
	jobs := make(chan *object.Change, h.Goroutines)
	resultSync := sync.Mutex{}
	wg := sync.WaitGroup{}
	wg.Add(h.Goroutines)

	for i := 0; i < h.Goroutines; i++ {
		go func() {
			for change := range jobs {
				blob := cache[change.To.TreeEntry.Hash]
				if blob.Size > int64(h.MaxFileSize) {
					continue
				}
				file, err := h.extractImports(change.To.TreeEntry.Name, blob.Data)
				if err == nil {
					resultSync.Lock()
					extractedImports[change.To.TreeEntry.Hash] = *file
					resultSync.Unlock()
				}
			}
			wg.Done()
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

	author := h.Identity.AuthorID
	tick := h.Ticks.Tick

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

	return nil
}

func (h *ImportsHistoryAnalyzer) Finalize() (analyze.Report, error) {
	return analyze.Report{
		"imports":      h.imports,
		"author_index": h.reversedPeopleDict,
		"tick_size":    h.TickSize,
	}, nil
}

func (h *ImportsHistoryAnalyzer) Fork(n int) []analyze.HistoryAnalyzer {
	forks := make([]analyze.HistoryAnalyzer, n)
	for i := 0; i < n; i++ {
		// Use shared state legacy behavior for now
		forks[i] = h
	}
	return forks
}

func (h *ImportsHistoryAnalyzer) Merge(branches []analyze.HistoryAnalyzer) {
}

func (h *ImportsHistoryAnalyzer) Serialize(result analyze.Report, binary bool, writer io.Writer) error {
	imports := result["imports"].(ImportsMap)
	reversedPeopleDict := result["author_index"].([]string)
	tickSize := result["tick_size"].(time.Duration)

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
			return err
		}
		devName := fmt.Sprintf("dev_%d", dev)
		if dev >= 0 && dev < len(reversedPeopleDict) {
			devName = reversedPeopleDict[dev]
		}
		fmt.Fprintf(writer, "    %s: %s\n", devName, string(obj))
	}
	return nil
}

func (h *ImportsHistoryAnalyzer) FormatReport(report analyze.Report, writer io.Writer) error {
	return h.Serialize(report, false, writer)
}
