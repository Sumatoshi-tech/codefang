package analyze

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"io/fs"
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"sync"
	"sync/atomic"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/common/plotpage"
	"github.com/Sumatoshi-tech/codefang/internal/storage"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
	"github.com/Sumatoshi-tech/codefang/pkg/meminfo"
	"github.com/Sumatoshi-tech/codefang/pkg/pipeline"
	"github.com/Sumatoshi-tech/codefang/pkg/textutil"
	"github.com/Sumatoshi-tech/codefang/pkg/uast"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

// DefaultStaticMaxWorkers is the maximum number of concurrent file analysis workers
// when no explicit override is provided. Caps memory from concurrent UAST parse trees.
const DefaultStaticMaxWorkers = 8

// DefaultMallocTrimInterval is the number of files between malloc_trim calls.
// Releases glibc arenas back to the OS to prevent native memory accumulation.
const DefaultMallocTrimInterval = 50

// DefaultProgressInterval is the number of files between progress callback invocations.
const DefaultProgressInterval = 1000

// ProgressPhaseProcessing indicates files are being analyzed.
const ProgressPhaseProcessing = "processing"

// ProgressPhaseComplete indicates analysis has finished.
const ProgressPhaseComplete = "complete"

// StaticProgressEvent represents a static analysis progress milestone.
type StaticProgressEvent struct {
	FilesProcessed int64
	RSSBytes       int64
	AggregatorSize int64
	Phase          string
}

// StaticProgressFunc is called at key pipeline milestones.
type StaticProgressFunc func(event StaticProgressEvent)

// ErrRendererNotSet is returned when a formatting method is called without a Renderer.
var ErrRendererNotSet = errors.New("static service renderer not set")

// StaticRenderer abstracts section-based rendering to avoid import cycles
// between the analyze and renderer packages. The renderer package provides
// the production implementation.
type StaticRenderer interface {
	// SectionsToJSON converts report sections to a JSON-serializable value.
	SectionsToJSON(sections []ReportSection) any

	// RenderText writes human-readable text output for the given sections.
	RenderText(sections []ReportSection, verbose, noColor bool, writer io.Writer) error

	// RenderCompact writes single-line-per-section compact output.
	RenderCompact(sections []ReportSection, noColor bool, writer io.Writer) error
}

// StaticService provides a high-level interface for running static analysis.
type StaticService struct {
	Analyzers []StaticAnalyzer

	// MaxWorkers limits the number of concurrent file analysis goroutines.
	// Zero means use min(runtime.NumCPU(), DefaultStaticMaxWorkers).
	MaxWorkers int

	// MallocTrimInterval is the number of files between native memory trim calls.
	// Zero means use DefaultMallocTrimInterval. Negative disables trimming.
	MallocTrimInterval int

	// NativeMemoryReleaseFn is called periodically to release native memory.
	// Defaults to gitlib.ReleaseNativeMemory when nil.
	NativeMemoryReleaseFn func()

	// AggregationMode controls whether per-item data is collected during aggregation.
	// Full (default) collects all data. SummaryOnly skips per-item collection.
	AggregationMode AggregationMode

	// SpillThreshold overrides the default spill-to-disk threshold on aggregators.
	// Zero means use the aggregator default. Derived from --memory-budget.
	SpillThreshold int

	// ProgressFunc is called at pipeline milestones when non-nil.
	// Called every ProgressInterval files during processing, and once after completion.
	ProgressFunc StaticProgressFunc

	// ProgressInterval is the number of files between progress callbacks.
	// Zero means use DefaultProgressInterval.
	ProgressInterval int

	// Renderer provides section-based output rendering.
	// Must be set before calling FormatJSON, FormatText, FormatCompact, or RunAndFormat.
	Renderer StaticRenderer

	// PerFile enables per-file report retention in aggregators.
	// When true, aggregators store per-file snapshots accessible via PerFileResults.
	PerFile bool

	// perFileResults is populated after AnalyzeFolder when PerFile is true.
	// Keyed by analyzer name → file path → per-file report.
	perFileResults map[string]map[string]Report

	// analysisRootPath is the root path used in the last AnalyzeFolder call.
	// Used by FormatJSON to make per-file paths relative.
	analysisRootPath string
}

