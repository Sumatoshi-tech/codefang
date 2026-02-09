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
func (svc *StaticService) AnalyzeFolder(rootPath string, analyzerList []string) (map[string]Report, error) {
	analyzersToRun := svc.resolveAnalyzerList(analyzerList)
	aggregators := svc.initAggregators(analyzersToRun)

	parser, err := uast.NewParser()
	if err != nil {
		return nil, fmt.Errorf("create parser: %w", err)
	}

	err = filepath.WalkDir(rootPath, func(path string, entry os.DirEntry, walkErr error) error {
		return svc.handleAnalyzeFolderPath(path, entry, walkErr, parser, analyzersToRun, aggregators)
	})
	if err != nil {
		return nil, fmt.Errorf("walk %s: %w", rootPath, err)
	}

	return buildFinalResults(aggregators), nil
}

func (svc *StaticService) handleAnalyzeFolderPath(
	path string,
	entry os.DirEntry,
	walkErr error,
	parser *uast.Parser,
	analyzersToRun []string,
	aggregators map[string]ResultAggregator,
) error {
	skip, err := ShouldSkipFolderNode(path, entry, walkErr, parser)
	if skip || err != nil {
		return err
	}

	reportMap, err := svc.analyzeFile(path, parser, analyzersToRun)
	if err != nil {
		if errors.Is(err, fs.ErrPermission) || errors.Is(err, fs.ErrNotExist) {
			return nil
		}

		return err
	}

	aggregateFolderAnalysis(reportMap, aggregators)

	return nil
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

func (svc *StaticService) analyzeFile(path string, parser *uast.Parser, analyzersToRun []string) (map[string]Report, error) {
	content, err := os.ReadFile(path)
	if err != nil {
		return nil, fmt.Errorf("read %s: %w", path, err)
	}

	uastNode, err := parser.Parse(path, content)
	if err != nil {
		return nil, fmt.Errorf("parse %s: %w", path, err)
	}

	results, err := svc.runAnalyzers(context.Background(), uastNode, analyzersToRun)
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

	results, err := svc.AnalyzeFolder(path, analyzerNames)
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
