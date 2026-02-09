package imports

import (
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"sort"
	"sync"
	"time"

	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/common/reportutil"
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

// Map maps file paths to their import lists.
type Map = map[int]map[string]map[string]map[int]int64

// HistoryAnalyzer tracks import usage across commit history.
type HistoryAnalyzer struct {
	l interface { //nolint:unused // used via dependency injection.
		Warnf(format string, args ...any)
		Errorf(format string, args ...any)
	}
	TreeDiff           *plumbing.TreeDiffAnalyzer
	BlobCache          *plumbing.BlobCacheAnalyzer
	Identity           *plumbing.IdentityDetector
	Ticks              *plumbing.TicksSinceStart
	imports            Map
	parser             *uast.Parser
	reversedPeopleDict []string
	TickSize           time.Duration
	Goroutines         int
	MaxFileSize        int
}

// Name returns the name of the analyzer.
func (h *HistoryAnalyzer) Name() string {
	return "ImportsPerDeveloper"
}

// Flag returns the CLI flag for the analyzer.
func (h *HistoryAnalyzer) Flag() string {
	return "imports-per-dev"
}

// Description returns a human-readable description of the analyzer.
func (h *HistoryAnalyzer) Description() string {
	return h.Descriptor().Description
}

// Descriptor returns stable analyzer metadata.
func (h *HistoryAnalyzer) Descriptor() analyze.Descriptor {
	return analyze.Descriptor{
		ID:          "history/imports",
		Description: "Whenever a file is changed or added, we extract the imports from it and increment their usage for the commit author.",
		Mode:        analyze.ModeHistory,
	}
}

// ListConfigurationOptions returns the configuration options for the analyzer.
func (h *HistoryAnalyzer) ListConfigurationOptions() []pipeline.ConfigurationOption {
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
func (h *HistoryAnalyzer) Configure(facts map[string]any) error {
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
func (h *HistoryAnalyzer) Initialize(_ *gitlib.Repository) error {
	h.imports = Map{}
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

func (h *HistoryAnalyzer) extractImports(name string, data []byte) (*importmodel.File, error) {
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
func (h *HistoryAnalyzer) extractImportsParallel(
	changes gitlib.Changes,
	cache map[gitlib.Hash]*pkgplumbing.CachedBlob,
) map[gitlib.Hash]importmodel.File {
	extracted := map[gitlib.Hash]importmodel.File{}
	jobs := make(chan *gitlib.Change, h.Goroutines)

	var (
		mu sync.Mutex
		wg sync.WaitGroup
	)

	wg.Add(h.Goroutines)

	for range h.Goroutines {
		go func() {
			defer wg.Done()

			for change := range jobs {
				blob := cache[change.To.Hash]
				if blob == nil || blob.Size() > int64(h.MaxFileSize) {
					continue
				}

				file, err := h.extractImports(change.To.Name, blob.Data)
				if err == nil {
					mu.Lock()

					extracted[change.To.Hash] = *file

					mu.Unlock()
				}
			}
		}()
	}

	for _, change := range changes {
		action := change.Action

		switch action {
		case gitlib.Modify, gitlib.Insert:
			jobs <- change
		case gitlib.Delete:
			continue
		}
	}

	close(jobs)
	wg.Wait()

	return extracted
}

// aggregateImports folds the per-blob import data into the analyzer's
// cumulative imports map, keyed by author and tick.
func (h *HistoryAnalyzer) aggregateImports(
	extractedImports map[gitlib.Hash]importmodel.File,
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
func (h *HistoryAnalyzer) Consume(_ *analyze.Context) error {
	extracted := h.extractImportsParallel(h.TreeDiff.Changes, h.BlobCache.Cache)
	h.aggregateImports(extracted, h.Identity.AuthorID, h.Ticks.Tick)

	return nil
}

// Finalize completes the analysis and returns the result.
func (h *HistoryAnalyzer) Finalize() (analyze.Report, error) {
	return analyze.Report{
		"imports":      h.imports,
		"author_index": h.reversedPeopleDict,
		"tick_size":    h.TickSize,
	}, nil
}

// SequentialOnly returns false because imports analysis can be parallelized.
func (h *HistoryAnalyzer) SequentialOnly() bool { return false }

// SnapshotPlumbing captures the current plumbing output state for one commit.
func (h *HistoryAnalyzer) SnapshotPlumbing() analyze.PlumbingSnapshot {
	return plumbing.Snapshot{
		Changes:   h.TreeDiff.Changes,
		BlobCache: h.BlobCache.Cache,
		Tick:      h.Ticks.Tick,
		AuthorID:  h.Identity.AuthorID,
	}
}

// ApplySnapshot restores plumbing state from a previously captured snapshot.
func (h *HistoryAnalyzer) ApplySnapshot(snap analyze.PlumbingSnapshot) {
	snapshot, ok := snap.(plumbing.Snapshot)
	if !ok {
		return
	}

	h.TreeDiff.Changes = snapshot.Changes
	h.BlobCache.Cache = snapshot.BlobCache
	h.Ticks.Tick = snapshot.Tick
	h.Identity.AuthorID = snapshot.AuthorID
}

// ReleaseSnapshot releases any resources owned by the snapshot.
func (h *HistoryAnalyzer) ReleaseSnapshot(_ analyze.PlumbingSnapshot) {}

// Fork creates a copy of the analyzer for parallel processing.
// Each fork gets independent mutable state while sharing read-only config.
func (h *HistoryAnalyzer) Fork(n int) []analyze.HistoryAnalyzer {
	forks := make([]analyze.HistoryAnalyzer, n)
	for i := range n {
		clone := &HistoryAnalyzer{
			TreeDiff:           &plumbing.TreeDiffAnalyzer{},
			BlobCache:          &plumbing.BlobCacheAnalyzer{},
			Identity:           &plumbing.IdentityDetector{},
			Ticks:              &plumbing.TicksSinceStart{},
			reversedPeopleDict: h.reversedPeopleDict,
			TickSize:           h.TickSize,
			Goroutines:         h.Goroutines,
			MaxFileSize:        h.MaxFileSize,
			parser:             h.parser, // Parser is thread-safe for reads
		}
		// Initialize independent state for each fork
		clone.imports = Map{}

		forks[i] = clone
	}

	return forks
}

// Merge combines results from forked analyzer branches.
func (h *HistoryAnalyzer) Merge(branches []analyze.HistoryAnalyzer) {
	for _, branch := range branches {
		other, ok := branch.(*HistoryAnalyzer)
		if !ok {
			continue
		}

		h.mergeImports(other.imports)
	}
}

// mergeImports combines import data from another analyzer.
func (h *HistoryAnalyzer) mergeImports(other Map) {
	for author, otherLangs := range other {
		h.ensureAuthor(author)
		h.mergeAuthorImports(author, otherLangs)
	}
}

// ensureAuthor ensures the author entry exists in imports map.
func (h *HistoryAnalyzer) ensureAuthor(author int) {
	if h.imports[author] == nil {
		h.imports[author] = make(map[string]map[string]map[int]int64)
	}
}

// mergeAuthorImports merges language imports for a specific author.
func (h *HistoryAnalyzer) mergeAuthorImports(author int, otherLangs map[string]map[string]map[int]int64) {
	for lang, otherImps := range otherLangs {
		if h.imports[author][lang] == nil {
			h.imports[author][lang] = make(map[string]map[int]int64)
		}

		h.mergeLangImports(author, lang, otherImps)
	}
}

// mergeLangImports merges imports for a specific author and language.
func (h *HistoryAnalyzer) mergeLangImports(author int, lang string, otherImps map[string]map[int]int64) {
	for imp, otherTicks := range otherImps {
		if h.imports[author][lang][imp] == nil {
			h.imports[author][lang][imp] = make(map[int]int64)
		}

		for tick, count := range otherTicks {
			h.imports[author][lang][imp][tick] += count
		}
	}
}

// Serialize writes the analysis result to the given writer.
func (h *HistoryAnalyzer) Serialize(result analyze.Report, format string, writer io.Writer) error {
	switch format {
	case analyze.FormatPlot:
		return h.generatePlot(result, writer)
	case analyze.FormatJSON:
		err := json.NewEncoder(writer).Encode(result)
		if err != nil {
			return fmt.Errorf("json encode: %w", err)
		}

		return nil
	case analyze.FormatBinary:
		err := reportutil.EncodeBinaryEnvelope(result, writer)
		if err != nil {
			return fmt.Errorf("binary encode: %w", err)
		}

		return nil
	case analyze.FormatYAML:
		return h.serializeYAML(result, writer)
	default:
		return fmt.Errorf("%w: %s", analyze.ErrUnsupportedFormat, format)
	}
}

func (h *HistoryAnalyzer) serializeYAML(result analyze.Report, writer io.Writer) error {
	imports, ok := result["imports"].(Map)
	if !ok {
		return errors.New("expected Map for imports") //nolint:err113 // descriptive error for type assertion failure.
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
func (h *HistoryAnalyzer) FormatReport(report analyze.Report, writer io.Writer) error {
	return h.Serialize(report, analyze.FormatYAML, writer)
}
