// Package commands provides CLI command implementations for codefang.
package commands

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"os"

	"github.com/spf13/cobra"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/cohesion"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/comments"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/common/renderer"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/common/terminal"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/complexity"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/halstead"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/imports"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

// AnalyzeCommand holds the flags for the analyze command.
type AnalyzeCommand struct {
	output       string
	format       string
	analyzerList []string
	verbose      bool
	noColor      bool
}

// NewAnalyzeCommand creates and configures the analyze command.
func NewAnalyzeCommand() *cobra.Command {
	ac := &AnalyzeCommand{}

	cobraCmd := &cobra.Command{
		Use:   "analyze",
		Short: "Analyze code complexity and other metrics",
		Long:  "Analyze code complexity and other metrics from UAST input",
		RunE:  ac.Run,
	}

	// Add flags.
	cobraCmd.Flags().StringVarP(&ac.output, "output", "o", "", "Output file (default: stdout)")
	cobraCmd.Flags().StringVarP(&ac.format, "format", "f", "text", "Output format: text, compact, or json")
	cobraCmd.Flags().StringSliceVarP(&ac.analyzerList, "analyzers", "a", []string{}, "Specific analyzers to run (comma-separated)")
	cobraCmd.Flags().BoolVarP(&ac.verbose, "verbose", "v", false, "Show all items without truncation")
	cobraCmd.Flags().BoolVar(&ac.noColor, "no-color", false, "Disable colored output")

	return cobraCmd
}

// Run executes the analyze command.
func (ac *AnalyzeCommand) Run(_ *cobra.Command, _ []string) error {
	// Create input reader.
	inputReader := ac.createInputReader()

	// Initialize analyzer service.
	analyzerService := ac.newService()

	// Run analysis and format results.
	return analyzerService.AnalyzeAndFormat(inputReader, ac.analyzerList, ac.format, ac.verbose, ac.noColor, ac.createOutputWriter())
}

// newService creates a new analyzer service.
func (ac *AnalyzeCommand) newService() *Service {
	return &Service{
		availableAnalyzers: []analyze.StaticAnalyzer{
			complexity.NewAnalyzer(),
			comments.NewAnalyzer(),
			halstead.NewAnalyzer(),
			cohesion.NewAnalyzer(),
			imports.NewAnalyzer(),
		},
	}
}

// createInputReader creates an input reader (stdin or file).
func (ac *AnalyzeCommand) createInputReader() *os.File {
	return os.Stdin
}

// createOutputWriter creates an output writer (stdout or file).
func (ac *AnalyzeCommand) createOutputWriter() *os.File {
	if ac.output == "" {
		return os.Stdout
	}

	file, err := os.Create(ac.output)
	if err != nil {
		fmt.Fprintf(os.Stderr, "Error creating output file: %v\n", err)

		return os.Stdout
	}

	return file
}

// Service provides a high-level interface for running analysis.
type Service struct {
	availableAnalyzers []analyze.StaticAnalyzer
}

// Format mode constants.
const (
	FormatText    = "text"
	FormatCompact = "compact"
	FormatJSON    = "json"
)

// AnalyzeAndFormat runs analysis and formats the results.
func (svc *Service) AnalyzeAndFormat(
	input io.Reader, analyzerList []string, format string, verbose, noColor bool, writer io.Writer,
) error {
	results, err := svc.Analyze(input, analyzerList)
	if err != nil {
		return fmt.Errorf("analysis failed: %w", err)
	}

	switch format {
	case FormatJSON:
		return svc.formatJSON(results, writer)
	case FormatCompact:
		return svc.formatCompact(results, noColor, writer)
	default:
		return svc.formatText(results, verbose, noColor, writer)
	}
}

// Analyze runs analysis on UAST input and returns aggregated results.
func (svc *Service) Analyze(input io.Reader, analyzerList []string) (map[string]analyze.Report, error) {
	analyzersToRun := svc.resolveAnalyzerList(analyzerList)
	aggregators := svc.initAggregators(analyzersToRun)

	err := svc.processInputNodes(input, analyzersToRun, aggregators)
	if err != nil {
		return nil, err
	}

	return buildFinalResults(aggregators), nil
}

func (svc *Service) resolveAnalyzerList(analyzerList []string) []string {
	if len(analyzerList) > 0 {
		return analyzerList
	}

	names := make([]string, 0, len(svc.availableAnalyzers))

	for _, analyzer := range svc.availableAnalyzers {
		names = append(names, analyzer.Name())
	}

	return names
}