// NewStaticService creates a StaticService with the given analyzers.
func NewStaticService(analyzers []StaticAnalyzer) *StaticService {
	return &StaticService{Analyzers: analyzers}
}

// ResolveMaxWorkers returns the effective worker count for parallel file analysis.
// Zero resolves to min(runtime.NumCPU(), DefaultStaticMaxWorkers).
func (svc *StaticService) ResolveMaxWorkers() int {
	if svc.MaxWorkers > 0 {
		return svc.MaxWorkers
	}

	cpus := runtime.NumCPU()
	if cpus > DefaultStaticMaxWorkers {
		return DefaultStaticMaxWorkers
	}

	return cpus
}

// ResolveMallocTrimInterval returns the effective trim interval.
// Zero resolves to DefaultMallocTrimInterval. Negative means disabled (returns -1).
func (svc *StaticService) ResolveMallocTrimInterval() int {
	if svc.MallocTrimInterval > 0 {
		return svc.MallocTrimInterval
	}

	if svc.MallocTrimInterval < 0 {
		return -1
	}

	return DefaultMallocTrimInterval
}

// resolveProgressInterval returns the effective progress interval.
func (svc *StaticService) resolveProgressInterval() int64 {
	if svc.ProgressInterval > 0 {
		return int64(svc.ProgressInterval)
	}

	return DefaultProgressInterval
}

// emitProgress calls ProgressFunc if set, computing aggregator sizes and reading RSS.
func (svc *StaticService) emitProgress(
	filesProcessed int64,
	aggregators map[string]ResultAggregator,
	phase string,
) {
	if svc.ProgressFunc == nil {
		return
	}

	var aggSize int64

	for _, agg := range aggregators {
		if sizer, ok := agg.(StateSizer); ok {
			aggSize += sizer.EstimatedStateSize()
		}
	}

	svc.ProgressFunc(StaticProgressEvent{
		FilesProcessed: filesProcessed,
		RSSBytes:       meminfo.ReadRSSBytes(),
		AggregatorSize: aggSize,
		Phase:          phase,
	})
}

// streamFilesBufSize is the channel buffer size for streaming file discovery.
// Workers block naturally when the buffer is full, providing backpressure.
const streamFilesBufSize = 100

// AnalyzeFolder runs static analyzers for supported files in a folder tree.
// File discovery streams paths to workers via a channel, providing natural backpressure.
func (svc *StaticService) AnalyzeFolder(ctx context.Context, rootPath string, analyzerList []string) (map[string]Report, error) {
	svc.analysisRootPath = rootPath

	analyzersToRun := svc.resolveAnalyzerList(analyzerList)
	aggregators := svc.initAggregators(analyzersToRun)

	ctx, cancel := context.WithCancel(ctx)
	defer cancel()

	var fileCounter atomic.Int64

	fileCh := make(chan string, streamFilesBufSize)
	walkErrCh := make(chan error, 1)

	go func() {
		walkErrCh <- svc.streamFiles(ctx, rootPath, fileCh)
	}()

	poolErr := svc.analyzeFilesParallel(ctx, fileCh, analyzersToRun, aggregators, &fileCounter)

	walkErr := <-walkErrCh

	if poolErr != nil {
		return nil, poolErr
	}

	if walkErr != nil {
		return nil, walkErr
	}

	results := buildFinalResults(aggregators)

	if svc.PerFile {
		svc.perFileResults = extractPerFileResults(aggregators)
	}

	svc.emitProgress(fileCounter.Load(), aggregators, ProgressPhaseComplete)

	return results, nil
}

