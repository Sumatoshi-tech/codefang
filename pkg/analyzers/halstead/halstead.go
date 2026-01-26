package halstead

import (
	"fmt"
	"io"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/common"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/common/renderer"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/common/terminal"
	"github.com/Sumatoshi-tech/codefang/pkg/pipeline"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

// Configuration constants for Halstead analysis.
const (
	// MaxDepthValue is the default maximum UAST traversal depth for Halstead analysis.
	MaxDepthValue = 10
	magic1000_1   = 1000
	magic30       = 30
	magic5        = 5
	magic5000     = 5000
	magic50000    = 50000
)

// HalsteadAnalyzer provides Halstead complexity measures analysis.
type HalsteadAnalyzer struct {
	// Traverser handles UAST traversal and node finding.
	traverser *common.UASTTraverser
	// Extractor handles data extraction from UAST nodes.
	extractor *common.DataExtractor
	// Metrics handles Halstead metrics calculations.
	metrics *MetricsCalculator
	// Detector handles operator and operand detection.
	detector *OperatorOperandDetector
	// Formatter handles report formatting and output.
	formatter *ReportFormatter
}

// NewHalsteadAnalyzer creates a new HalsteadAnalyzer with common modules.
func NewHalsteadAnalyzer() *HalsteadAnalyzer {
	// Configure UAST traverser with advanced filtering.
	traversalConfig := common.TraversalConfig{
		Filters: []common.NodeFilter{
			{
				Types:    []string{node.UASTFunction, node.UASTMethod},
				Roles:    []string{node.RoleFunction, node.RoleDeclaration},
				MinLines: 1,
			},
		},
		MaxDepth:    MaxDepthValue,
		IncludeRoot: false,
	}

	// Configure data extractor with Halstead-specific extractors.
	extractionConfig := common.ExtractionConfig{
		DefaultExtractors: true,
		NameExtractors: map[string]common.NameExtractor{
			"function_name": common.ExtractFunctionName,
			"operator_name": extractOperatorName,
			"operand_name":  extractOperandName,
		},
	}

	return &HalsteadAnalyzer{
		traverser: common.NewUASTTraverser(traversalConfig),
		extractor: common.NewDataExtractor(extractionConfig),
		metrics:   NewMetricsCalculator(),
		detector:  NewOperatorOperandDetector(),
		formatter: NewReportFormatter(),
	}
}

// extractOperatorName extracts operator name from a node.
func extractOperatorName(n *node.Node) (string, bool) {
	if n == nil {
		return "", false
	}

	return string(n.Type), true
}

// extractOperandName extracts operand name from a node.
func extractOperandName(target *node.Node) (string, bool) {
	if target == nil {
		return "", false
	}

	// Try to extract from token first.
	if target.Token != "" {
		return target.Token, true
	}

	// Try to extract from properties.
	if target.Props != nil {
		if name, ok := target.Props["name"]; ok {
			return name, true
		}
	}

	// Fallback to node type.
	return string(target.Type), true
}

// HalsteadMetrics holds all Halstead complexity measures.
type HalsteadMetrics struct {
	Functions         map[string]*FunctionHalsteadMetrics `json:"functions"`
	EstimatedLength   float64                             `json:"estimated_length"`
	TotalOperators    int                                 `json:"total_operators"`
	TotalOperands     int                                 `json:"total_operands"`
	Vocabulary        int                                 `json:"vocabulary"`
	Length            int                                 `json:"length"`
	DistinctOperators int                                 `json:"distinct_operators"`
	Volume            float64                             `json:"volume"`
	Difficulty        float64                             `json:"difficulty"`
	Effort            float64                             `json:"effort"`
	TimeToProgram     float64                             `json:"time_to_program"`
	DeliveredBugs     float64                             `json:"delivered_bugs"`
	DistinctOperands  int                                 `json:"distinct_operands"`
}

// FunctionHalsteadMetrics contains Halstead metrics for a single function.
type FunctionHalsteadMetrics struct {
	Operands          map[string]int `json:"operands"`
	Operators         map[string]int `json:"operators"`
	Name              string         `json:"name"`
	Length            int            `json:"length"`
	TotalOperands     int            `json:"total_operands"`
	Vocabulary        int            `json:"vocabulary"`
	TotalOperators    int            `json:"total_operators"`
	EstimatedLength   float64        `json:"estimated_length"`
	Volume            float64        `json:"volume"`
	Difficulty        float64        `json:"difficulty"`
	Effort            float64        `json:"effort"`
	TimeToProgram     float64        `json:"time_to_program"`
	DeliveredBugs     float64        `json:"delivered_bugs"`
	DistinctOperands  int            `json:"distinct_operands"`
	DistinctOperators int            `json:"distinct_operators"`
}

// HalsteadConfig holds configuration for Halstead analysis.
type HalsteadConfig struct {
	// IncludeFunctionBreakdown determines whether to include per-function metrics.
	IncludeFunctionBreakdown bool
	// IncludeTimeEstimate determines whether to calculate time to program estimates.
	IncludeTimeEstimate bool
	// IncludeBugEstimate determines whether to calculate delivered bug estimates.
	IncludeBugEstimate bool
}

// Name returns the analyzer name.
func (h *HalsteadAnalyzer) Name() string {
	return "halstead"
}

// Flag returns the CLI flag for the analyzer.
func (h *HalsteadAnalyzer) Flag() string {
	return "halstead-analysis"
}

// Description returns the analyzer description.
func (h *HalsteadAnalyzer) Description() string {
	return "Calculates Halstead complexity metrics."
}

// ListConfigurationOptions returns the configuration options for the analyzer.
func (h *HalsteadAnalyzer) ListConfigurationOptions() []pipeline.ConfigurationOption {
	return []pipeline.ConfigurationOption{}
}

// Configure configures the analyzer.
func (h *HalsteadAnalyzer) Configure(_ map[string]any) error {
	return nil
}

// Thresholds returns the color-coded thresholds for Halstead metrics.
func (h *HalsteadAnalyzer) Thresholds() analyze.Thresholds {
	return analyze.Thresholds{
		"volume": {
			"green":  magic100,
			"yellow": magic1000,
			"red":    magic5000,
		},
		"difficulty": {
			"green":  magic5,
			"yellow": magic15,
			"red":    magic30,
		},
		"effort": {
			"green":  magic1000_1,
			"yellow": magic10000,
			"red":    magic50000,
		},
	}
}

// CreateAggregator returns a new aggregator for Halstead analysis.
func (h *HalsteadAnalyzer) CreateAggregator() analyze.ResultAggregator {
	return NewHalsteadAggregator()
}

// CreateVisitor creates a new visitor for Halstead analysis.
func (h *HalsteadAnalyzer) CreateVisitor() analyze.AnalysisVisitor {
	return NewHalsteadVisitor()
}

// Analyze performs Halstead analysis on the UAST.
func (h *HalsteadAnalyzer) Analyze(root *node.Node) (analyze.Report, error) {
	if root == nil {
		return nil, fmt.Errorf("root node is nil") //nolint:err113,perfsprint // fmt.Sprintf is clearer than string concat.
	}

	functions := h.findFunctions(root)

	if len(functions) == 0 {
		return h.buildEmptyResult("No functions found"), nil
	}

	functionMetrics := h.calculateAllFunctionMetrics(functions)
	fileMetrics := h.calculateFileLevelMetrics(functionMetrics)
	detailedFunctionsTable := h.buildDetailedFunctionsTable(functionMetrics)
	functionDetails := h.buildFunctionDetails(functionMetrics)
	message := h.formatter.GetHalsteadMessage(fileMetrics.Volume, fileMetrics.Difficulty, fileMetrics.Effort)

	return h.buildResult(fileMetrics, detailedFunctionsTable, functionDetails, message), nil
}

// FormatReport formats the analysis report for display.
func (h *HalsteadAnalyzer) FormatReport(report analyze.Report, w io.Writer) error {
	section := NewHalsteadReportSection(report)
	config := terminal.NewConfig()
	r := renderer.NewSectionRenderer(config.Width, false, config.NoColor)

	_, err := fmt.Fprint(w, r.Render(section))
	if err != nil {
		return fmt.Errorf("formatreport: %w", err)
	}

	return nil
}

// FormatReportJSON formats the analysis report as JSON.
func (h *HalsteadAnalyzer) FormatReportJSON(report analyze.Report, w io.Writer) error {
	return h.formatter.FormatReportJSON(report, w)
}

// buildEmptyResult creates an empty result for cases with no functions.
func (h *HalsteadAnalyzer) buildEmptyResult(message string) analyze.Report {
	return common.NewResultBuilder().BuildCustomEmptyResult(map[string]any{
		"total_functions":    0,
		"volume":             0.0,
		"difficulty":         0.0,
		"effort":             0.0,
		"time_to_program":    0.0,
		"delivered_bugs":     0.0,
		"distinct_operators": 0,
		"distinct_operands":  0,
		"total_operators":    0,
		"total_operands":     0,
		"vocabulary":         0,
		"length":             0,
		"estimated_length":   0.0,
		"message":            message,
	})
}

// calculateAllFunctionMetrics calculates metrics for all functions.
func (h *HalsteadAnalyzer) calculateAllFunctionMetrics(functions []*node.Node) map[string]*FunctionHalsteadMetrics {
	functionMetrics := make(map[string]*FunctionHalsteadMetrics)

	for _, fn := range functions {
		funcName := h.getFunctionName(fn)
		funcMetrics := h.calculateFunctionHalsteadMetrics(fn)
		funcMetrics.Name = funcName
		functionMetrics[funcName] = funcMetrics
	}

	return functionMetrics
}

// getFunctionName extracts function name with fallback to anonymous for unnamed functions.
func (h *HalsteadAnalyzer) getFunctionName(fn *node.Node) string {
	funcName := h.extractFunctionName(fn)
	if funcName == "" {
		return "anonymous"
	}

	return funcName
}

// calculateFileLevelMetrics calculates file-level metrics from function metrics.
func (h *HalsteadAnalyzer) calculateFileLevelMetrics(functionMetrics map[string]*FunctionHalsteadMetrics) *HalsteadMetrics {
	fileOperators := make(map[string]int)
	fileOperands := make(map[string]int)

	for _, fn := range functionMetrics {
		h.aggregateOperatorsAndOperandsFromMetrics(fn, fileOperators, fileOperands)
	}

	fileMetrics := &HalsteadMetrics{
		DistinctOperators: len(fileOperators),
		DistinctOperands:  len(fileOperands),
		TotalOperators:    h.metrics.SumMap(fileOperators),
		TotalOperands:     h.metrics.SumMap(fileOperands),
		Functions:         functionMetrics,
	}

	h.metrics.CalculateHalsteadMetrics(fileMetrics)

	return fileMetrics
}

// aggregateOperatorsAndOperandsFromMetrics aggregates operators and operands from function metrics.
func (h *HalsteadAnalyzer) aggregateOperatorsAndOperandsFromMetrics(
	fn *FunctionHalsteadMetrics, operators, operands map[string]int,
) { //nolint:whitespace // multi-line signature.
	for operator, count := range fn.Operators {
		operators[operator] += count
	}

	for operand, count := range fn.Operands {
		operands[operand] += count
	}
}

// buildDetailedFunctionsTable creates the detailed functions table for display.
func (h *HalsteadAnalyzer) buildDetailedFunctionsTable(functionMetrics map[string]*FunctionHalsteadMetrics) []map[string]any {
	detailedFunctionsTable := make([]map[string]any, 0, len(functionMetrics))

	for _, fn := range functionMetrics {
		functionData := h.buildFunctionTableEntry(fn)
		detailedFunctionsTable = append(detailedFunctionsTable, functionData)
	}

	return detailedFunctionsTable
}

// buildFunctionTableEntry creates a single function table entry with metrics and assessments.
func (h *HalsteadAnalyzer) buildFunctionTableEntry(fn *FunctionHalsteadMetrics) map[string]any {
	return map[string]any{
		"name":                  fn.Name,
		"volume":                fn.Volume,
		"difficulty":            fn.Difficulty,
		"effort":                fn.Effort,
		"delivered_bugs":        fn.DeliveredBugs,
		"volume_assessment":     h.formatter.GetVolumeAssessment(fn.Volume),
		"difficulty_assessment": h.formatter.GetDifficultyAssessment(fn.Difficulty),
		"effort_assessment":     h.formatter.GetEffortAssessment(fn.Effort),
		"operators":             fn.Operators,
		"operands":              fn.Operands,
	}
}

// buildFunctionDetails creates simplified function details for result.
func (h *HalsteadAnalyzer) buildFunctionDetails(functionMetrics map[string]*FunctionHalsteadMetrics) []map[string]any {
	functionDetails := make([]map[string]any, 0, len(functionMetrics))

	for _, fn := range functionMetrics {
		functionData := h.buildFunctionDetailEntry(fn)
		functionDetails = append(functionDetails, functionData)
	}

	return functionDetails
}

// buildFunctionDetailEntry creates a single function detail entry with comprehensive metrics.
func (h *HalsteadAnalyzer) buildFunctionDetailEntry(fn *FunctionHalsteadMetrics) map[string]any {
	return map[string]any{
		"name":               fn.Name,
		"volume":             fn.Volume,
		"difficulty":         fn.Difficulty,
		"effort":             fn.Effort,
		"time_to_program":    fn.TimeToProgram,
		"delivered_bugs":     fn.DeliveredBugs,
		"distinct_operators": fn.DistinctOperators,
		"distinct_operands":  fn.DistinctOperands,
		"operators":          fn.Operators,
		"operands":           fn.Operands,
	}
}

// buildResult constructs the final analysis result.
func (h *HalsteadAnalyzer) buildResult(
	fileMetrics *HalsteadMetrics, detailedFunctionsTable, functionDetails []map[string]any, message string,
) analyze.Report { //nolint:whitespace // multi-line signature.
	metrics := map[string]any{
		"volume":             fileMetrics.Volume,
		"difficulty":         fileMetrics.Difficulty,
		"effort":             fileMetrics.Effort,
		"time_to_program":    fileMetrics.TimeToProgram,
		"delivered_bugs":     fileMetrics.DeliveredBugs,
		"distinct_operators": fileMetrics.DistinctOperators,
		"distinct_operands":  fileMetrics.DistinctOperands,
		"total_operators":    fileMetrics.TotalOperators,
		"total_operands":     fileMetrics.TotalOperands,
		"vocabulary":         fileMetrics.Vocabulary,
		"length":             fileMetrics.Length,
		"estimated_length":   fileMetrics.EstimatedLength,
		"total_functions":    len(functionDetails), // Add explicit total_functions for backward compatibility.
	}

	result := common.NewResultBuilder().BuildCollectionResult(
		"halstead",
		"functions",
		detailedFunctionsTable,
		metrics,
		message,
	)

	return result
}

// findFunctions finds all functions using the enhanced traverser.
func (h *HalsteadAnalyzer) findFunctions(root *node.Node) []*node.Node {
	functionNodes := h.traverser.FindNodesByType(root, []string{node.UASTFunction, node.UASTMethod})
	roleNodes := h.traverser.FindNodesByRoles(root, []string{node.RoleFunction})

	allNodes := make(map[*node.Node]bool)
	for _, node := range functionNodes {
		allNodes[node] = true
	}

	for _, node := range roleNodes {
		allNodes[node] = true
	}

	// Convert back to slice.
	functions := make([]*node.Node, 0, len(allNodes))
	for node := range allNodes {
		functions = append(functions, node)
	}

	return functions
}

// extractFunctionName extracts the function name using common extractor.
func (h *HalsteadAnalyzer) extractFunctionName(n *node.Node) string {
	if name, ok := h.extractor.ExtractName(n, "function_name"); ok && name != "" {
		return name
	}

	if name, ok := common.ExtractFunctionName(n); ok && name != "" {
		return name
	}

	return ""
}

// calculateFunctionHalsteadMetrics calculates Halstead metrics for a single function.
func (h *HalsteadAnalyzer) calculateFunctionHalsteadMetrics(fn *node.Node) *FunctionHalsteadMetrics {
	operators := make(map[string]int)
	operands := make(map[string]int)

	h.detector.CollectOperatorsAndOperands(fn, operators, operands)

	metrics := &FunctionHalsteadMetrics{
		DistinctOperators: len(operators),
		DistinctOperands:  len(operands),
		TotalOperators:    h.metrics.SumMap(operators),
		TotalOperands:     h.metrics.SumMap(operands),
		Operators:         operators,
		Operands:          operands,
	}

	h.metrics.CalculateHalsteadMetrics(metrics)

	return metrics
}
