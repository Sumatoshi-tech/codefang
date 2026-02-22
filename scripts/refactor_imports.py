import os
import re

history_go_path = "pkg/analyzers/imports/history.go"
metrics_go_path = "pkg/analyzers/imports/metrics.go"
plot_go_path = "pkg/analyzers/imports/plot.go"
tc_go_path = "pkg/analyzers/imports/tc.go"
aggregator_go_path = "pkg/analyzers/imports/aggregator.go"
analyzer_test_go_path = "pkg/analyzers/imports/analyzer_test.go"
history_test_go_path = "pkg/analyzers/imports/history_test.go"

# 1. Update history.go to use BaseHistoryAnalyzer
history_code = """package imports

import (
	"context"
	"time"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
	"github.com/Sumatoshi-tech/codefang/pkg/identity"
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
// author -> lang -> import -> tick -> count
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
			Description: "Whenever a file is changed or added, we extract the imports from it and increment their usage for the commit author.",
			Mode:        analyze.ModeHistory,
		},
		Sequential: false,
		ComputeMetricsFn: func(report analyze.Report) (*ComputedMetrics, error) {
			if len(report) == 0 {
				return &ComputedMetrics{}, nil
			}
			return ComputeAllMetrics(report)
		},
		AggregatorFn: func() analyze.Aggregator {
			return analyze.NewGenericAggregator[*tickAccumulator, *TickData](
				a.extractTC,
				a.mergeState,
				a.sizeState,
				a.buildTick,
			)
		},
		TicksToReportFn: func(ticks []analyze.TICK) analyze.Report {
			return ticksToReport(ticks, a.reversedPeopleDict, a.TickSize)
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

	h.TreeDiff = facts[plumbing.FactTreeDiffAnalyzer].(*plumbing.TreeDiffAnalyzer)
	h.BlobCache = facts[plumbing.FactBlobCacheAnalyzer].(*plumbing.BlobCacheAnalyzer)
	h.Identity = facts[plumbing.FactIdentityDetector].(*plumbing.IdentityDetector)
	h.Ticks = facts[plumbing.FactTicksSinceStart].(*plumbing.TicksSinceStart)
	h.parser = facts[pkgplumbing.FactUASTParser].(*uast.Parser)

	return nil
}

// MapDependencies returns the required plumbing analyzers.
func (h *HistoryAnalyzer) MapDependencies() []string {
	return []string{
		plumbing.FactTreeDiffAnalyzer,
		plumbing.FactBlobCacheAnalyzer,
		plumbing.FactIdentityDetector,
		plumbing.FactTicksSinceStart,
		pkgplumbing.FactUASTParser,
	}
}

// AnalyzeCommit extracts import paths from added/modified files in a commit.
func (h *HistoryAnalyzer) AnalyzeCommit(ctx context.Context, commit *gitlib.Commit) (analyze.TC, error) {
	if h.parser == nil {
		return analyze.TC{}, errors.New("parser not initialized")
	}

	diff, err := h.TreeDiff.GetDiff(commit.Hash())
	if err != nil {
		return analyze.TC{}, fmt.Errorf("tree diff: %w", err)
	}

	tc := analyze.TC{
		Tick: h.Ticks.GetTick(commit.Hash()),
	}

	var mu sync.Mutex
	var entries []ImportEntry

	// Process files safely using waitgroup to gather entries

	// Create processing pipeline
	filesCh := make(chan string, len(diff.Added)+len(diff.Modified))
	errCh := make(chan error, 1) // Just hold the first error

	var wg sync.WaitGroup
	// Start workers
	for i := 0; i < h.Goroutines; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			for file := range filesCh {
				select {
				case <-ctx.Done():
					return
				default:
				}

				blob, bErr := h.BlobCache.GetBlob(commit.Hash(), file)
				if bErr != nil {
					select {
					case errCh <- fmt.Errorf("blob %s: %w", file, bErr):
					default:
					}
					return
				}

				if len(blob) > h.MaxFileSize || len(blob) == 0 {
					continue
				}

				lang, hasLang := pkgplumbing.ExtToLang[pkgplumbing.GetExt(file)]
				if !hasLang {
					continue
				}

				imports, parseErr := extractImports(h.parser, blob, file)
				if parseErr != nil {
					// Soft fail: ignore parse errors. They might be caused by
					// syntax errors or unsupported languages in tree-sitter.
					continue
				}

				localEntries := make([]ImportEntry, 0, len(imports))
				for _, imp := range imports {
					localEntries = append(localEntries, ImportEntry{
						Lang:   lang,
						Import: imp,
					})
				}

				mu.Lock()
				entries = append(entries, localEntries...)
				mu.Unlock()
			}
		}()
	}

	// Feed files
	for _, file := range diff.Added {
		filesCh <- file
	}
	for _, file := range diff.Modified {
		filesCh <- file
	}
	close(filesCh)

	// Wait for workers
	wg.Wait()

	select {
	case err := <-errCh:
		return tc, err
	default:
	}

	if len(entries) > 0 {
		authorID := h.Identity.GetAuthorID(commit.Hash())
		tc.Data = map[string]any{
			"entries":  entries,
			"authorID": authorID,
		}
	}

	return tc, nil
}

func extractImports(parser *uast.Parser, src []byte, name string) ([]string, error) {
	node, err := parser.Parse(src, name)
	if err != nil {
		return nil, fmt.Errorf("parse: %w", err)
	}

	// In the future this should ideally be handled using importmodel.Visitor
	// For now we extract via simple UAST query. We'd need to adapt importmodel
	// to operate directly on the universal AST rather than specific languages.

	// Placeholder fallback for now if importmodel isn't directly usable:
	return importmodel.ExtractFromUAST(node)
}

// GenericAggregator Delegates

func (h *HistoryAnalyzer) extractTC(tc analyze.TC, byTick map[int]*tickAccumulator) error {
	if tc.Data == nil {
		return nil
	}

	data, ok := tc.Data.(map[string]any)
	if !ok {
		return nil // skip silently
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

func (h *HistoryAnalyzer) mergeState(dst, src *tickAccumulator) {
	mergeImportMaps(dst.imports, src.imports)
}

func (h *HistoryAnalyzer) sizeState(acc *tickAccumulator) int {
	// A rough estimate: each developer -> lang -> import -> tick path takes some bytes.
	// For simplicity, we just count the leaf maps as 100 bytes each.
	size := 0
	for _, langs := range acc.imports {
		for _, imps := range langs {
			for _, ticks := range imps {
				size += len(ticks) * 24 // int + int64 ~ 16 bytes + map overhead
			}
		}
	}
	return size
}

func (h *HistoryAnalyzer) buildTick(tick int, acc *tickAccumulator) (*TickData, error) {
	return &TickData{
		Imports: acc.imports,
	}, nil
}

// NewAggregator creates an imports Aggregator that collects per-commit entries.
func (h *HistoryAnalyzer) NewAggregator(opts analyze.AggregatorOptions) analyze.Aggregator {
	return h.AggregatorFn()
}

// ReportFromTICKs converts aggregated TICKs into a Report.
func (h *HistoryAnalyzer) ReportFromTICKs(ticks []analyze.TICK) (analyze.Report, error) {
	return ticksToReport(ticks, h.reversedPeopleDict, h.TickSize), nil
}

// Helper methods from aggregator.go

func mergeImportMaps(dst, src Map) {
	for auth, srcLangs := range src {
		dstLangs, ok := dst[auth]
		if !ok {
			dstLangs = make(map[string]map[string]map[int]int64)
			dst[auth] = dstLangs
		}

		for lang, srcImps := range srcLangs {
			dstImps, ok := dstLangs[lang]
			if !ok {
				dstImps = make(map[string]map[int]int64)
				dstLangs[lang] = dstImps
			}

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
	}
}

func addEntriesToMap(m Map, entries []ImportEntry, authorID, tick int) {
	// Get or create lang map for author
	langs, ok := m[authorID]
	if !ok {
		langs = make(map[string]map[string]map[int]int64)
		m[authorID] = langs
	}

	for _, entry := range entries {
		// Get or create imports map for lang
		imps, ok := langs[entry.Lang]
		if !ok {
			imps = make(map[string]map[int]int64)
			langs[entry.Lang] = imps
		}

		// Get or create tick map for import
		timps, ok := imps[entry.Import]
		if !ok {
			timps = make(map[int]int64)
			imps[entry.Import] = timps
		}

		// Increment usage
		timps[tick]++
	}
}

// ticksToReport converts aggregated TICKs into the analyze.Report format
// that existing Serialize() understands.
func ticksToReport(
	ticks []analyze.TICK,
	reversedPeopleDict []string,
	tickSize interface{ Seconds() float64 },
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
"""

