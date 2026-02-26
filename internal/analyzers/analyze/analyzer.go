// Package analyze provides analyze functionality.
package analyze

import (
	"context"
	"errors"
	"fmt"
	"io"
	"runtime"
	"strings"
	"sync"

	"github.com/Sumatoshi-tech/codefang/pkg/pipeline"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

// ErrUnregisteredAnalyzer indicates that no analyzer with the given name is registered.
var ErrUnregisteredAnalyzer = errors.New("no registered analyzer with name")

// ErrAnalysisFailed indicates that one or more analyzers failed during parallel execution.
var ErrAnalysisFailed = errors.New("analysis failed")

// ErrNilRootNode indicates that a nil root node was passed to an analyzer.
var ErrNilRootNode = errors.New("root node is nil")

// Report is a map of string keys to arbitrary values representing analysis output.
type Report = map[string]any

// ReportFunctionList extracts a []map[string]any from a report key.
// Handles both direct typed values and JSON-decoded []any slices.
func ReportFunctionList(report Report, key string) ([]map[string]any, bool) {
	val, exists := report[key]
	if !exists {
		return nil, false
	}

	if typed, ok := val.([]map[string]any); ok {
		return typed, true
	}

	raw, ok := val.([]any)
	if !ok {
		return nil, false
	}

	result := make([]map[string]any, 0, len(raw))

	for _, item := range raw {
		if m, mOK := item.(map[string]any); mOK {
			result = append(result, m)
		}
	}

	return result, len(result) > 0
}

// Thresholds represents color-coded thresholds for multiple metrics
// Structure: {"metric_name": {"red": value, "yellow": value, "green": value}}.
type Thresholds = map[string]map[string]any

// Analyzer is the common base interface for all analyzers.
type Analyzer interface {
	Name() string
	Flag() string
	Descriptor() Descriptor

	// Configuration.
	ListConfigurationOptions() []pipeline.ConfigurationOption
	Configure(facts map[string]any) error
}

// StaticAnalyzer interface defines the contract for UAST-based static analysis.
type StaticAnalyzer interface {
	Analyzer

	Analyze(root *node.Node) (Report, error)
	Thresholds() Thresholds

	// Aggregation methods.
	CreateAggregator() ResultAggregator

	// Formatting methods.
	FormatReport(report Report, writer io.Writer) error
	FormatReportJSON(report Report, writer io.Writer) error
	FormatReportYAML(report Report, writer io.Writer) error
	FormatReportPlot(report Report, writer io.Writer) error
	FormatReportBinary(report Report, writer io.Writer) error
}

// VisitorProvider enables single-pass traversal optimization.
type VisitorProvider interface {
	CreateVisitor() AnalysisVisitor
}

// ResultAggregator defines the interface for aggregating analyzer results.
type ResultAggregator interface {
	Aggregate(results map[string]Report)
	GetResult() Report
}

// Factory manages registration and execution of static analyzers.
type Factory struct {
	analyzers   map[string]StaticAnalyzer
	maxParallel int
}

// NewFactory creates a new factory instance.
func NewFactory(analyzers []StaticAnalyzer) *Factory {
	factory := &Factory{
		analyzers:   make(map[string]StaticAnalyzer),
		maxParallel: runtime.NumCPU(),
	}

	for _, a := range analyzers {
		factory.RegisterAnalyzer(a)
	}

	return factory
}

// RegisterAnalyzer adds an analyzer to the registry.
func (f *Factory) RegisterAnalyzer(analyzer StaticAnalyzer) {
	f.analyzers[analyzer.Name()] = analyzer
}

// RunAnalyzer executes the specified analyzer.
func (f *Factory) RunAnalyzer(name string, root *node.Node) (Report, error) {
	analyzer, ok := f.analyzers[name]

	if !ok {
		return nil, fmt.Errorf("%w: %s", ErrUnregisteredAnalyzer, name)
	}

	return analyzer.Analyze(root)
}

// analyzerCategories holds categorized analyzers for dispatch.
type analyzerCategories struct {
	visitors         []NodeVisitor
	visitorAnalyzers map[string]AnalysisVisitor
	legacyAnalyzers  []string
}

// categorizeAnalyzers splits analyzers into visitor-based and legacy categories.
func (f *Factory) categorizeAnalyzers(analyzers []string) (*analyzerCategories, error) {
	cats := &analyzerCategories{
		visitors:         make([]NodeVisitor, 0),
		visitorAnalyzers: make(map[string]AnalysisVisitor),
		legacyAnalyzers:  make([]string, 0),
	}

	for _, name := range analyzers {
		analyzer, ok := f.analyzers[name]
		if !ok {
			return nil, fmt.Errorf("%w: %s", ErrUnregisteredAnalyzer, name)
		}

		if vp, isVP := analyzer.(VisitorProvider); isVP {
			v := vp.CreateVisitor()
			cats.visitors = append(cats.visitors, v)
			cats.visitorAnalyzers[name] = v
		} else {
			cats.legacyAnalyzers = append(cats.legacyAnalyzers, name)
		}
	}

	return cats, nil
}

func (c *analyzerCategories) totalTasks() int {
	total := len(c.legacyAnalyzers)
	if len(c.visitors) > 0 {
		total++
	}

	return total
}

// RunAnalyzers runs the specified analyzers on the given UAST root node.
func (f *Factory) RunAnalyzers(ctx context.Context, root *node.Node, analyzers []string) (map[string]Report, error) {
	cats, err := f.categorizeAnalyzers(analyzers)
	if err != nil {
		return nil, err
	}

	if cats.totalTasks() == 0 {
		return make(map[string]Report), nil
	}

	if ctx.Err() != nil {
		return nil, fmt.Errorf("runanalyzers: %w", ctx.Err())
	}

	if cats.totalTasks() == 1 || f.maxParallel <= 1 {
		return f.runSequentially(ctx, root, cats.visitors, cats.visitorAnalyzers, cats.legacyAnalyzers)
	}

	return f.runParallel(ctx, root, cats)
}

// parallelState holds shared state for parallel analyzer execution.
type parallelState struct {
	combinedReport map[string]Report
	reportMu       sync.Mutex
	errs           []string
	errMu          sync.Mutex
}

// runParallel executes visitor-based and legacy analyzers concurrently.
func (f *Factory) runParallel(ctx context.Context, root *node.Node, cats *analyzerCategories) (map[string]Report, error) {
	state := &parallelState{
		combinedReport: make(map[string]Report),
	}

	var wg sync.WaitGroup

	sem := make(chan struct{}, f.maxParallel)

	if len(cats.visitors) > 0 {
		wg.Add(1)

		go f.runVisitorsParallel(ctx, root, cats, state, sem, &wg)
	}

	for _, name := range cats.legacyAnalyzers {
		wg.Add(1)

		go f.runLegacyParallel(ctx, root, name, state, sem, &wg)
	}

	wg.Wait()

	if ctx.Err() != nil {
		return nil, fmt.Errorf("runanalyzers: %w", ctx.Err())
	}

	if len(state.errs) > 0 {
		return nil, fmt.Errorf("%w: %s", ErrAnalysisFailed, strings.Join(state.errs, "; "))
	}

	return state.combinedReport, nil
}

// runVisitorsParallel runs visitor-based analyzers as a single parallel task.
func (f *Factory) runVisitorsParallel(
	ctx context.Context, root *node.Node, cats *analyzerCategories,
	state *parallelState, sem chan struct{}, wg *sync.WaitGroup,
) {
	defer wg.Done()

	select {
	case sem <- struct{}{}:
		defer func() { <-sem }()
	case <-ctx.Done():
		return
	}

	f.runVisitors(root, cats.visitors)

	state.reportMu.Lock()

	for name, v := range cats.visitorAnalyzers {
		state.combinedReport[name] = v.GetReport()
	}

	state.reportMu.Unlock()
}

// runLegacyParallel runs a single legacy analyzer as a parallel task.
func (f *Factory) runLegacyParallel(
	ctx context.Context, root *node.Node, name string,
	state *parallelState, sem chan struct{}, wg *sync.WaitGroup,
) {
	defer wg.Done()

	select {
	case sem <- struct{}{}:
		defer func() { <-sem }()
	case <-ctx.Done():
		return
	}

	if ctx.Err() != nil {
		return
	}

	report, err := f.RunAnalyzer(name, root)
	if err != nil {
		state.errMu.Lock()
		state.errs = append(state.errs, fmt.Sprintf("analyzer %s error: %v", name, err))
		state.errMu.Unlock()

		return
	}

	state.reportMu.Lock()
	state.combinedReport[name] = report
	state.reportMu.Unlock()
}

func (f *Factory) runSequentially(
	ctx context.Context,
	root *node.Node,
	visitors []NodeVisitor,
	visitorAnalyzers map[string]AnalysisVisitor,
	legacyAnalyzers []string,
) (map[string]Report, error) {
	combinedReport := make(map[string]Report)

	if ctx.Err() != nil {
		return nil, fmt.Errorf("runsequentially: %w", ctx.Err())
	}

	if len(visitors) > 0 {
		f.runVisitors(root, visitors)

		for name, v := range visitorAnalyzers {
			combinedReport[name] = v.GetReport()
		}
	}

	for _, name := range legacyAnalyzers {
		if ctx.Err() != nil {
			return nil, fmt.Errorf("runsequentially: %w", ctx.Err())
		}

		report, err := f.RunAnalyzer(name, root)
		if err != nil {
			return nil, err
		}

		combinedReport[name] = report
	}

	return combinedReport, nil
}

func (f *Factory) runVisitors(root *node.Node, visitors []NodeVisitor) {
	traverser := NewMultiAnalyzerTraverser()
	for _, v := range visitors {
		traverser.RegisterVisitor(v)
	}

	traverser.Traverse(root)
}
