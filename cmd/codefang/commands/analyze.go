package commands

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"os"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/cohesion"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/comments"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/common/renderer"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/common/terminal"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/complexity"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/halstead"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/imports"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
	"github.com/spf13/cobra"
)

// AnalyzeCommand holds the flags for the analyze command
type AnalyzeCommand struct {
	output       string
	format       string
	analyzerList []string
	verbose      bool
	noColor      bool
}

// NewAnalyzeCommand creates and configures the analyze command
func NewAnalyzeCommand() *cobra.Command {
	cmd := &AnalyzeCommand{}

	cobraCmd := &cobra.Command{
		Use:   "analyze",
		Short: "Analyze code complexity and other metrics",
		Long:  "Analyze code complexity and other metrics from UAST input",
		RunE:  cmd.Run,
	}

	// Add flags
	cobraCmd.Flags().StringVarP(&cmd.output, "output", "o", "", "Output file (default: stdout)")
	cobraCmd.Flags().StringVarP(&cmd.format, "format", "f", "text", "Output format: text, compact, or json")
	cobraCmd.Flags().StringSliceVarP(&cmd.analyzerList, "analyzers", "a", []string{}, "Specific analyzers to run (comma-separated)")
	cobraCmd.Flags().BoolVarP(&cmd.verbose, "verbose", "v", false, "Show all items without truncation")
	cobraCmd.Flags().BoolVar(&cmd.noColor, "no-color", false, "Disable colored output")

	return cobraCmd
}

// Run executes the analyze command
func (c *AnalyzeCommand) Run(cmd *cobra.Command, args []string) error {
	// Create input reader
	inputReader := c.createInputReader()

	// Initialize analyzer service
	analyzerService := c.newService()

	// Run analysis and format results
	return analyzerService.AnalyzeAndFormat(inputReader, c.analyzerList, c.format, c.verbose, c.noColor, c.createOutputWriter())
}

// newService creates a new analyzer service
func (c *AnalyzeCommand) newService() *Service {
	return &Service{
		availableAnalyzers: []analyze.StaticAnalyzer{
			complexity.NewComplexityAnalyzer(),
			comments.NewCommentsAnalyzer(),
			halstead.NewHalsteadAnalyzer(),
			cohesion.NewCohesionAnalyzer(),
			imports.NewImportsAnalyzer(),
		},
	}
}

// createInputReader creates an input reader (stdin or file)
func (c *AnalyzeCommand) createInputReader() *os.File {
	return os.Stdin
}

// createOutputWriter creates an output writer (stdout or file)
func (c *AnalyzeCommand) createOutputWriter() *os.File {
	if c.output == "" {
		return os.Stdout
	}

	file, err := os.Create(c.output)
	if err != nil {
		fmt.Fprintf(os.Stderr, "Error creating output file: %v\n", err)
		return os.Stdout
	}

	return file
}

// Service provides a high-level interface for running analysis
type Service struct {
	availableAnalyzers []analyze.StaticAnalyzer
}

// Format mode constants
const (
	FormatText    = "text"
	FormatCompact = "compact"
	FormatJSON    = "json"
)

// AnalyzeAndFormat runs analysis and formats the results
func (s *Service) AnalyzeAndFormat(input io.Reader, analyzerList []string, format string, verbose bool, noColor bool, writer io.Writer) error {
	results, err := s.Analyze(input, analyzerList)
	if err != nil {
		return fmt.Errorf("analysis failed: %w", err)
	}

	switch format {
	case FormatJSON:
		return s.formatJSON(results, writer)
	case FormatCompact:
		return s.formatCompact(results, noColor, writer)
	default:
		return s.formatText(results, verbose, noColor, writer)
	}
}

