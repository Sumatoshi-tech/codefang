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
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/plumbing/pathpolicy"
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
	UASTAnalyzers    []StaticAnalyzer
	RawFileAnalyzers []RawFileAnalyzer

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

	// LanguageGlobs restricts the directory walk to files whose basename
	// matches any of the given fnmatch-style globs (e.g. "*.go",
	// "Dockerfile"). Built from --languages via langpath.Globs. Empty or
	// nil disables the filter — default behavior.
	LanguageGlobs []string

	// PathPolicy carries vendor / generated / extra-prefix exclusion
	// rules shared across phases. The zero value excludes
	// enry.IsVendor and pathfilter-detected generated files by
	// default.
	PathPolicy pathpolicy.Options

	// perFileResults is populated after AnalyzeFolder when PerFile is true.
	// Keyed by analyzer name → file path → per-file report.
	perFileResults map[string]map[string]Report

	// analysisRootPath is the root path used in the last AnalyzeFolder call.
	// Used by FormatJSON to make per-file paths relative.
	analysisRootPath string
}

// NewStaticService creates a StaticService with the given analyzers.
func NewStaticService(uastAnalyzers []StaticAnalyzer, rawAnalyzers []RawFileAnalyzer) *StaticService {
	return &StaticService{UASTAnalyzers: uastAnalyzers, RawFileAnalyzers: rawAnalyzers}
}

