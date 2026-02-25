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
	"sync"

	"github.com/Sumatoshi-tech/codefang/pkg/uast"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

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

	// Renderer provides section-based output rendering.
	// Must be set before calling FormatJSON, FormatText, FormatCompact, or RunAndFormat.
	Renderer StaticRenderer
}

// NewStaticService creates a StaticService with the given analyzers.
func NewStaticService(analyzers []StaticAnalyzer) *StaticService {
	return &StaticService{Analyzers: analyzers}
}

// AnalyzeFolder runs static analyzers for supported files in a folder tree.
// Files are discovered sequentially, then analyzed in parallel using a worker pool.
func (svc *StaticService) AnalyzeFolder(ctx context.Context, rootPath string, analyzerList []string) (map[string]Report, error) {
	analyzersToRun := svc.resolveAnalyzerList(analyzerList)
	aggregators := svc.initAggregators(analyzersToRun)

	files, err := svc.collectFiles(rootPath)
	if err != nil {
		return nil, err
	}

	err = svc.analyzeFilesParallel(ctx, files, analyzersToRun, aggregators)
	if err != nil {
		return nil, err
	}

	return buildFinalResults(aggregators), nil
}

// collectFiles walks the directory tree and returns paths of supported files.
func (svc *StaticService) collectFiles(rootPath string) ([]string, error) {
	parser, err := uast.NewParser()
	if err != nil {
		return nil, fmt.Errorf("create parser: %w", err)
	}

	var files []string

	err = filepath.WalkDir(rootPath, func(path string, entry os.DirEntry, walkErr error) error {
		skip, skipErr := ShouldSkipFolderNode(path, entry, walkErr, parser)
		if skip || skipErr != nil {
			return skipErr
		}

		files = append(files, path)

		return nil
	})
	if err != nil {
		return nil, fmt.Errorf("walk %s: %w", rootPath, err)
	}

	return files, nil
}

// workerState holds shared mutable state for parallel file analysis workers.
type workerState struct {
	mu       sync.Mutex
	firstErr error
}

// setError records the first error encountered by any worker.
func (ws *workerState) setError(err error) {
	ws.mu.Lock()
	defer ws.mu.Unlock()

	if ws.firstErr == nil {
		ws.firstErr = err
	}
}

// analyzeFilesParallel processes files using a pool of workers, each with its own parser.
func (svc *StaticService) analyzeFilesParallel(
	ctx context.Context,
	files []string,
	analyzersToRun []string,
	aggregators map[string]ResultAggregator,
) error {
	numWorkers := max(1, runtime.NumCPU())
	fileChan := make(chan string, numWorkers)
	state := &workerState{}

	var wg sync.WaitGroup

	wg.Add(numWorkers)

	for range numWorkers {
		go svc.fileWorker(ctx, &wg, fileChan, analyzersToRun, aggregators, state)
	}

	for _, filePath := range files {
		fileChan <- filePath
	}

	close(fileChan)
	wg.Wait()

	return state.firstErr
}

// fileWorker is the body of each parallel file analysis goroutine.
func (svc *StaticService) fileWorker(
	ctx context.Context,
	wg *sync.WaitGroup,
	fileChan <-chan string,
	analyzersToRun []string,
	aggregators map[string]ResultAggregator,
	state *workerState,
) {
	defer wg.Done()

	parser, parserErr := uast.NewParser()
	if parserErr != nil {
		state.setError(fmt.Errorf("create worker parser: %w", parserErr))

		for range fileChan {
			continue // Drain remaining items so senders don't block.
		}

		return
	}

	for filePath := range fileChan {
		stopped := svc.processFile(ctx, filePath, parser, analyzersToRun, aggregators, state)
		if stopped {
			return
		}
	}
}

// processFile analyzes a single file and aggregates the results.
// Returns true if the worker should stop due to a fatal error.
func (svc *StaticService) processFile(
	ctx context.Context,
	filePath string,
	parser *uast.Parser,
	analyzersToRun []string,
	aggregators map[string]ResultAggregator,
	state *workerState,
) bool {
	reportMap, analyzeErr := svc.analyzeFile(ctx, filePath, parser, analyzersToRun)
	if analyzeErr != nil {
		if errors.Is(analyzeErr, fs.ErrPermission) || errors.Is(analyzeErr, fs.ErrNotExist) {
			return false
		}

		state.setError(analyzeErr)

		return true
	}

	StampSourceFile(reportMap, filePath)

	state.mu.Lock()
	aggregateFolderAnalysis(reportMap, aggregators)
	state.mu.Unlock()

	return false
}

// StampSourceFile adds "_source_file" metadata to every collection item in each report.
// This allows downstream consumers (e.g., plot generators) to group results by file/package.
func StampSourceFile(reports map[string]Report, filePath string) {
	for _, report := range reports {
		for _, val := range report {
			if collection, ok := val.([]map[string]any); ok {
				for _, item := range collection {
					item["_source_file"] = filePath
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
		if analyzer != nil {
			aggregators[analyzerName] = analyzer.CreateAggregator()
		}
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