// streamFiles walks the directory tree and sends supported file paths on fileCh.
// The channel is closed when the walk completes. Returns walk errors.
func (svc *StaticService) streamFiles(ctx context.Context, rootPath string, fileCh chan<- string) error {
	defer close(fileCh)

	parser, err := uast.NewParser()
	if err != nil {
		return fmt.Errorf("create parser: %w", err)
	}

	err = filepath.WalkDir(rootPath, func(path string, entry os.DirEntry, walkErr error) error {
		if ctx.Err() != nil {
			return ctx.Err()
		}

		skip, skipErr := ShouldSkipFolderNode(path, entry, walkErr, parser)
		if skip || skipErr != nil {
			return skipErr
		}

		select {
		case fileCh <- path:
		case <-ctx.Done():
			return ctx.Err()
		}

		return nil
	})
	if err != nil {
		return fmt.Errorf("walk %s: %w", rootPath, err)
	}

	return nil
}

// resolveReleaseFn returns the effective native memory release function.
func (svc *StaticService) resolveReleaseFn() func() {
	if svc.NativeMemoryReleaseFn != nil {
		return svc.NativeMemoryReleaseFn
	}

	return func() { gitlib.ReleaseNativeMemory() }
}

// analyzeFilesParallel processes files from a channel using a WorkerPool,
// each goroutine acquiring a parser from a bounded channel-based pool.
func (svc *StaticService) analyzeFilesParallel(
	ctx context.Context,
	fileCh <-chan string,
	analyzersToRun []string,
	aggregators map[string]ResultAggregator,
	fileCounter *atomic.Int64,
) error {
	var mu sync.Mutex

	maxWorkers := svc.ResolveMaxWorkers()
	parserCh := make(chan *uast.Parser, maxWorkers)
	trimInterval := svc.ResolveMallocTrimInterval()
	releaseFn := svc.resolveReleaseFn()
	progressInterval := svc.resolveProgressInterval()

	pool := pipeline.WorkerPool[string]{
		MaxParallel: maxWorkers,
		Work: func(ctx context.Context, filePath string) error {
			parser, err := acquireParser(parserCh)
			if err != nil {
				return err
			}

			defer func() { parserCh <- parser }()

			reportMap, analyzeErr := svc.analyzeFile(ctx, filePath, parser, analyzersToRun)
			if analyzeErr != nil {
				if errors.Is(analyzeErr, fs.ErrPermission) || errors.Is(analyzeErr, fs.ErrNotExist) {
					return nil
				}

				return analyzeErr
			}

			StampSourceFile(reportMap, filePath)

			mu.Lock()
			aggregateFolderAnalysis(reportMap, aggregators)
			mu.Unlock()

			count := fileCounter.Add(1)

			if trimInterval > 0 && count%int64(trimInterval) == 0 {
				releaseFn()
			}

			if count%progressInterval == 0 {
				svc.emitProgress(count, aggregators, ProgressPhaseProcessing)
			}

			return nil
		},
	}

	return pool.RunChan(ctx, fileCh)
}

// acquireParser retrieves a parser from the channel or creates a new one.
func acquireParser(ch chan *uast.Parser) (*uast.Parser, error) {
	select {
	case p := <-ch:
		return p, nil
	default:
	}

	parser, err := uast.NewParser()
	if err != nil {
		return nil, fmt.Errorf("create worker parser: %w", err)
	}

	return parser, nil
}

// StampSourceFile adds "_source_file" metadata to every collection item in each report.
// Also sets SourceFileKey at the report top level for analyzers without collections (e.g., imports).
// This allows downstream consumers (e.g., plot generators, per-file retention) to group results by file.
// Handles both legacy []map[string]any collections and TypedCollection wrappers.
func StampSourceFile(reports map[string]Report, filePath string) {
	for _, report := range reports {
		report[SourceFileKey] = filePath

		for key, val := range report {
			switch v := val.(type) {
			case TypedCollection:
				v.SourceFile = filePath
				report[key] = v
			case []map[string]any:
				for _, item := range v {
					item[SourceFileKey] = filePath
				}
			}
		}
	}
}

