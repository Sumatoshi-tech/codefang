package imports

import (
	"context"
	"errors"
	"fmt"
	"sync"
	"time"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
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
	estimatedImportSize     = 24
)

// ErrParserNotInitialized indicates the UAST parser is not initialized.
var ErrParserNotInitialized = errors.New("parser not initialized")

// ErrUnsupportedLanguage indicates the language is not supported.
var ErrUnsupportedLanguage = errors.New("unsupported language")

// Map maps file paths to their import lists.
// author -> lang -> import -> tick -> count.
type Map = map[int]map[string]map[string]map[int]int64

// ImportEntry represents a single import extracted from a commit.
// It carries the language and import path for aggregation.
type ImportEntry struct {
	Lang   string
	Import string
}

// TickData is the per-tick aggregated payload stored in analyze.TICK.Data.
// It holds the accumulated 4-level imports map for the tick.
type TickData struct {
	Imports Map
}

// tickAccumulator holds the in-memory state during aggregation for a single tick.
type tickAccumulator struct {
	imports Map
}

// HistoryAnalyzer tracks import usage across commit history.
type HistoryAnalyzer struct {
	*analyze.BaseHistoryAnalyzer[*ComputedMetrics]

	TreeDiff           *plumbing.TreeDiffAnalyzer
	BlobCache          *plumbing.BlobCacheAnalyzer
	Identity           *plumbing.IdentityDetector
	Ticks              *plumbing.TicksSinceStart
	parser             *uast.Parser
	reversedPeopleDict []string
	TickSize           time.Duration
	Goroutines         int
	MaxFileSize        int
}

// NewHistoryAnalyzer creates a new HistoryAnalyzer.
func NewHistoryAnalyzer() *HistoryAnalyzer {
	a := &HistoryAnalyzer{
		Goroutines:  defaultGoroutines,
		MaxFileSize: 1 << defaultMaxFileSizeShift,
		TickSize:    defaultTickHours * time.Hour,
	}

	a.BaseHistoryAnalyzer = &analyze.BaseHistoryAnalyzer[*ComputedMetrics]{
		Desc: analyze.Descriptor{
			ID:          "history/imports",
			Description: "Extracts imports from changed files and tracks usage per author.",
			Mode:        analyze.ModeHistory,
		},
		Sequential: false,
		ComputeMetricsFn: func(report analyze.Report) (*ComputedMetrics, error) {
			if len(report) == 0 {
				return &ComputedMetrics{}, nil
			}

			return ComputeAllMetrics(report)
		},
		AggregatorFn: func(opts analyze.AggregatorOptions) analyze.Aggregator {
			return analyze.NewGenericAggregator[*tickAccumulator, *TickData](
				opts,
				a.extractTC,
				a.mergeState,
				a.sizeState,
				a.buildTick,
			)
		},
		TicksToReportFn: func(ctx context.Context, ticks []analyze.TICK) analyze.Report {
			return ticksToReport(ctx, ticks, a.reversedPeopleDict, a.TickSize)
		},
	}

	return a
}

// Name returns the name of the analyzer.
func (h *HistoryAnalyzer) Name() string {
	return "ImportsPerDeveloper"
}

// Flag returns the CLI flag for the analyzer.
func (h *HistoryAnalyzer) Flag() string {
	return "imports-per-dev"
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

func (h *HistoryAnalyzer) extractImports(ctx context.Context, name string, data []byte) (*importmodel.File, error) {
	if h.parser == nil {
		return nil, ErrParserNotInitialized
	}

	// Check if supported.
	if !h.parser.IsSupported(name) {
		return nil, fmt.Errorf("%w: %s", ErrUnsupportedLanguage, name)
	}

	// Parse.
	root, err := h.parser.Parse(ctx, name, data)
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
	ctx context.Context,
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

			h.processImportJobs(ctx, jobs, cache, &mu, extracted)
		}()
	}

	dispatchImportJobs(changes, jobs)
	wg.Wait()

	return extracted
}

// processImportJobs reads changes from the jobs channel, extracts imports from
// each blob, and stores results in the extracted map under lock.
func (h *HistoryAnalyzer) processImportJobs(
	ctx context.Context,
	jobs <-chan *gitlib.Change,
	cache map[gitlib.Hash]*pkgplumbing.CachedBlob,
	mu *sync.Mutex,
	extracted map[gitlib.Hash]importmodel.File,
) {
	for change := range jobs {
		blob := cache[change.To.Hash]
		if blob == nil || blob.Size() > int64(h.MaxFileSize) {
			continue
		}

		file, err := h.extractImports(ctx, change.To.Name, blob.Data)
		if err != nil {
			continue
		}

		mu.Lock()

		extracted[change.To.Hash] = *file

		mu.Unlock()
	}
}

// dispatchImportJobs sends modified or inserted changes to the jobs channel.
func dispatchImportJobs(changes gitlib.Changes, jobs chan<- *gitlib.Change) {
	for _, change := range changes {
		switch change.Action {
		case gitlib.Modify, gitlib.Insert:
			jobs <- change
		case gitlib.Delete:
			continue
		}
	}

	close(jobs)
}

// Consume processes a single commit with the provided dependency results.
func (h *HistoryAnalyzer) Consume(ctx context.Context, _ *analyze.Context) (analyze.TC, error) {
	if h.parser == nil {
		return analyze.TC{}, ErrParserNotInitialized
	}

	extracted := h.extractImportsParallel(ctx, h.TreeDiff.Changes, h.BlobCache.Cache)

	var entries []ImportEntry

	for _, file := range extracted {
		for _, imp := range file.Imports {
			entries = append(entries, ImportEntry{
				Lang:   file.Lang,
				Import: imp,
			})
		}
	}

	tc := analyze.TC{
		Tick: h.Ticks.Tick,
	}

	if len(entries) > 0 {
		tc.Data = map[string]any{
			"entries":  entries,
			"authorID": h.Identity.AuthorID,
		}
	}

	return tc, nil
}