with open(history_go_path, "w") as f:
    f.write(history_code)

# 2. Update metrics.go to remove AnalyzerName, ToJSON, ToYAML
with open(metrics_go_path, "r") as f:
    metrics_code = f.read()

metrics_code = re.sub(r'// AnalyzerName returns the analyzer name\.\nfunc \(m \*ComputedMetrics\) AnalyzerName\(\) string \{\n\treturn "[^"]+"\n\}\n\n', "", metrics_code)
metrics_code = re.sub(r'// ToJSON returns the JSON representation\.\nfunc \(m \*ComputedMetrics\) ToJSON\(\) any \{\n\treturn m\n\}\n\n', "", metrics_code)
metrics_code = re.sub(r'// ToYAML returns the YAML representation\.\nfunc \(m \*ComputedMetrics\) ToYAML\(\) any \{\n\treturn m\n\}\n\n', "", metrics_code)

with open(metrics_go_path, "w") as f:
    f.write(metrics_code)

# 3. Update plot.go
with open(plot_go_path, "r") as f:
    plot_code = f.read()

plot_code = plot_code.replace("HistoryAnalyzer", "Analyzer")
plot_code = plot_code.replace("&Analyzer{}", "&HistoryAnalyzer{}")

# Wait, `analyzer.go` already exists as the static analyzer. `HistoryAnalyzer` generates the plots?
# Yes, `func (h *HistoryAnalyzer) GeneratePlots...` -> we keep `HistoryAnalyzer` in `plot.go`!
plot_code = plot_code.replace("func (h *Analyzer) GeneratePlots", "func (h *HistoryAnalyzer) GeneratePlots")
plot_code = plot_code.replace("analyze.RegisterPlotSections(&Analyzer{},", "analyze.RegisterPlotSections(&HistoryAnalyzer{},")

with open(plot_go_path, "w") as f:
    f.write(plot_code)

# 4. Remove unnecessary files
for file_path in [
    tc_go_path,
    aggregator_go_path,
    "pkg/analyzers/imports/aggregator_test.go",
    "pkg/analyzers/imports/checkpoint.go",
    "pkg/analyzers/imports/checkpoint_test.go",
    "pkg/analyzers/imports/hibernation.go",
    "pkg/analyzers/imports/hibernation_test.go"
]:
    if os.path.exists(file_path):
        os.remove(file_path)