// ShouldSkipFolderNode decides whether a folder walk entry should be skipped.
func ShouldSkipFolderNode(path string, entry os.DirEntry, walkErr error, parser *uast.Parser) (bool, error) {
	if walkErr != nil {
		if errors.Is(walkErr, fs.ErrPermission) || errors.Is(walkErr, fs.ErrNotExist) {
			if entry != nil && entry.IsDir() {
				return true, filepath.SkipDir
			}

			return true, nil
		}

		return false, walkErr
	}

	if entry == nil {
		return true, nil
	}

	if entry.IsDir() {
		if entry.Name() == ".git" {
			return true, filepath.SkipDir
		}

		return true, nil
	}

	if !parser.IsSupported(path) {
		return true, nil
	}

	return false, nil
}

func (svc *StaticService) analyzeFile(
	ctx context.Context, path string, parser *uast.Parser, analyzersToRun []string,
) (map[string]Report, error) {
	content, err := os.ReadFile(path)
	if err != nil {
		return nil, fmt.Errorf("read %s: %w", path, err)
	}

	uastNode, err := parser.Parse(ctx, path, content)
	if err != nil {
		return nil, fmt.Errorf("parse %s: %w", path, err)
	}

	results, err := svc.runAnalyzers(ctx, uastNode, analyzersToRun)

	node.ReleaseTree(uastNode)

	if err != nil {
		return nil, fmt.Errorf("run analyzers for %s: %w", path, err)
	}

	return results, nil
}

func aggregateFolderAnalysis(results map[string]Report, aggregators map[string]ResultAggregator) {
	for analyzerName, aggregator := range aggregators {
		report, found := results[analyzerName]
		if !found {
			continue
		}

		aggregator.Aggregate(map[string]Report{analyzerName: report})
	}
}

func (svc *StaticService) resolveAnalyzerList(analyzerList []string) []string {
	if len(analyzerList) > 0 {
		return analyzerList
	}

	names := make([]string, 0, len(svc.Analyzers))

	for _, analyzer := range svc.Analyzers {
		names = append(names, analyzer.Name())
	}

	return names
}

func (svc *StaticService) initAggregators(analyzersToRun []string) map[string]ResultAggregator {
	aggregators := make(map[string]ResultAggregator)

	for _, analyzerName := range analyzersToRun {
		analyzer := svc.FindAnalyzer(analyzerName)
		if analyzer == nil {
			continue
		}

		agg := analyzer.CreateAggregator()

		if aware, ok := agg.(AggregationModeAware); ok {
			aware.SetAggregationMode(svc.AggregationMode)
		}

		if setter, ok := agg.(SpillThresholdSetter); svc.SpillThreshold > 0 && ok {
			setter.SetSpillThreshold(svc.SpillThreshold)
		}

		if pf, ok := agg.(PerFileModeEnabled); svc.PerFile && ok {
			pf.SetPerFileMode(true)
		}

		aggregators[analyzerName] = agg
	}

	return aggregators
}

func buildFinalResults(aggregators map[string]ResultAggregator) map[string]Report {
	allResults := make(map[string]Report)

	for analyzerName, aggregator := range aggregators {
		allResults[analyzerName] = aggregator.GetResult()
	}

	return allResults
}

// BuildSections creates ReportSection instances from results in deterministic order.
func (svc *StaticService) BuildSections(results map[string]Report) []ReportSection {
	sections := make([]ReportSection, 0, len(results))

	for _, currentAnalyzer := range svc.Analyzers {
		report, found := results[currentAnalyzer.Name()]
		if !found {
			continue
		}

		if provider, isProvider := currentAnalyzer.(ReportSectionProvider); isProvider {
			sections = append(sections, provider.CreateReportSection(report))
		}
	}

	return sections
}

func (svc *StaticService) runAnalyzers(ctx context.Context, uastNode *node.Node, analyzerList []string) (map[string]Report, error) {
	factory := NewFactory(svc.Analyzers)

	return factory.RunAnalyzers(ctx, uastNode, analyzerList)
}

// FindAnalyzer finds an analyzer by name.
func (svc *StaticService) FindAnalyzer(name string) StaticAnalyzer {
	for _, analyzer := range svc.Analyzers {
		if analyzer.Name() == name {
			return analyzer
		}
	}

	return nil
}