// GenericAggregator Delegates.

func (h *HistoryAnalyzer) extractTC(tc analyze.TC, byTick map[int]*tickAccumulator) error {
	if tc.Data == nil {
		return nil
	}

	data, ok := tc.Data.(map[string]any)
	if !ok {
		return nil
	}

	entries, ok := data["entries"].([]ImportEntry)
	if !ok || len(entries) == 0 {
		return nil
	}

	authorID, ok := data["authorID"].(int)
	if !ok {
		return nil
	}

	acc, exists := byTick[tc.Tick]
	if !exists {
		acc = &tickAccumulator{imports: make(Map)}
		byTick[tc.Tick] = acc
	}

	addEntriesToMap(acc.imports, entries, authorID, tc.Tick)

	return nil
}

func (h *HistoryAnalyzer) mergeState(dst, src *tickAccumulator) *tickAccumulator {
	mergeImportMaps(dst.imports, src.imports)

	return dst
}

func (h *HistoryAnalyzer) sizeState(acc *tickAccumulator) int64 {
	size := int64(0)

	for _, langs := range acc.imports {
		for _, imps := range langs {
			for _, ticks := range imps {
				size += int64(len(ticks) * estimatedImportSize)
			}
		}
	}

	return size
}

func (h *HistoryAnalyzer) buildTick(tick int, acc *tickAccumulator) (analyze.TICK, error) {
	return analyze.TICK{
		Tick: tick,
		Data: &TickData{
			Imports: acc.imports,
		},
	}, nil
}

// NewAggregator creates an imports Aggregator that collects per-commit entries.
func (h *HistoryAnalyzer) NewAggregator(opts analyze.AggregatorOptions) analyze.Aggregator {
	return h.AggregatorFn(opts)
}

// ReportFromTICKs converts aggregated TICKs into a Report.
func (h *HistoryAnalyzer) ReportFromTICKs(ctx context.Context, ticks []analyze.TICK) (analyze.Report, error) {
	return ticksToReport(ctx, ticks, h.reversedPeopleDict, h.TickSize), nil
}

// Helper methods.

func mergeImportMaps(dst, src Map) {
	for auth, srcLangs := range src {
		dstLangs, ok := dst[auth]
		if !ok {
			dstLangs = make(map[string]map[string]map[int]int64)
			dst[auth] = dstLangs
		}

		mergeLangImports(dstLangs, srcLangs)
	}
}

func mergeLangImports(dstLangs, srcLangs map[string]map[string]map[int]int64) {
	for lang, srcImps := range srcLangs {
		dstImps, ok := dstLangs[lang]
		if !ok {
			dstImps = make(map[string]map[int]int64)
			dstLangs[lang] = dstImps
		}

		mergeTicks(dstImps, srcImps)
	}
}

func mergeTicks(dstImps, srcImps map[string]map[int]int64) {
	for imp, srcTicks := range srcImps {
		dstTicks, ok := dstImps[imp]
		if !ok {
			dstTicks = make(map[int]int64)
			dstImps[imp] = dstTicks
		}

		for tick, count := range srcTicks {
			dstTicks[tick] += count
		}
	}
}

func addEntriesToMap(m Map, entries []ImportEntry, authorID, tick int) {
	langs, hasAuthor := m[authorID]
	if !hasAuthor {
		langs = make(map[string]map[string]map[int]int64)
		m[authorID] = langs
	}

	for _, entry := range entries {
		imps, hasLang := langs[entry.Lang]
		if !hasLang {
			imps = make(map[string]map[int]int64)
			langs[entry.Lang] = imps
		}

		timps, hasImp := imps[entry.Import]
		if !hasImp {
			timps = make(map[int]int64)
			imps[entry.Import] = timps
		}

		timps[tick]++
	}
}

// ticksToReport converts aggregated TICKs into the analyze.Report format.
func ticksToReport(
	_ context.Context,
	ticks []analyze.TICK,
	reversedPeopleDict []string,
	tickSize time.Duration,
) analyze.Report {
	merged := Map{}

	for _, tick := range ticks {
		td, ok := tick.Data.(*TickData)
		if !ok || td == nil {
			continue
		}

		mergeImportMaps(merged, td.Imports)
	}

	return analyze.Report{
		"imports":      merged,
		"author_index": reversedPeopleDict,
		"tick_size":    tickSize,
	}
}

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
			BaseHistoryAnalyzer: h.BaseHistoryAnalyzer,
			TreeDiff:            &plumbing.TreeDiffAnalyzer{},
			BlobCache:           &plumbing.BlobCacheAnalyzer{},
			Identity:            &plumbing.IdentityDetector{},
			Ticks:               &plumbing.TicksSinceStart{},
			reversedPeopleDict:  h.reversedPeopleDict,
			TickSize:            h.TickSize,
			Goroutines:          h.Goroutines,
			MaxFileSize:         h.MaxFileSize,
			parser:              h.parser, // Parser is thread-safe for reads.
		}

		forks[i] = clone
	}

	return forks
}

// Merge is a no-op since state is managed by the GenericAggregator.
func (h *HistoryAnalyzer) Merge(_ []analyze.HistoryAnalyzer) {}