func (svc *Service) initAggregators(analyzersToRun []string) map[string]analyze.ResultAggregator {
	aggregators := make(map[string]analyze.ResultAggregator)

	for _, analyzerName := range analyzersToRun {
		analyzer := svc.findAnalyzer(analyzerName)
		if analyzer != nil {
			aggregators[analyzerName] = analyzer.CreateAggregator()
		}
	}

	return aggregators
}

func (svc *Service) processInputNodes(
	input io.Reader, analyzersToRun []string, aggregators map[string]analyze.ResultAggregator,
) error {
	decoder := json.NewDecoder(input)

	for {
		var uastNode *node.Node

		err := decoder.Decode(&uastNode)
		if errors.Is(err, io.EOF) {
			break
		}

		if err != nil {
			return fmt.Errorf("failed to parse UAST input: %w", err)
		}

		results, err := svc.runAnalyzers(context.Background(), uastNode, analyzersToRun)
		if err != nil {
			return fmt.Errorf("failed to run analyzers: %w", err)
		}

		for analyzerName, aggregator := range aggregators {
			if report, isPresent := results[analyzerName]; isPresent {
				aggregator.Aggregate(map[string]analyze.Report{analyzerName: report})
			}
		}
	}

	return nil
}

func buildFinalResults(aggregators map[string]analyze.ResultAggregator) map[string]analyze.Report {
	allResults := make(map[string]analyze.Report)

	for analyzerName, aggregator := range aggregators {
		allResults[analyzerName] = aggregator.GetResult()
	}

	return allResults
}

// formatJSON formats all results as structured JSON using ReportSection data.
func (svc *Service) formatJSON(results map[string]analyze.Report, writer io.Writer) error {
	sections := svc.buildSections(results)
	report := renderer.SectionsToJSON(sections)

	encoder := json.NewEncoder(writer)
	encoder.SetIndent("", "  ")

	err := encoder.Encode(report)
	if err != nil {
		return fmt.Errorf("failed to encode JSON: %w", err)
	}

	return nil
}

// formatText formats all results as human-readable text.
func (svc *Service) formatText(results map[string]analyze.Report, verbose, noColor bool, writer io.Writer) error {
	// Build sections in deterministic order (matching availableAnalyzers registration order).
	sections := svc.buildSections(results)

	config := terminal.NewConfig()
	if noColor {
		config.NoColor = true
	}

	sectionRenderer := renderer.NewSectionRenderer(config.Width, verbose, config.NoColor)

	// Executive summary if multiple analyzers.
	if len(sections) >= renderer.MinSectionsForSummary {
		summary := renderer.NewExecutiveSummary(sections)

		fmt.Fprintln(writer, sectionRenderer.RenderSummary(summary))
	}

	// Individual sections.
	for _, section := range sections {
		fmt.Fprintln(writer)
		fmt.Fprintln(writer, sectionRenderer.Render(section))
	}

	return nil
}

// formatCompact formats all results as single-line-per-analyzer output.
func (svc *Service) formatCompact(results map[string]analyze.Report, noColor bool, writer io.Writer) error {
	sections := svc.buildSections(results)

	config := terminal.NewConfig()
	if noColor {
		config.NoColor = true
	}

	sectionRenderer := renderer.NewSectionRenderer(config.Width, false, config.NoColor)

	for _, section := range sections {
		fmt.Fprintln(writer, sectionRenderer.RenderCompact(section))
	}

	return nil
}

// buildSections creates ReportSection instances from results in deterministic order.
func (svc *Service) buildSections(results map[string]analyze.Report) []analyze.ReportSection {
	sections := make([]analyze.ReportSection, 0, len(results))

	for _, currentAnalyzer := range svc.availableAnalyzers {
		report, found := results[currentAnalyzer.Name()]
		if !found {
			continue
		}

		if provider, isProvider := currentAnalyzer.(analyze.ReportSectionProvider); isProvider {
			sections = append(sections, provider.CreateReportSection(report))
		}
	}

	return sections
}

// runAnalyzers runs the specified analyzers on a single UAST node.
func (svc *Service) runAnalyzers(ctx context.Context, uastNode *node.Node, analyzerList []string) (map[string]analyze.Report, error) {
	factory := analyze.NewFactory(svc.availableAnalyzers)

	return factory.RunAnalyzers(ctx, uastNode, analyzerList)
}

// findAnalyzer finds an analyzer by name.
func (svc *Service) findAnalyzer(name string) analyze.StaticAnalyzer { //nolint:ireturn // returns interface by design
	for _, analyzer := range svc.availableAnalyzers {
		if analyzer.Name() == name {
			return analyzer
		}
	}

	return nil
}