// AnalyzerNamesByID resolves analyzer descriptor IDs to internal names.
func (svc *StaticService) AnalyzerNamesByID(ids []string) ([]string, error) {
	idToName := make(map[string]string, len(svc.Analyzers))
	for _, analyzer := range svc.Analyzers {
		idToName[analyzer.Descriptor().ID] = analyzer.Name()
	}

	names := make([]string, 0, len(ids))
	for _, id := range ids {
		name, ok := idToName[id]
		if !ok {
			return nil, fmt.Errorf("%w: %s", ErrUnknownAnalyzerID, id)
		}

		names = append(names, name)
	}

	return names, nil
}

// FormatJSON encodes analysis results as indented JSON.
func (svc *StaticService) FormatJSON(results map[string]Report, writer io.Writer) error {
	if svc.Renderer == nil {
		return ErrRendererNotSet
	}

	sections := svc.BuildSections(results)
	report := svc.Renderer.SectionsToJSON(sections)

	if svc.PerFile {
		report = svc.enrichWithPerFileData(report, sections)
	}

	encoder := json.NewEncoder(writer)
	encoder.SetIndent("", "  ")

	err := encoder.Encode(report)
	if err != nil {
		return fmt.Errorf("failed to encode JSON: %w", err)
	}

	return nil
}

// FormatText renders analysis results as human-readable text with optional color and verbosity.
func (svc *StaticService) FormatText(results map[string]Report, verbose, noColor bool, writer io.Writer) error {
	if svc.Renderer == nil {
		return ErrRendererNotSet
	}

	sections := svc.BuildSections(results)

	return svc.Renderer.RenderText(sections, verbose, noColor, writer)
}

// FormatCompact renders analysis results as single-line-per-analyzer compact output.
func (svc *StaticService) FormatCompact(results map[string]Report, noColor bool, writer io.Writer) error {
	if svc.Renderer == nil {
		return ErrRendererNotSet
	}

	sections := svc.BuildSections(results)

	return svc.Renderer.RenderCompact(sections, noColor, writer)
}

// FormatPerAnalyzer renders results using per-analyzer formatters (YAML, plot, or binary).
func (svc *StaticService) FormatPerAnalyzer(
	analyzerNames []string,
	results map[string]Report,
	format string,
	writer io.Writer,
) error {
	isFirst := true

	for _, analyzerName := range analyzerNames {
		report, ok := results[analyzerName]
		if !ok {
			continue
		}

		analyzer := svc.FindAnalyzer(analyzerName)
		if analyzer == nil {
			return fmt.Errorf("%w: %s", ErrUnknownAnalyzerID, analyzerName)
		}

		if !isFirst && format != FormatBinary {
			_, _ = fmt.Fprintln(writer)
		}

		var err error

		switch format {
		case FormatYAML:
			err = analyzer.FormatReportYAML(report, writer)
		case FormatPlot:
			err = analyzer.FormatReportPlot(report, writer)
		case FormatBinary:
			err = analyzer.FormatReportBinary(report, writer)
		default:
			err = fmt.Errorf("%w: %s", ErrUnsupportedFormat, format)
		}

		if err != nil {
			return fmt.Errorf("format static analyzer %s: %w", analyzerName, err)
		}

		isFirst = false
	}

	return nil
}

// plotPageTitle is the project title shown on plot pages.
const plotPageTitle = "Codefang"

// plotIDSep is the separator in analyzer IDs (e.g., "static/complexity").
const plotIDSep = "/"

// plotSafeIDSep is the safe separator for filenames (e.g., "static-complexity").
const plotSafeIDSep = "-"

// plotDirPerm is the permission for plot output directories.
const plotDirPerm = 0o750