// Analyze runs analysis on UAST input and returns aggregated results
func (s *Service) Analyze(input io.Reader, analyzerList []string) (map[string]analyze.Report, error) {
	// Read multiple JSON objects from input (one per file from uast parse)
	decoder := json.NewDecoder(input)
	allResults := make(map[string]analyze.Report)

	// Initialize aggregators for each analyzer
	aggregators := make(map[string]analyze.ResultAggregator)

	// Determine which analyzers to run
	analyzersToRun := analyzerList
	if len(analyzersToRun) == 0 {
		// Run all available analyzers
		for _, analyzer := range s.availableAnalyzers {
			analyzersToRun = append(analyzersToRun, analyzer.Name())
		}
	}

	// Initialize aggregators for each analyzer
	for _, analyzerName := range analyzersToRun {
		analyzer := s.findAnalyzer(analyzerName)
		if analyzer != nil {
			aggregators[analyzerName] = analyzer.CreateAggregator()
		}
	}

	for {
		var uastNode *node.Node
		err := decoder.Decode(&uastNode)
		if err == io.EOF {
			break
		}
		if err != nil {
			return nil, fmt.Errorf("failed to parse UAST input: %w", err)
		}

		// Run analyzers for this file
		results, err := s.runAnalyzers(context.Background(), uastNode, analyzersToRun)
		if err != nil {
			return nil, fmt.Errorf("failed to run analyzers: %w", err)
		}

		// Aggregate results for each analyzer
		for analyzerName, aggregator := range aggregators {
			if report, ok := results[analyzerName]; ok {
				aggregator.Aggregate(map[string]analyze.Report{analyzerName: report})
			}
		}
	}

	// Build final results from aggregators
	for analyzerName, aggregator := range aggregators {
		allResults[analyzerName] = aggregator.GetResult()
	}

	return allResults, nil
}

// formatJSON formats all results as structured JSON using ReportSection data.
func (s *Service) formatJSON(results map[string]analyze.Report, writer io.Writer) error {
	sections := s.buildSections(results)
	report := renderer.SectionsToJSON(sections)

	encoder := json.NewEncoder(writer)
	encoder.SetIndent("", "  ")
	return encoder.Encode(report)
}

// formatText formats all results as human-readable text
func (s *Service) formatText(results map[string]analyze.Report, verbose bool, noColor bool, writer io.Writer) error {
	// Build sections in deterministic order (matching availableAnalyzers registration order)
	sections := s.buildSections(results)

	config := terminal.NewConfig()
	if noColor {
		config.NoColor = true
	}
	r := renderer.NewSectionRenderer(config.Width, verbose, config.NoColor)

	// Executive summary if multiple analyzers
	if len(sections) >= renderer.MinSectionsForSummary {
		summary := renderer.NewExecutiveSummary(sections)
		fmt.Fprintln(writer, r.RenderSummary(summary))
	}

	// Individual sections
	for _, section := range sections {
		fmt.Fprintln(writer)
		fmt.Fprintln(writer, r.Render(section))
	}

	return nil
}

// formatCompact formats all results as single-line-per-analyzer output
func (s *Service) formatCompact(results map[string]analyze.Report, noColor bool, writer io.Writer) error {
	sections := s.buildSections(results)

	config := terminal.NewConfig()
	if noColor {
		config.NoColor = true
	}
	r := renderer.NewSectionRenderer(config.Width, false, config.NoColor)

	for _, section := range sections {
		fmt.Fprintln(writer, r.RenderCompact(section))
	}

	return nil
}

// buildSections creates ReportSection instances from results in deterministic order.
func (s *Service) buildSections(results map[string]analyze.Report) []analyze.ReportSection {
	sections := make([]analyze.ReportSection, 0, len(results))
	for _, a := range s.availableAnalyzers {
		report, ok := results[a.Name()]
		if !ok {
			continue
		}
		if provider, ok := a.(analyze.ReportSectionProvider); ok {
			sections = append(sections, provider.CreateReportSection(report))
		}
	}
	return sections
}

// runAnalyzers runs the specified analyzers on a single UAST node
func (s *Service) runAnalyzers(ctx context.Context, uastNode *node.Node, analyzerList []string) (map[string]analyze.Report, error) {
	factory := analyze.NewFactory(s.availableAnalyzers)
	return factory.RunAnalyzers(ctx, uastNode, analyzerList)
}

// findAnalyzer finds an analyzer by name
func (s *Service) findAnalyzer(name string) analyze.StaticAnalyzer {
	for _, analyzer := range s.availableAnalyzers {
		if analyzer.Name() == name {
			return analyzer
		}
	}
	return nil
}