// allFormattable returns a merged, deterministically-ordered slice of all analyzers
// that satisfy FormattableAnalyzer (UAST first, then raw-file).
func (svc *StaticService) allFormattable() []FormattableAnalyzer {
	result := make([]FormattableAnalyzer, 0, len(svc.UASTAnalyzers)+len(svc.RawFileAnalyzers))

	for _, a := range svc.UASTAnalyzers {
		result = append(result, a)
	}

	for _, a := range svc.RawFileAnalyzers {
		result = append(result, a)
	}

	return result
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

// analysisPipelineState threads shared state through pipeline phases.
type analysisPipelineState struct {
	rootPath       string
	analyzersToRun []string
	aggregators    map[string]ResultAggregator
}

// AnalyzeFolder runs static analyzers for supported files in a folder tree.
// Executes raw-file and UAST phases sequentially via pipeline.RunPhases.
func (svc *StaticService) AnalyzeFolder(ctx context.Context, rootPath string, analyzerList []string) (map[string]Report, error) {
	svc.analysisRootPath = rootPath

	ctx, cancel := context.WithCancel(ctx)
	defer cancel()

	state := analysisPipelineState{
		rootPath:       rootPath,
		analyzersToRun: svc.resolveAnalyzerList(analyzerList),
	}
	state.aggregators = svc.initAggregators(state.analyzersToRun)

	state, err := pipeline.RunPhases(ctx, state,
		pipeline.PhaseFunc[analysisPipelineState](svc.rawFilePhase),
		pipeline.PhaseFunc[analysisPipelineState](svc.uastPhase),
	)
	if err != nil {
		return nil, err
	}

	results := buildFinalResults(state.aggregators)

	if svc.PerFile {
		svc.perFileResults = extractPerFileResults(state.aggregators)
	}

	return results, nil
}

// rawFilePhase walks ALL files and runs RawFileAnalyzers on file headers.
func (svc *StaticService) rawFilePhase(ctx context.Context, state analysisPipelineState) (analysisPipelineState, error) {
	if len(svc.RawFileAnalyzers) == 0 {
		return state, nil
	}

	// Filter to only requested raw-file analyzers.
	rawNames := svc.requestedRawFileAnalyzers(state.analyzersToRun)
	if len(rawNames) == 0 {
		return state, nil
	}

	var mu sync.Mutex

	walkErr := filepath.WalkDir(state.rootPath, func(path string, entry os.DirEntry, err error) error {
		if ctx.Err() != nil {
			return ctx.Err()
		}

		skip, skipErr := skipAllFilesEntry(entry, err)
		if skip || skipErr != nil {
			return skipErr
		}

		if !matchesLanguageGlobs(path, svc.LanguageGlobs) {
			return nil
		}

		if pathpolicy.Exclude(path, nil, svc.PathPolicy) {
			return nil
		}

		classifyFile(path, rawNames, state.aggregators, &mu, state.rootPath)

		return nil
	})
	if walkErr != nil {
		return state, fmt.Errorf("raw-file phase walk %s: %w", state.rootPath, walkErr)
	}

	return state, nil
}

// requestedRawFileAnalyzers returns RawFileAnalyzers whose names appear in the requested list.
func (svc *StaticService) requestedRawFileAnalyzers(requested []string) []RawFileAnalyzer {
	nameSet := make(map[string]struct{}, len(requested))
	for _, n := range requested {
		nameSet[n] = struct{}{}
	}

	var result []RawFileAnalyzer

	for _, a := range svc.RawFileAnalyzers {
		if _, ok := nameSet[a.Name()]; ok {
			result = append(result, a)
		}
	}

	return result
}

// uastPhase streams UAST-supported files and runs StaticAnalyzers in parallel.
func (svc *StaticService) uastPhase(ctx context.Context, state analysisPipelineState) (analysisPipelineState, error) {
	uastNames := svc.requestedUASTAnalyzers(state.analyzersToRun)
	if len(uastNames) == 0 {
		return state, nil
	}

	var fileCounter atomic.Int64

	fileCh := make(chan string, streamFilesBufSize)
	walkErrCh := make(chan error, 1)

	go func() {
		walkErrCh <- svc.streamFiles(ctx, state.rootPath, fileCh)
	}()

	poolErr := svc.analyzeFilesParallel(ctx, fileCh, uastNames, state.aggregators, &fileCounter, state.rootPath)

	walkErr := <-walkErrCh

	if poolErr != nil {
		return state, poolErr
	}

	if walkErr != nil {
		return state, walkErr
	}

	svc.emitProgress(fileCounter.Load(), state.aggregators, ProgressPhaseComplete)

	return state, nil
}

// requestedUASTAnalyzers returns names of UAST analyzers that appear in the requested list.
func (svc *StaticService) requestedUASTAnalyzers(requested []string) []string {
	nameSet := make(map[string]struct{}, len(svc.UASTAnalyzers))
	for _, a := range svc.UASTAnalyzers {
		nameSet[a.Name()] = struct{}{}
	}

	result := make([]string, 0, len(requested))

	for _, name := range requested {
		if _, ok := nameSet[name]; ok {
			result = append(result, name)
		}
	}

	return result
}

// runUASTAnalysis runs UAST-based analyzers with file streaming and parallel parsing.
// streamFiles walks the directory tree and sends UAST-supported file paths on fileCh.
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

		if !matchesLanguageGlobs(path, svc.LanguageGlobs) {
			return nil
		}

		if pathpolicy.Exclude(path, nil, svc.PathPolicy) {
			return nil
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
	rootPath string,
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

			StampSourceFile(reportMap, filePath, rootPath)
			StampLanguage(reportMap, parser.GetLanguage(filePath))

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
// When rootPath is non-empty, the stamped path is made relative to it.
func StampSourceFile(reports map[string]Report, filePath, rootPath string) {
	stamped := MakeRelativePath(filePath, rootPath)
	dir := filepath.Dir(stamped)

	for _, report := range reports {
		report[SourceFileKey] = stamped
		report[DirectoryKey] = dir

		for key, val := range report {
			switch v := val.(type) {
			case TypedCollection:
				v.SourceFile = stamped
				v.Directory = dir
				report[key] = v
			case []map[string]any:
				for _, item := range v {
					item[SourceFileKey] = stamped
					item[DirectoryKey] = dir
				}
			}
		}
	}
}

// StampLanguage adds "_language" metadata to every collection item in each report.
func StampLanguage(reports map[string]Report, language string) {
	if language == "" {
		return
	}

	for _, report := range reports {
		report[LanguageKey] = language

		for key, val := range report {
			switch v := val.(type) {
			case TypedCollection:
				v.Language = language
				report[key] = v
			case []map[string]any:
				for _, item := range v {
					item[LanguageKey] = language
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

	uastNode, parseErr := parser.Parse(ctx, path, content)
	if parseErr != nil {
		return nil, fmt.Errorf("parse %s: %w", path, parseErr)
	}

	results, runErr := svc.runAnalyzers(ctx, uastNode, analyzersToRun)

	node.ReleaseTree(uastNode)

	if runErr != nil {
		return nil, fmt.Errorf("run analyzers for %s: %w", path, runErr)
	}

	return results, nil
}

// contentHeaderSize is the max bytes read per file in the all-files pre-pass.
// Enry needs only a prefix for binary/language detection.
const contentHeaderSize = 8192

// skipAllFilesEntry decides if a walk entry should be skipped in the raw-file phase.
func skipAllFilesEntry(entry os.DirEntry, walkErr error) (bool, error) {
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

	return false, nil
}

// classifyFile runs raw-file analyzers on a single file and aggregates results.
func classifyFile(
	path string,
	analyzers []RawFileAnalyzer,
	aggregators map[string]ResultAggregator,
	mu *sync.Mutex,
	rootPath string,
) {
	header := readFileHeader(path, contentHeaderSize)

	for _, a := range analyzers {
		report, analyzeErr := a.AnalyzeFileContent(path, header)
		if analyzeErr != nil {
			continue
		}

		report[SourceFileKey] = MakeRelativePath(path, rootPath)

		mu.Lock()

		if agg, ok := aggregators[a.Name()]; ok {
			agg.Aggregate(map[string]Report{a.Name(): report})
		}

		mu.Unlock()
	}
}

// readFileHeader reads up to limit bytes from a file. Returns nil on error.
func readFileHeader(path string, limit int) []byte {
	f, err := os.Open(path)
	if err != nil {
		return nil
	}
	defer f.Close()

	buf := make([]byte, limit)

	n, readErr := f.Read(buf)
	if readErr != nil && !errors.Is(readErr, io.EOF) {
		return nil
	}

	return buf[:n]
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

	all := svc.allFormattable()
	names := make([]string, 0, len(all))

	for _, analyzer := range all {
		names = append(names, analyzer.Name())
	}

	return names
}

func (svc *StaticService) initAggregators(analyzersToRun []string) map[string]ResultAggregator {
	aggregators := make(map[string]ResultAggregator)
	byName := svc.analyzersByName()

	for _, analyzerName := range analyzersToRun {
		analyzer, found := byName[analyzerName]
		if !found {
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

	for _, currentAnalyzer := range svc.allFormattable() {
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
	factory := NewFactory(svc.UASTAnalyzers)

	return factory.RunAnalyzers(ctx, uastNode, analyzerList)
}

// analyzersByName builds a name-to-analyzer lookup map from all formattable analyzers.
func (svc *StaticService) analyzersByName() map[string]FormattableAnalyzer {
	all := svc.allFormattable()
	result := make(map[string]FormattableAnalyzer, len(all))

	for _, a := range all {
		result[a.Name()] = a
	}

	return result
}

// AnalyzerNamesByID resolves analyzer descriptor IDs to internal names.
func (svc *StaticService) AnalyzerNamesByID(ids []string) ([]string, error) {
	all := svc.allFormattable()
	idToName := make(map[string]string, len(all))

	for _, analyzer := range all {
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
	byName := svc.analyzersByName()

	for _, analyzerName := range analyzerNames {
		report, ok := results[analyzerName]
		if !ok {
			continue
		}

		analyzer, found := byName[analyzerName]
		if !found {
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
	byName := svc.analyzersByName()

	for _, name := range analyzerNames {
		report, ok := results[name]
		if !ok {
			continue
		}

		analyzer, found := byName[name]
		if !found {
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
