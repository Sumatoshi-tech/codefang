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

type Report = map[string]any

// Thresholds represents color-coded thresholds for multiple metrics
// Structure: {"metric_name": {"red": value, "yellow": value, "green": value}}
type Thresholds = map[string]map[string]any

// Analyzer is the common base interface for all analyzers
type Analyzer interface {
	Name() string
	Flag() string
	Description() string

	// Configuration
	ListConfigurationOptions() []pipeline.ConfigurationOption
	Configure(facts map[string]interface{}) error
}

// StaticAnalyzer interface defines the contract for UAST-based static analysis
type StaticAnalyzer interface {
	Analyzer

	// Core analysis methods
	Analyze(root *node.Node) (Report, error)
	Thresholds() Thresholds

	// Aggregation methods
	CreateAggregator() ResultAggregator

	// Formatting methods
	FormatReport(report Report, writer io.Writer) error
	FormatReportJSON(report Report, writer io.Writer) error
}

// VisitorProvider enables single-pass traversal optimization
type VisitorProvider interface {
	CreateVisitor() AnalysisVisitor
}

// ResultAggregator defines the interface for aggregating analyzer results
type ResultAggregator interface {
	Aggregate(results map[string]Report)
	GetResult() Report
}

type Factory struct {
	analyzers   map[string]StaticAnalyzer
	maxParallel int
}

func (f *Factory) RegisterAnalyzer(analyzer StaticAnalyzer) {
	f.analyzers[analyzer.Name()] = analyzer
}

func (f *Factory) RunAnalyzer(name string, root *node.Node) (Report, error) {
	analyzer, ok := f.analyzers[name]

	if !ok {
		return nil, fmt.Errorf("no registered analyzer with name=%s", name)
	}

	return analyzer.Analyze(root)
}

func (f *Factory) RunAnalyzers(ctx context.Context, root *node.Node, analyzers []string) (map[string]Report, error) {
	// Initialize containers
	combinedReport := make(map[string]Report)
	reportMu := sync.Mutex{}
	
	visitors := make([]NodeVisitor, 0)
	visitorAnalyzers := make(map[string]AnalysisVisitor)
	legacyAnalyzers := make([]string, 0)
	
	// Pre-check and categorization
	for _, name := range analyzers {
		analyzer, ok := f.analyzers[name]
		if !ok {
			return nil, fmt.Errorf("no registered analyzer with name=%s", name)
		}

		if vp, ok := analyzer.(VisitorProvider); ok {
			v := vp.CreateVisitor()
			visitors = append(visitors, v)
			visitorAnalyzers[name] = v
		} else {
			legacyAnalyzers = append(legacyAnalyzers, name)
		}
	}

	// Setup concurrency
	// 1 task for all visitors + 1 task per legacy analyzer
	totalTasks := 0
	if len(visitors) > 0 {
		totalTasks++
	}
	totalTasks += len(legacyAnalyzers)
	
	if totalTasks == 0 {
		return combinedReport, nil
	}

	if ctx.Err() != nil {
		return nil, ctx.Err()
	}

	// If single task or no parallelism requested, run sequentially
	if totalTasks == 1 || f.maxParallel <= 1 {
		return f.runSequentially(ctx, root, visitors, visitorAnalyzers, legacyAnalyzers)
	}

	// Parallel execution
	errs := make([]string, 0)
	errMu := sync.Mutex{}
	wg := sync.WaitGroup{}
	
	// Semaphore to limit concurrency
	sem := make(chan struct{}, f.maxParallel)
	
	// Task 1: Visitors (Grouped)
	if len(visitors) > 0 {
		wg.Add(1)
		go func() {
			defer wg.Done()
			select {
			case sem <- struct{}{}:
				defer func() { <-sem }()
			case <-ctx.Done():
				return
			}
			
			if err := f.runVisitors(root, visitors); err != nil {
				errMu.Lock()
				errs = append(errs, fmt.Sprintf("visitors error: %v", err))
				errMu.Unlock()
				return
			}
			
			// Collect reports
			reportMu.Lock()
			for name, v := range visitorAnalyzers {
				combinedReport[name] = v.GetReport()
			}
			reportMu.Unlock()
		}()
	}
	
	// Task 2..N: Legacy Analyzers
	for _, name := range legacyAnalyzers {
		name := name // capture loop var
		wg.Add(1)
		go func() {
			defer wg.Done()
			select {
			case sem <- struct{}{}:
				defer func() { <-sem }()
			case <-ctx.Done():
				return
			}
			
			// Check context before work
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
		return nil, ctx.Err()
	}
	
	if len(errs) > 0 {
		return nil, fmt.Errorf("analysis failed: %s", strings.Join(errs, "; "))
	}
	
	return combinedReport, nil
}

func (f *Factory) runSequentially(ctx context.Context, root *node.Node, visitors []NodeVisitor, visitorAnalyzers map[string]AnalysisVisitor, legacyAnalyzers []string) (map[string]Report, error) {
	combinedReport := make(map[string]Report)
	
	if ctx.Err() != nil {
		return nil, ctx.Err()
	}

	if len(visitors) > 0 {
		if err := f.runVisitors(root, visitors); err != nil {
			return nil, err
		}
		for name, v := range visitorAnalyzers {
			combinedReport[name] = v.GetReport()
		}
	}
	
	for _, name := range legacyAnalyzers {
		if ctx.Err() != nil {
			return nil, ctx.Err()
		}
		
		report, err := f.RunAnalyzer(name, root)
		if err != nil {
			return nil, err
		}
		combinedReport[name] = report
	}
	
	return combinedReport, nil
}

func (f *Factory) runVisitors(root *node.Node, visitors []NodeVisitor) error {
	traverser := NewMultiAnalyzerTraverser()
	for _, v := range visitors {
		traverser.RegisterVisitor(v)
	}
	traverser.Traverse(root)
	return nil
}

func NewFactory(analyzers []StaticAnalyzer) *Factory {
	f := &Factory{
		analyzers:   make(map[string]StaticAnalyzer),
		maxParallel: runtime.NumCPU(),
	}

	for _, a := range analyzers {
		f.RegisterAnalyzer(a)
	}

	return f
}

// WithMaxParallelism sets the maximum number of parallel analyzers
func (f *Factory) WithMaxParallelism(n int) *Factory {
	if n < 1 {
		n = 1
	}
	f.maxParallel = n
	return f
}
