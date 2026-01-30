// Package analyze provides analyze functionality.
package analyze

import (
	"context"
	"fmt"
	"io"
	"runtime"
	"strings"
	"sync"

	"github.com/Sumatoshi-tech/codefang/pkg/pipeline"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

// Report is a map of string keys to arbitrary values representing analysis output.
type Report = map[string]any

// Thresholds represents color-coded thresholds for multiple metrics
// Structure: {"metric_name": {"red": value, "yellow": value, "green": value}}.
type Thresholds = map[string]map[string]any

// Analyzer is the common base interface for all analyzers.
type Analyzer interface {
	Name() string
	Flag() string
	Description() string

	// Configuration.
	ListConfigurationOptions() []pipeline.ConfigurationOption
	Configure(facts map[string]any) error
}

// StaticAnalyzer interface defines the contract for UAST-based static analysis.
type StaticAnalyzer interface { //nolint:interfacebloat // interface methods are all needed.
	Analyzer

	// Core analysis methods.
	Analyze(root *node.Node) (Report, error)
	Thresholds() Thresholds

	// Aggregation methods.
	CreateAggregator() ResultAggregator

	// Formatting methods.
	FormatReport(report Report, writer io.Writer) error
	FormatReportJSON(report Report, writer io.Writer) error
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

// RegisterAnalyzer adds an analyzer to the registry.
func (f *Factory) RegisterAnalyzer(analyzer StaticAnalyzer) {
	f.analyzers[analyzer.Name()] = analyzer
}

// RunAnalyzer executes the specified analyzer.
func (f *Factory) RunAnalyzer(name string, root *node.Node) (Report, error) {
	analyzer, ok := f.analyzers[name]

	if !ok {
		return nil, fmt.Errorf("no registered analyzer with name=%s", name) //nolint:err113 // dynamic error is acceptable here.
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
			return nil, fmt.Errorf("no registered analyzer with name=%s", name) //nolint:err113 // dynamic error is acceptable here.
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

// runParallel executes visitor-based and legacy analyzers concurrently.
//
//nolint:funlen,gocognit // long but straightforward parallel dispatch.
func (f *Factory) runParallel(ctx context.Context, root *node.Node, cats *analyzerCategories) (map[string]Report, error) {
	combinedReport := make(map[string]Report)
	reportMu := sync.Mutex{}
	errs := make([]string, 0)
	errMu := sync.Mutex{}
	wg := sync.WaitGroup{}
	sem := make(chan struct{}, f.maxParallel)

	if len(cats.visitors) > 0 {
		wg.Add(1)

		go func() {
			defer wg.Done()

			select {
			case sem <- struct{}{}:
				defer func() { <-sem }()
			case <-ctx.Done():
				return
			}

			err := f.runVisitors(root, cats.visitors)
			if err != nil {
				errMu.Lock()

				errs = append(errs, fmt.Sprintf("visitors error: %v", err))

				errMu.Unlock()

				return
			}

			reportMu.Lock()

			for name, v := range cats.visitorAnalyzers {
				combinedReport[name] = v.GetReport()
			}

			reportMu.Unlock()
		}()
	}

	for _, name := range cats.legacyAnalyzers {
		wg.Add(1)

		go func() {
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
				errMu.Lock()

				errs = append(errs, fmt.Sprintf("analyzer %s error: %v", name, err))

				errMu.Unlock()

				return
			}

			reportMu.Lock()

			combinedReport[name] = report

			reportMu.Unlock()
		}()
	}

	wg.Wait()

	if ctx.Err() != nil {
		return nil, fmt.Errorf("runanalyzers: %w", ctx.Err())
	}

	if len(errs) > 0 {
		return nil, fmt.Errorf("analysis failed: %s", strings.Join(errs, "; ")) //nolint:err113 // dynamic error is acceptable here.
	}

	return combinedReport, nil
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
		err := f.runVisitors(root, visitors)
		if err != nil {
			return nil, err
		}

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

//nolint:unparam // parameter is needed for interface compliance.
func (f *Factory) runVisitors(root *node.Node, visitors []NodeVisitor) error {
	traverser := NewMultiAnalyzerTraverser()
	for _, v := range visitors {
		traverser.RegisterVisitor(v)
	}

	traverser.Traverse(root)

	return nil
}

// NewFactory creates a new factory instance.
func NewFactory(analyzers []StaticAnalyzer) *Factory { //nolint:funcorder // function order is intentional.
	factory := &Factory{
		analyzers:   make(map[string]StaticAnalyzer),
		maxParallel: runtime.NumCPU(),
	}

	for _, a := range analyzers {
		factory.RegisterAnalyzer(a)
	}

	return factory
}
