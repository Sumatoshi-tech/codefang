package complexity

import (
	"fmt"
	"io"
	"sort"
	"strings"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/common"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/common/renderer"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/common/reportutil"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/common/terminal"
	"github.com/Sumatoshi-tech/codefang/pkg/pipeline"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

// Configuration constants for complexity analysis.
const (
	// MaxDepthValue is the default maximum UAST traversal depth for complexity analysis.
	MaxDepthValue = 10
	// MaxNestingDepthValue is the default maximum nesting depth tracked during complexity analysis.
	MaxNestingDepthValue       = 10
	avgComplexityThresholdHigh = 3.0
	complexityThresholdHigh    = 5
	depthThresholdHigh         = 3
	magic10                    = 10
	magic10_1                  = 10
	magic10_2                  = 10
	magic15                    = 15
	magic15_1                  = 15
	magic3                     = 3
	magic3_1                   = 3
	magic5                     = 5
	magic5_1                   = 5
	magic5_2                   = 5
	magic5_3                   = 5
	magic5_4                   = 5
	magic7                     = 7
	magic7_1                   = 7
)

// Analyzer provides comprehensive complexity analysis.
type Analyzer struct {
	traverser *common.UASTTraverser
	extractor *common.DataExtractor
}

// NewAnalyzer creates a new Analyzer.
func NewAnalyzer() *Analyzer {
	return &Analyzer{
		traverser: common.NewUASTTraverser(common.TraversalConfig{
			MaxDepth:    MaxDepthValue,
			IncludeRoot: true,
		}),
		extractor: common.NewDataExtractor(common.ExtractionConfig{
			DefaultExtractors: true,
		}),
	}
}

// Metrics holds different types of complexity measurements.
type Metrics struct {
	FunctionMetrics        map[string]FunctionMetrics `json:"function_metrics"`
	ComplexityDistribution map[string]int             `json:"complexity_distribution"`
	CyclomaticComplexity   int                        `json:"cyclomatic_complexity"`
	CognitiveComplexity    int                        `json:"cognitive_complexity"`
	NestingDepth           int                        `json:"nesting_depth"`
	DecisionPoints         int                        `json:"decision_points"`
	TotalFunctions         int                        `json:"total_functions"`
	AverageComplexity      float64                    `json:"average_complexity"`
	MaxComplexity          int                        `json:"max_complexity"`
}

// FunctionMetrics holds complexity metrics for individual functions.
type FunctionMetrics struct {
	Name                 string `json:"name"`
	CyclomaticComplexity int    `json:"cyclomatic_complexity"`
	CognitiveComplexity  int    `json:"cognitive_complexity"`
	NestingDepth         int    `json:"nesting_depth"`
	DecisionPoints       int    `json:"decision_points"`
	LinesOfCode          int    `json:"lines_of_code"`
	Parameters           int    `json:"parameters"`
	ReturnStatements     int    `json:"return_statements"`
}

// Config holds configuration for complexity analysis.
type Config struct {
	ComplexityThresholds       map[string]int
	MaxNestingDepth            int
	IncludeCognitiveComplexity bool
	IncludeNestingDepth        bool
	IncludeDecisionPoints      bool
	IncludeLOCMetrics          bool
}

// Name returns the analyzer name.
func (c *Analyzer) Name() string {
	return "complexity"
}

// Flag returns the CLI flag for the analyzer.
func (c *Analyzer) Flag() string {
	return "complexity-analysis"
}

// Description returns the analyzer description.
func (c *Analyzer) Description() string {
	return c.Descriptor().Description
}

// Descriptor returns stable analyzer metadata.
func (c *Analyzer) Descriptor() analyze.Descriptor {
	return analyze.NewDescriptor(
		analyze.ModeStatic,
		c.Name(),
		"Analyzes code complexity including cyclomatic and cognitive complexity.",
	)
}

// ListConfigurationOptions returns the configuration options for the analyzer.
func (c *Analyzer) ListConfigurationOptions() []pipeline.ConfigurationOption {
	return []pipeline.ConfigurationOption{}
}

// Configure configures the analyzer.
func (c *Analyzer) Configure(_ map[string]any) error {
	return nil
}

// Thresholds returns the color-coded thresholds for complexity metrics.
func (c *Analyzer) Thresholds() analyze.Thresholds {
	return analyze.Thresholds{
		"cyclomatic_complexity": {
			"green":  1,
			"yellow": magic5,
			"red":    magic10,
		},
		"cognitive_complexity": {
			"green":  1,
			"yellow": magic7,
			"red":    magic15,
		},
		"nesting_depth": {
			"green":  1,
			"yellow": magic3,
			"red":    magic5_1,
		},
	}
}

// CreateAggregator returns a new aggregator for complexity analysis.
func (c *Analyzer) CreateAggregator() analyze.ResultAggregator {
	return NewAggregator()
}

// FormatReport formats complexity analysis results as human-readable text.
func (c *Analyzer) FormatReport(report analyze.Report, w io.Writer) error {
	section := NewReportSection(report)
	config := terminal.NewConfig()
	r := renderer.NewSectionRenderer(config.Width, false, config.NoColor)

	_, err := fmt.Fprint(w, r.Render(section))
	if err != nil {
		return fmt.Errorf("formatreport: %w", err)
	}

	return nil
}

// FormatReportJSON formats complexity analysis results as JSON.
func (c *Analyzer) FormatReportJSON(report analyze.Report, w io.Writer) error {
	metrics, err := ComputeAllMetrics(report)
	if err != nil {
		metrics = &ComputedMetrics{}
	}

	data, err := renderer.RenderMetricsJSON(metrics)
	if err != nil {
		return fmt.Errorf("formatreportjson: %w", err)
	}

	_, err = w.Write(data)
	if err != nil {
		return fmt.Errorf("formatreportjson: %w", err)
	}

	return nil
}

// FormatReportYAML formats complexity analysis results as YAML.
func (c *Analyzer) FormatReportYAML(report analyze.Report, w io.Writer) error {
	metrics, err := ComputeAllMetrics(report)
	if err != nil {
		metrics = &ComputedMetrics{}
	}

	data, err := renderer.RenderMetricsYAML(metrics)
	if err != nil {
		return fmt.Errorf("formatreportyaml: %w", err)
	}

	_, err = w.Write(data)
	if err != nil {
		return fmt.Errorf("formatreportyaml: %w", err)
	}

	return nil
}

// FormatReportBinary formats complexity analysis results as binary envelope.
func (c *Analyzer) FormatReportBinary(report analyze.Report, w io.Writer) error {
	metrics, err := ComputeAllMetrics(report)
	if err != nil {
		metrics = &ComputedMetrics{}
	}

	err = reportutil.EncodeBinaryEnvelope(metrics, w)
	if err != nil {
		return fmt.Errorf("formatreportbinary: %w", err)
	}

	return nil
}

// DefaultConfig returns default complexity analysis configuration.
func (c *Analyzer) DefaultConfig() Config {
	return Config{
		IncludeCognitiveComplexity: true,
		IncludeNestingDepth:        true,
		IncludeDecisionPoints:      true,
		IncludeLOCMetrics:          true,
		MaxNestingDepth:            MaxNestingDepthValue,
		ComplexityThresholds: map[string]int{
			"cyclomatic_green":  1,
			"cyclomatic_yellow": magic5_2,
			"cyclomatic_red":    magic10_1,
			"cognitive_green":   1,
			"cognitive_yellow":  magic7_1,
			"cognitive_red":     magic15_1,
			"nesting_green":     1,
			"nesting_yellow":    magic3_1,
			"nesting_red":       magic5_3,
		},
	}
}

// CreateVisitor creates a new visitor for complexity analysis.
func (c *Analyzer) CreateVisitor() analyze.AnalysisVisitor {
	return NewVisitor()
}

// Analyze performs complexity analysis on the given UAST.
func (c *Analyzer) Analyze(root *node.Node) (analyze.Report, error) {
	if root == nil {
		return c.buildEmptyResult("No AST provided"), nil
	}

	config := c.DefaultConfig()
	functions := c.findFunctions(root)

	if len(functions) == 0 {
		return c.buildEmptyResult("No functions found"), nil
	}

	functionMetrics, totals := c.calculateAllFunctionMetrics(functions, config)
	detailedFunctionsTable := c.buildDetailedFunctionsTable(functionMetrics, config)
	avgComplexity := c.calculateAverageComplexity(totals, len(functions))
	message := c.getComplexityMessage(avgComplexity)

	return c.buildResult(len(functions), avgComplexity, totals, detailedFunctionsTable, message), nil
}

// buildEmptyResult creates an empty result for cases with no functions.
func (c *Analyzer) buildEmptyResult(message string) analyze.Report {
	return common.NewResultBuilder().BuildCustomEmptyResult(map[string]any{
		"total_functions":    0,
		"average_complexity": 0.0,
		"max_complexity":     0,
		"total_complexity":   0,
		"message":            message,
	})
}

// buildDetailedFunctionsTable creates the detailed functions table for display.
func (c *Analyzer) buildDetailedFunctionsTable(
	functionMetrics []FunctionMetrics,
	config Config,
) []map[string]any {
	detailedFunctionsTable := make([]map[string]any, 0, len(functionMetrics))

	for _, metrics := range functionMetrics {
		complexityAssessment := c.getComplexityAssessment(metrics.CyclomaticComplexity, config.ComplexityThresholds)
		cognitiveAssessment := c.getCognitiveAssessment(metrics.CognitiveComplexity)
		nestingAssessment := c.getNestingAssessment(metrics.NestingDepth)

		detailedFunctionsTable = append(detailedFunctionsTable, map[string]any{
			"name":                  metrics.Name,
			"cyclomatic_complexity": metrics.CyclomaticComplexity,
			"cognitive_complexity":  metrics.CognitiveComplexity,
			"nesting_depth":         metrics.NestingDepth,
			"lines_of_code":         metrics.LinesOfCode,
			"complexity_assessment": complexityAssessment,
			"cognitive_assessment":  cognitiveAssessment,
			"nesting_assessment":    nestingAssessment,
		})
	}

	return detailedFunctionsTable
}

// calculateAverageComplexity calculates the average complexity across all functions.
func (c *Analyzer) calculateAverageComplexity(totals map[string]int, functionCount int) float64 {
	if functionCount == 0 {
		return 0.0
	}

	return float64(totals["cyclomatic"]) / float64(functionCount)
}

// buildResult constructs the final analysis result.
func (c *Analyzer) buildResult(
	functionCount int,
	avgComplexity float64,
	totals map[string]int,
	detailedFunctionsTable []map[string]any,
	message string,
) analyze.Report {
	return analyze.Report{
		"analyzer_name":        "complexity",
		"total_functions":      functionCount,
		"average_complexity":   avgComplexity,
		"max_complexity":       totals["max"],
		"total_complexity":     totals["cyclomatic"],
		"cognitive_complexity": totals["cognitive"],
		"nesting_depth":        totals["nesting"],
		"decision_points":      totals["decisions"],
		"functions":            detailedFunctionsTable,
		"message":              message,
	}
}

// findFunctions finds all functions in the UAST using common traverser.
func (c *Analyzer) findFunctions(root *node.Node) []*node.Node {
	functionNodes := c.traverser.FindNodesByType(root, []string{node.UASTFunction, node.UASTMethod})
	roleNodes := c.traverser.FindNodesByRoles(root, []string{node.RoleFunction})

	allNodes := make(map[*node.Node]bool)
	for _, node := range functionNodes {
		allNodes[node] = true
	}

	for _, node := range roleNodes {
		allNodes[node] = true
	}

	var functions []*node.Node

	for node := range allNodes {
		if c.isFunctionNode(node) {
			functions = append(functions, node)
		}
	}

	return functions
}

// isFunctionNode checks if a node represents a function.
func (c *Analyzer) isFunctionNode(n *node.Node) bool {
	if n == nil {
		return false
	}

	return n.HasAnyType(node.UASTFunction, node.UASTMethod) ||
		n.HasAllRoles(node.RoleFunction, node.RoleDeclaration)
}

// calculateAllFunctionMetrics calculates metrics for all functions.
func (c *Analyzer) calculateAllFunctionMetrics(
	functions []*node.Node, config Config,
) (functionMetrics []FunctionMetrics, totals map[string]int) {
	functionMetrics = make([]FunctionMetrics, 0, len(functions))
	totals = c.initializeTotals()
	complexityDistribution := c.initializeComplexityDistribution()

	for _, fn := range functions {
		metrics := c.calculateFunctionMetrics(fn)
		functionMetrics = append(functionMetrics, metrics)

		c.updateTotals(totals, metrics)
		c.updateComplexityDistribution(complexityDistribution, metrics, config)
	}

	sort.Slice(functionMetrics, func(i, j int) bool {
		left, right := functionMetrics[i], functionMetrics[j]

		if left.CyclomaticComplexity != right.CyclomaticComplexity {
			return left.CyclomaticComplexity > right.CyclomaticComplexity
		}

		if left.CognitiveComplexity != right.CognitiveComplexity {
			return left.CognitiveComplexity > right.CognitiveComplexity
		}

		return left.Name < right.Name
	})

	c.addDistributionToTotals(totals, complexityDistribution)

	return functionMetrics, totals
}

// initializeTotals creates a new totals map with default values.
func (c *Analyzer) initializeTotals() map[string]int {
	return map[string]int{
		"cyclomatic": 0,
		"cognitive":  0,
		"nesting":    0,
		"decisions":  0,
		"max":        0,
	}
}

// initializeComplexityDistribution creates a new complexity distribution map.
func (c *Analyzer) initializeComplexityDistribution() map[string]int {
	return map[string]int{
		"green":  0,
		"yellow": 0,
		"red":    0,
	}
}

// updateTotals updates the totals with metrics from a function.
func (c *Analyzer) updateTotals(totals map[string]int, metrics FunctionMetrics) {
	totals["cyclomatic"] += metrics.CyclomaticComplexity
	totals["cognitive"] += metrics.CognitiveComplexity
	totals["nesting"] += metrics.NestingDepth
	totals["decisions"] += metrics.DecisionPoints

	if metrics.CyclomaticComplexity > totals["max"] {
		totals["max"] = metrics.CyclomaticComplexity
	}
}

// updateComplexityDistribution updates the complexity distribution.
func (c *Analyzer) updateComplexityDistribution(distribution map[string]int, metrics FunctionMetrics, config Config) {
	complexityLevel := c.getComplexityLevel(metrics.CyclomaticComplexity, config.ComplexityThresholds)
	distribution[complexityLevel]++
}

// addDistributionToTotals adds distribution counts to totals.
func (c *Analyzer) addDistributionToTotals(totals, distribution map[string]int) {
	totals["distribution_green"] = distribution["green"]
	totals["distribution_yellow"] = distribution["yellow"]
	totals["distribution_red"] = distribution["red"]
}

// calculateFunctionMetrics calculates metrics for a single function.
func (c *Analyzer) calculateFunctionMetrics(fn *node.Node) FunctionMetrics {
	name := c.extractFunctionName(fn)
	cyclomatic := c.calculateCyclomaticComplexity(fn)

	return FunctionMetrics{
		Name:                 name,
		CyclomaticComplexity: cyclomatic,
		CognitiveComplexity:  c.calculateCognitiveComplexity(fn),
		NestingDepth:         c.calculateNestingDepth(fn),
		DecisionPoints:       max(cyclomatic-1, 0),
		LinesOfCode:          c.estimateLinesOfCode(fn),
		Parameters:           c.countParameters(fn),
		ReturnStatements:     c.countReturnStatements(fn),
	}
}

// calculateCyclomaticComplexity calculates cyclomatic complexity for a function.
func (c *Analyzer) calculateCyclomaticComplexity(fn *node.Node) int {
	complexity := 1 // Base complexity.
	sourceCtx := newFunctionSourceContext(fn)

	fn.VisitPreOrder(func(n *node.Node) {
		if n == fn {
			return
		}

		if c.isDecisionPointWithSource(n, sourceCtx) {
			complexity++
		}
	})

	return complexity
}

// calculateCognitiveComplexity calculates cognitive complexity for a function.
func (c *Analyzer) calculateCognitiveComplexity(fn *node.Node) int {
	calculator := NewCognitiveComplexityCalculator()

	return calculator.CalculateCognitiveComplexity(fn)
}

// calculateNestingDepth calculates the maximum nesting depth for a function.
func (c *Analyzer) calculateNestingDepth(fn *node.Node) int {
	maxDepth := 0

	var walk func(curr *node.Node, depth int, parent *node.Node, childIdx int)

	walk = func(curr *node.Node, depth int, parent *node.Node, childIdx int) {
		if curr == nil {
			return
		}

		currentDepth := depth
		if c.isNestingNode(curr) && !isElseIfNode(parent, curr, childIdx) {
			currentDepth++
			if currentDepth > maxDepth {
				maxDepth = currentDepth
			}
		}

		for idx, child := range curr.Children {
			walk(child, currentDepth, curr, idx)
		}
	}

	for idx, child := range fn.Children {
		walk(child, 0, fn, idx)
	}

	return maxDepth
}

// estimateLinesOfCode estimates the lines of code for a function.
func (c *Analyzer) estimateLinesOfCode(fn *node.Node) int {
	if fn.Pos != nil && fn.Pos.EndLine >= fn.Pos.StartLine {
		return int(fn.Pos.EndLine-fn.Pos.StartLine) + 1
	}

	loc := 0

	fn.VisitPreOrder(func(n *node.Node) {
		if n.Token != "" {
			lines := strings.Count(n.Token, "\n") + 1
			loc += lines
		}
	})

	return loc
}

// countParameters counts the number of parameters in a function.
func (c *Analyzer) countParameters(fn *node.Node) int {
	paramNodes := c.traverser.FindNodesByRoles(fn, []string{node.RoleArgument, node.RoleParameter})

	return len(paramNodes)
}

// countReturnStatements counts the number of return statements in a function.
func (c *Analyzer) countReturnStatements(fn *node.Node) int {
	returnNodes := c.traverser.FindNodesByType(fn, []string{node.UASTReturn})
	returnStmts := c.traverser.FindNodesByRoles(fn, []string{node.RoleReturn})

	return len(returnNodes) + len(returnStmts)
}

func (c *Analyzer) isDecisionPointWithSource(target *node.Node, sourceCtx functionSourceContext) bool {
	if target == nil {
		return false
	}

	switch target.Type {
	case node.UASTIf, node.UASTLoop, node.UASTCatch:
		return true
	case node.UASTCase:
		if !isDefaultCase(target) {
			return true
		}
	case node.UASTBinaryOp:
		if sourceCtx.binaryOperator(target) == "" {
			return false
		}

		return isLogicalOperatorToken(sourceCtx.binaryOperator(target))
	}

	return false
}

func (c *Analyzer) isNestingNode(target *node.Node) bool {
	if target == nil {
		return false
	}

	switch target.Type {
	case node.UASTIf, node.UASTLoop, node.UASTSwitch, node.UASTTry, node.UASTCatch:
		return true
	default:
		return false
	}
}

// extractFunctionName extracts the name of a function.
func (c *Analyzer) extractFunctionName(fn *node.Node) string {
	if name, ok := common.ExtractEntityName(fn); ok && name != "" {
		return name
	}

	if name, ok := c.extractor.ExtractName(fn, "function_name"); ok && name != "" {
		return name
	}

	name := c.extractNameFromProps(fn)
	if name != "" {
		return name
	}

	if fn.Type == node.UASTMethod {
		name = c.extractMethodFullName(fn)
		if name != "" {
			return name
		}
	}

	nameNodes := c.traverser.FindNodesByRoles(fn, []string{node.RoleName})
	if len(nameNodes) > 0 {
		if tokenName, tokenOK := c.extractor.ExtractNameFromToken(nameNodes[0]); tokenOK && tokenName != "" {
			return tokenName
		}
	}

	return anonymousFunctionName
}

// extractNameFromProps extracts name from node properties.
func (c *Analyzer) extractNameFromProps(fn *node.Node) string {
	props := []string{"name", "function_name", "method_name"}
	for _, prop := range props {
		if name, ok := fn.Props[prop]; ok && name != "" {
			return strings.TrimSpace(name)
		}
	}

	return ""
}

// extractMethodFullName extracts the full name of a method including class.
func (c *Analyzer) extractMethodFullName(fn *node.Node) string {
	className := c.extractClassName(fn)

	methodName := c.extractMethodName(fn)
	if className != "" && methodName != "" {
		return className + "." + methodName
	}

	if methodName != "" {
		return methodName
	}

	return ""
}

// extractClassName extracts the class name for a method.
func (c *Analyzer) extractClassName(fn *node.Node) string {
	if className, ok := fn.Props["class_name"]; ok && className != "" {
		return strings.TrimSpace(className)
	}

	classNodes := c.traverser.FindNodesByType(fn, []string{node.UASTClass})
	if len(classNodes) > 0 {
		if name, ok := common.ExtractEntityName(classNodes[0]); ok && name != "" {
			return name
		}
	}

	return c.findClassNameInAncestors(fn)
}

// findClassNameInAncestors finds class name in ancestor nodes.
func (c *Analyzer) findClassNameInAncestors(fn *node.Node) string {
	ancestors := fn.Ancestors(fn)
	for _, ancestor := range ancestors {
		if ancestor.Type == node.UASTClass {
			return c.extractFunctionName(ancestor)
		}
	}

	return ""
}

// extractMethodName extracts the method name.
func (c *Analyzer) extractMethodName(fn *node.Node) string {
	if name, ok := common.ExtractEntityName(fn); ok && name != "" {
		return name
	}

	name := c.extractNameFromProps(fn)
	if name != "" {
		return name
	}

	nameNodes := c.traverser.FindNodesByRoles(fn, []string{node.RoleName})
	if len(nameNodes) > 0 {
		if tokenName, tokenOK := c.extractor.ExtractNameFromToken(nameNodes[0]); tokenOK && tokenName != "" {
			return tokenName
		}
	}

	return c.findMethodNameInChildren(fn)
}

// findMethodNameInChildren finds method name in child nodes.
func (c *Analyzer) findMethodNameInChildren(fn *node.Node) string {
	for _, child := range fn.Children {
		if child.HasAnyRole(node.RoleName) {
			return strings.TrimSpace(child.Token)
		}
	}

	return ""
}

// getComplexityLevel determines the complexity level based on thresholds.
func (c *Analyzer) getComplexityLevel(complexity int, thresholds map[string]int) string {
	if complexity <= thresholds["cyclomatic_green"] {
		return "green"
	}

	if complexity <= thresholds["cyclomatic_yellow"] {
		return "yellow"
	}

	return "red"
}

// getComplexityMessage returns a message based on the average complexity score.
func (c *Analyzer) getComplexityMessage(avgComplexity float64) string {
	if avgComplexity <= 1.0 {
		return "Excellent complexity - functions are simple and maintainable"
	}

	if avgComplexity <= avgComplexityThresholdHigh {
		return msgGoodComplexity
	}

	if avgComplexity <= magic7p0 {
		return "Fair complexity - some functions could be simplified"
	}

	return "High complexity - functions are complex and should be refactored"
}

// getComplexityAssessment returns an assessment with emoji for cyclomatic complexity.
func (c *Analyzer) getComplexityAssessment(complexity int, thresholds map[string]int) string {
	level := c.getComplexityLevel(complexity, thresholds)
	switch level {
	case "green":
		return "🟢 Simple"
	case "yellow":
		return "🟡 Moderate"
	case "red":
		return "🔴 Complex"
	default:
		return "⚪ Unknown"
	}
}

// getCognitiveAssessment returns an assessment with emoji for cognitive complexity.
func (c *Analyzer) getCognitiveAssessment(complexity int) string {
	if complexity <= complexityThresholdHigh {
		return "🟢 Low"
	}

	if complexity <= magic10_2 {
		return "🟡 Medium"
	}

	return "🔴 High"
}

// getNestingAssessment returns an assessment with emoji for nesting depth.
func (c *Analyzer) getNestingAssessment(depth int) string {
	if depth <= depthThresholdHigh {
		return "🟢 Shallow"
	}

	if depth <= magic5_4 {
		return "🟡 Moderate"
	}

	return "🔴 Deep"
}
