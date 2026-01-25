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

// HalsteadAnalyzer provides Halstead complexity measures analysis
type HalsteadAnalyzer struct {
	// traverser handles UAST traversal and node finding
	traverser *common.UASTTraverser
	// extractor handles data extraction from UAST nodes
	extractor *common.DataExtractor
	// metrics handles Halstead metrics calculations
	metrics *MetricsCalculator
	// detector handles operator and operand detection
	detector *OperatorOperandDetector
	// formatter handles report formatting and output
	formatter *ReportFormatter
}

// NewHalsteadAnalyzer creates a new HalsteadAnalyzer with common modules
func NewHalsteadAnalyzer() *HalsteadAnalyzer {
	// Configure UAST traverser with advanced filtering
	traversalConfig := common.TraversalConfig{
		Filters: []common.NodeFilter{
			{
				Types:    []string{node.UASTFunction, node.UASTMethod},
				Roles:    []string{node.RoleFunction, node.RoleDeclaration},
				MinLines: 1,
			},
		},
		MaxDepth:    10,
		IncludeRoot: false,
	}

	// Configure data extractor with Halstead-specific extractors
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

// extractOperatorName extracts operator name from a node
func extractOperatorName(n *node.Node) (string, bool) {
	if n == nil {
		return "", false
	}
	return string(n.Type), true
}

// extractOperandName extracts operand name from a node
func extractOperandName(n *node.Node) (string, bool) {
	if n == nil {
		return "", false
	}

	// Try to extract from token first
	if n.Token != "" {
		return n.Token, true
	}

	// Try to extract from properties
	if n.Props != nil {
		if name, ok := n.Props["name"]; ok {
			return name, true
		}
	}

	// Fallback to node type
	return string(n.Type), true
}

// HalsteadMetrics holds all Halstead complexity measures
type HalsteadMetrics struct {
	// DistinctOperators is the number of unique operators (η1)
	DistinctOperators int `json:"distinct_operators"`
	// DistinctOperands is the number of unique operands (η2)
	DistinctOperands int `json:"distinct_operands"`
	// TotalOperators is the total count of all operators (N1)
	TotalOperators int `json:"total_operators"`
	// TotalOperands is the total count of all operands (N2)
	TotalOperands int `json:"total_operands"`
	// Vocabulary is the sum of distinct operators and operands (η = η1 + η2)
	Vocabulary int `json:"vocabulary"`
	// Length is the sum of total operators and operands (N = N1 + N2)
	Length int `json:"length"`
	// EstimatedLength is the estimated program length (N̂ = η1*log2(η1) + η2*log2(η2))
	EstimatedLength float64 `json:"estimated_length"`
	// Volume measures the information content of the program (V = N * log2(η))
	Volume float64 `json:"volume"`
	// Difficulty measures how hard the program is to understand (D = (η1/2) * (N2/η2))
	Difficulty float64 `json:"difficulty"`
	// Effort measures the mental effort required to understand the program (E = D * V)
	Effort float64 `json:"effort"`
	// TimeToProgram estimates the time to program in seconds (T = E/18)
	TimeToProgram float64 `json:"time_to_program"`
	// DeliveredBugs estimates the number of delivered bugs (B = V/3000)
	DeliveredBugs float64 `json:"delivered_bugs"`
	// Functions contains per-function Halstead metrics
	Functions map[string]*FunctionHalsteadMetrics `json:"functions"`
}

// FunctionHalsteadMetrics contains Halstead metrics for a single function
type FunctionHalsteadMetrics struct {
	// Name is the function name
	Name string `json:"name"`
	// DistinctOperators is the number of unique operators in this function
	DistinctOperators int `json:"distinct_operators"`
	// DistinctOperands is the number of unique operands in this function
	DistinctOperands int `json:"distinct_operands"`
	// TotalOperators is the total count of all operators in this function
	TotalOperators int `json:"total_operators"`
	// TotalOperands is the total count of all operands in this function
	TotalOperands int `json:"total_operands"`
	// Vocabulary is the sum of distinct operators and operands in this function
	Vocabulary int `json:"vocabulary"`
	// Length is the sum of total operators and operands in this function
	Length int `json:"length"`
	// EstimatedLength is the estimated program length for this function
	EstimatedLength float64 `json:"estimated_length"`
	// Volume measures the information content of this function
	Volume float64 `json:"volume"`
	// Difficulty measures how hard this function is to understand
	Difficulty float64 `json:"difficulty"`
	// Effort measures the mental effort required to understand this function
	Effort float64 `json:"effort"`
	// TimeToProgram estimates the time to program this function in seconds
	TimeToProgram float64 `json:"time_to_program"`
	// DeliveredBugs estimates the number of delivered bugs in this function
	DeliveredBugs float64 `json:"delivered_bugs"`
	// Operators maps operator names to their counts in this function
	Operators map[string]int `json:"operators"`
	// Operands maps operand names to their counts in this function
	Operands map[string]int `json:"operands"`
}

// HalsteadConfig holds configuration for Halstead analysis
type HalsteadConfig struct {
	// IncludeFunctionBreakdown determines whether to include per-function metrics
	IncludeFunctionBreakdown bool
	// IncludeTimeEstimate determines whether to calculate time to program estimates
	IncludeTimeEstimate bool
	// IncludeBugEstimate determines whether to calculate delivered bug estimates
	IncludeBugEstimate bool
}

// Name returns the analyzer name
func (h *HalsteadAnalyzer) Name() string {
	return "halstead"
}

// Flag returns the CLI flag for the analyzer
func (h *HalsteadAnalyzer) Flag() string {
	return "halstead-analysis"
}

// Description returns the analyzer description
func (h *HalsteadAnalyzer) Description() string {
	return "Calculates Halstead complexity metrics."
}

// ListConfigurationOptions returns the configuration options for the analyzer
func (h *HalsteadAnalyzer) ListConfigurationOptions() []pipeline.ConfigurationOption {
	return []pipeline.ConfigurationOption{}
}

// Configure configures the analyzer
func (h *HalsteadAnalyzer) Configure(facts map[string]interface{}) error {
	return nil
}

// Thresholds returns the color-coded thresholds for Halstead metrics
func (h *HalsteadAnalyzer) Thresholds() analyze.Thresholds {
	return analyze.Thresholds{
		"volume": {
			"green":  100,
			"yellow": 1000,
			"red":    5000,
		},
		"difficulty": {
			"green":  5,
			"yellow": 15,
			"red":    30,
		},
		"effort": {
			"green":  1000,
			"yellow": 10000,
			"red":    50000,
		},
	}
}

// CreateAggregator returns a new aggregator for Halstead analysis
func (h *HalsteadAnalyzer) CreateAggregator() analyze.ResultAggregator {
	return NewHalsteadAggregator()
}

// CreateVisitor creates a new visitor for Halstead analysis
func (h *HalsteadAnalyzer) CreateVisitor() analyze.AnalysisVisitor {
	return NewHalsteadVisitor()
}

// Analyze performs Halstead analysis on the UAST
func (h *HalsteadAnalyzer) Analyze(root *node.Node) (analyze.Report, error) {
	if root == nil {
		return nil, fmt.Errorf("root node is nil")
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

// FormatReport formats the analysis report for display
func (h *HalsteadAnalyzer) FormatReport(report analyze.Report, w io.Writer) error {
	section := NewHalsteadReportSection(report)
	config := terminal.NewConfig()
	r := renderer.NewSectionRenderer(config.Width, false, config.NoColor)
	_, err := fmt.Fprint(w, r.Render(section))
	return err
}

// FormatReportJSON formats the analysis report as JSON
func (h *HalsteadAnalyzer) FormatReportJSON(report analyze.Report, w io.Writer) error {
	return h.formatter.FormatReportJSON(report, w)
}

// buildEmptyResult creates an empty result for cases with no functions
func (h *HalsteadAnalyzer) buildEmptyResult(message string) analyze.Report {
	return common.NewResultBuilder().BuildCustomEmptyResult(map[string]interface{}{
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

// calculateAllFunctionMetrics calculates metrics for all functions
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

// getFunctionName extracts function name with fallback to anonymous for unnamed functions
func (h *HalsteadAnalyzer) getFunctionName(fn *node.Node) string {
	funcName := h.extractFunctionName(fn)
	if funcName == "" {
		return "anonymous"
	}
	return funcName
}

// calculateFileLevelMetrics calculates file-level metrics from function metrics
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

// aggregateOperatorsAndOperandsFromMetrics aggregates operators and operands from function metrics
func (h *HalsteadAnalyzer) aggregateOperatorsAndOperandsFromMetrics(fn *FunctionHalsteadMetrics, operators map[string]int, operands map[string]int) {
	for operator, count := range fn.Operators {
		operators[operator] += count
	}

	for operand, count := range fn.Operands {
		operands[operand] += count
	}
}

// buildDetailedFunctionsTable creates the detailed functions table for display
func (h *HalsteadAnalyzer) buildDetailedFunctionsTable(functionMetrics map[string]*FunctionHalsteadMetrics) []map[string]interface{} {
	detailedFunctionsTable := make([]map[string]interface{}, 0, len(functionMetrics))

	for _, fn := range functionMetrics {
		functionData := h.buildFunctionTableEntry(fn)
		detailedFunctionsTable = append(detailedFunctionsTable, functionData)
	}

	return detailedFunctionsTable
}

// buildFunctionTableEntry creates a single function table entry with metrics and assessments
func (h *HalsteadAnalyzer) buildFunctionTableEntry(fn *FunctionHalsteadMetrics) map[string]interface{} {
	return map[string]interface{}{
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

// buildFunctionDetails creates simplified function details for result
func (h *HalsteadAnalyzer) buildFunctionDetails(functionMetrics map[string]*FunctionHalsteadMetrics) []map[string]interface{} {
	functionDetails := make([]map[string]interface{}, 0, len(functionMetrics))

	for _, fn := range functionMetrics {
		functionData := h.buildFunctionDetailEntry(fn)
		functionDetails = append(functionDetails, functionData)
	}

	return functionDetails
}

// buildFunctionDetailEntry creates a single function detail entry with comprehensive metrics
func (h *HalsteadAnalyzer) buildFunctionDetailEntry(fn *FunctionHalsteadMetrics) map[string]interface{} {
	return map[string]interface{}{
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

// buildResult constructs the final analysis result
func (h *HalsteadAnalyzer) buildResult(fileMetrics *HalsteadMetrics, detailedFunctionsTable []map[string]interface{}, functionDetails []map[string]interface{}, message string) analyze.Report {
	metrics := map[string]interface{}{
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
		"total_functions":    len(functionDetails), // Add explicit total_functions for backward compatibility
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

// findFunctions finds all functions using the enhanced traverser
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

	// Convert back to slice
	functions := make([]*node.Node, 0, len(allNodes))
	for node := range allNodes {
		functions = append(functions, node)
	}

	return functions
}

// extractFunctionName extracts the function name using common extractor
func (h *HalsteadAnalyzer) extractFunctionName(n *node.Node) string {
	if name, ok := h.extractor.ExtractName(n, "function_name"); ok && name != "" {
		return name
	}
	if name, ok := common.ExtractFunctionName(n); ok && name != "" {
		return name
	}
	return ""
}

// calculateFunctionHalsteadMetrics calculates Halstead metrics for a single function
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