// RenderPlotPages renders per-analyzer HTML pages to outputDir without an index.
// Returns page metadata for later index rendering.
func (svc *StaticService) RenderPlotPages(
	analyzerNames []string,
	results map[string]Report,
	outputDir string,
) ([]plotpage.PageMeta, error) {
	mkErr := os.MkdirAll(outputDir, plotDirPerm)
	if mkErr != nil {
		return nil, fmt.Errorf("create plot output dir: %w", mkErr)
	}

	renderer := &plotpage.MultiPageRenderer{
		OutputDir: outputDir,
		Title:     plotPageTitle,
		Theme:     plotpage.ThemeDark,
	}

	pages := make([]plotpage.PageMeta, 0, len(analyzerNames))

	for _, name := range analyzerNames {
		report, ok := results[name]
		if !ok {
			continue
		}

		analyzer := svc.FindAnalyzer(name)
		if analyzer == nil {
			continue
		}

		fullID := analyzer.Descriptor().ID
		sectionFn := PlotSectionsFor(fullID)

		if sectionFn == nil {
			continue
		}

		sections, secErr := sectionFn(report)
		if secErr != nil {
			continue
		}

		safeID := strings.ReplaceAll(fullID, plotIDSep, plotSafeIDSep)

		pageErr := renderer.RenderAnalyzerPage(safeID, fullID, sections)
		if pageErr != nil {
			return nil, fmt.Errorf("render static plot page %s: %w", fullID, pageErr)
		}

		pages = append(pages, plotpage.PageMeta{
			ID:    safeID,
			Title: fullID,
		})
	}

	return pages, nil
}

// reportJSONFilename is the name of the machine-readable JSON report emitted alongside plot pages.
const reportJSONFilename = "report.json"

// reportJSONPerm is the file permission for report.json.
const reportJSONPerm = 0o640

// FormatPlotPages renders multi-page HTML plot output to outputDir.
// Each analyzer gets its own HTML page plus an index page with navigation.
// Also emits report.json with the raw analysis results for external dashboards.
// FRD: specs/frds/FRD-20260312-static-plot-multipage.md.
// FRD: specs/frds/FRD-20260328-report-json-emission.md.
func (svc *StaticService) FormatPlotPages(
	analyzerNames []string,
	results map[string]Report,
	outputDir string,
) error {
	pages, err := svc.RenderPlotPages(analyzerNames, results, outputDir)
	if err != nil {
		return err
	}

	mpRenderer := &plotpage.MultiPageRenderer{
		OutputDir: outputDir,
		Title:     plotPageTitle,
		Theme:     plotpage.ThemeDark,
	}

	indexErr := mpRenderer.RenderIndex(pages)
	if indexErr != nil {
		return indexErr
	}

	return writeReportJSON(results, outputDir)
}

// writeReportJSON writes the analysis results as indented JSON to outputDir/report.json.
func writeReportJSON(results map[string]Report, outputDir string) error {
	reportPath := filepath.Join(outputDir, reportJSONFilename)

	return storage.WriteAtomic(reportPath, reportJSONPerm, func(w io.Writer) error {
		return textutil.WriteJSON(w, results, true)
	})
}

// ResolveAggregationMode returns the aggregation mode for a given output format.
// Text and compact formats only show summary metrics, so per-item data is skipped.
func ResolveAggregationMode(format string) AggregationMode {
	switch format {
	case FormatText, FormatCompact:
		return AggregationModeSummaryOnly
	default:
		return AggregationModeFull
	}
}

// RunAndFormat resolves analyzer IDs, runs analysis on the given path, and formats the output.
func (svc *StaticService) RunAndFormat(
	ctx context.Context,
	path string,
	analyzerIDs []string,
	format string,
	verbose, noColor bool,
	writer io.Writer,
) error {
	analyzerNames, err := svc.AnalyzerNamesByID(analyzerIDs)
	if err != nil {
		return err
	}

	svc.AggregationMode = ResolveAggregationMode(format)

	results, err := svc.AnalyzeFolder(ctx, path, analyzerNames)
	if err != nil {
		return err
	}

	switch format {
	case FormatJSON:
		return svc.FormatJSON(results, writer)
	case FormatCompact:
		return svc.FormatCompact(results, noColor, writer)
	case FormatYAML, FormatPlot, FormatBinary:
		return svc.FormatPerAnalyzer(analyzerNames, results, format, writer)
	case FormatText:
		return svc.FormatText(results, verbose, noColor, writer)
	default:
		return fmt.Errorf("%w: %s", ErrUnsupportedFormat, format)
	}
}
