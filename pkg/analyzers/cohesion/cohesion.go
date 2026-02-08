package cohesion

import (
	"encoding/json"
	"errors"
	"fmt"
	"io"

	"gopkg.in/yaml.v3"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/common"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/common/renderer"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/common/terminal"
	"github.com/Sumatoshi-tech/codefang/pkg/pipeline"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

const (
	cohesionThresholdHigh   = 0.8
	cohesionThresholdLow    = 0.3
	cohesionThresholdMedium = 0.6
	countThresholdHigh      = 3
	lineCountThresholdHigh  = 10
	magic0p2                = 0.2
	magic0p3                = 0.3
	magic0p5                = 0.5
	magic0p6                = 0.6
	magic0p7                = 0.7
	magic0p8                = 0.8
	magic2p0                = 2.0
	magic30                 = 30
	magic4p0                = 4.0
	magic7                  = 7
)

// Name returns the analyzer name.
func (c *Analyzer) Name() string {
	return "cohesion"
}

// Flag returns the CLI flag for the analyzer.
func (c *Analyzer) Flag() string {
	return "cohesion-analysis"
}

// Description returns the analyzer description.
func (c *Analyzer) Description() string {
	return "Calculates LCOM and cohesion metrics."
}

// ListConfigurationOptions returns the configuration options for the analyzer.
func (c *Analyzer) ListConfigurationOptions() []pipeline.ConfigurationOption {
	return []pipeline.ConfigurationOption{}
}

// Configure configures the analyzer.
func (c *Analyzer) Configure(_ map[string]any) error {
	return nil
}

// Thresholds returns the color-coded thresholds for cohesion metrics.
func (c *Analyzer) Thresholds() analyze.Thresholds {
	return analyze.Thresholds{
		"lcom": {
			"red":    magic4p0,
			"yellow": magic2p0,
			"green":  1.0,
		},
		"cohesion_score": {
			"red":    magic0p3,
			"yellow": magic0p6,
			"green":  magic0p8,
		},
		"function_cohesion": {
			"red":    magic0p2,
			"yellow": magic0p5,
			"green":  magic0p7,
		},
	}
}

// CreateVisitor creates a new visitor for cohesion analysis.
func (c *Analyzer) CreateVisitor() analyze.AnalysisVisitor {
	return NewVisitor()
}

// Analyze performs cohesion analysis on the UAST.
func (c *Analyzer) Analyze(root *node.Node) (analyze.Report, error) {
	if root == nil {
		return nil, errors.New("root node is nil") //nolint:err113 // simple guard, no sentinel needed
	}

	functions, err := c.findFunctions(root)
	if err != nil {
		return nil, err
	}

	if len(functions) == 0 {
		return c.buildEmptyResult(), nil
	}

	metrics := c.calculateMetrics(functions)
	result := c.buildResult(functions, metrics)

	return result, nil
}

// buildEmptyResult creates an empty result when no functions are found.
func (c *Analyzer) buildEmptyResult() analyze.Report {
	return common.NewResultBuilder().BuildCustomEmptyResult(map[string]any{
		"total_functions":   0,
		"lcom":              0.0,
		"cohesion_score":    1.0,
		"function_cohesion": 1.0,
		"message":           "No functions found",
	})
}

// calculateMetrics calculates all cohesion metrics for the functions.
func (c *Analyzer) calculateMetrics(functions []Function) map[string]float64 {
	lcom := c.calculateLCOM(functions)
	cohesionScore := c.calculateCohesionScore(lcom, len(functions))
	functionCohesion := c.calculateFunctionCohesion(functions)

	return map[string]float64{
		"lcom":              lcom,
		"cohesion_score":    cohesionScore,
		"function_cohesion": functionCohesion,
	}
}

// buildResult constructs the final analysis result.
func (c *Analyzer) buildResult(functions []Function, metrics map[string]float64) analyze.Report {
	detailedFunctionsTable := c.buildDetailedFunctionsTable(functions)
	message := c.getCohesionMessage(metrics["cohesion_score"])

	return analyze.Report{
		"analyzer_name":     "cohesion",
		"total_functions":   len(functions),
		"lcom":              metrics["lcom"],
		"cohesion_score":    metrics["cohesion_score"],
		"function_cohesion": metrics["function_cohesion"],
		"functions":         detailedFunctionsTable,
		"message":           message,
	}
}

// buildDetailedFunctionsTable creates the detailed functions table with assessments.
func (c *Analyzer) buildDetailedFunctionsTable(functions []Function) []map[string]any {
	table := make([]map[string]any, 0, len(functions))

	for _, fn := range functions {
		entry := map[string]any{
			"name":                fn.Name,
			"line_count":          fn.LineCount,
			"variable_count":      len(fn.Variables),
			"cohesion":            fn.Cohesion,
			"cohesion_assessment": c.getCohesionAssessment(fn.Cohesion),
			"variable_assessment": c.getVariableAssessment(len(fn.Variables)),
			"size_assessment":     c.getSizeAssessment(fn.LineCount),
		}
		table = append(table, entry)
	}

	return table
}

// FormatReport formats the analysis report for display.
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

// FormatReportJSON formats the analysis report as JSON.
func (c *Analyzer) FormatReportJSON(report analyze.Report, w io.Writer) error {
	metrics, err := ComputeAllMetrics(report)
	if err != nil {
		metrics = &ComputedMetrics{}
	}

	jsonData, err := json.MarshalIndent(metrics, "", "  ")
	if err != nil {
		return fmt.Errorf("formatreportjson: %w", err)
	}

	_, err = fmt.Fprint(w, string(jsonData))
	if err != nil {
		return fmt.Errorf("formatreportjson: %w", err)
	}

	return nil
}

// FormatReportYAML formats the analysis report as YAML.
func (c *Analyzer) FormatReportYAML(report analyze.Report, w io.Writer) error {
	metrics, err := ComputeAllMetrics(report)
	if err != nil {
		metrics = &ComputedMetrics{}
	}

	data, err := yaml.Marshal(metrics)
	if err != nil {
		return fmt.Errorf("formatreportyaml: %w", err)
	}

	_, err = w.Write(data)
	if err != nil {
		return fmt.Errorf("formatreportyaml: %w", err)
	}

	return nil
}

// getCohesionMessage returns a message based on the cohesion score.
func (c *Analyzer) getCohesionMessage(score float64) string {
	if score >= scoreThresholdHigh {
		return "Excellent cohesion - functions are well-focused and cohesive"
	}

	if score >= scoreThresholdMedium {
		return "Good cohesion - functions have reasonable focus"
	}

	if score >= scoreThresholdLow {
		return "Fair cohesion - some functions could be more focused"
	}

	return "Poor cohesion - functions lack focus and should be refactored"
}

// getCohesionAssessment returns an assessment with emoji for cohesion.
func (c *Analyzer) getCohesionAssessment(cohesion float64) string {
	if cohesion >= cohesionThresholdHigh {
		return "🟢 Excellent"
	}

	if cohesion >= cohesionThresholdMedium {
		return "🟡 Good"
	}

	if cohesion >= cohesionThresholdLow {
		return "🟡 Fair"
	}

	return "🔴 Poor"
}

// getVariableAssessment returns an assessment with emoji for variable count.
func (c *Analyzer) getVariableAssessment(count int) string {
	if count <= countThresholdHigh {
		return "🟢 Few"
	}

	if count <= magic7 {
		return "🟡 Moderate"
	}

	return "🔴 Many"
}

// getSizeAssessment returns an assessment with emoji for function size.
func (c *Analyzer) getSizeAssessment(lineCount int) string {
	if lineCount <= lineCountThresholdHigh {
		return "🟢 Small"
	}

	if lineCount <= magic30 {
		return "🟡 Medium"
	}

	return "🔴 Large"
}

// findFunctions finds all functions using the generic traverser.
//
//nolint:unparam // parameter is needed for interface compliance.
func (c *Analyzer) findFunctions(root *node.Node) ([]Function, error) {
	functionNodes := c.traverser.FindNodesByRoles(root, []string{"Function"})
	typeNodes := c.traverser.FindNodesByType(root, []string{"Function", "Method"})

	allNodes := c.deduplicateNodes(functionNodes, typeNodes)

	return c.extractFunctionsFromNodes(allNodes), nil
}

// deduplicateNodes combines and deduplicates function nodes.
func (c *Analyzer) deduplicateNodes(functionNodes, typeNodes []*node.Node) []*node.Node {
	nodeMap := make(map[*node.Node]bool)

	for _, node := range functionNodes {
		nodeMap[node] = true
	}

	for _, node := range typeNodes {
		nodeMap[node] = true
	}

	result := make([]*node.Node, 0, len(nodeMap))
	for node := range nodeMap {
		result = append(result, node)
	}

	return result
}

// extractFunctionsFromNodes extracts Function structs from UAST nodes.
func (c *Analyzer) extractFunctionsFromNodes(nodes []*node.Node) []Function {
	functions := make([]Function, 0, len(nodes))

	for _, node := range nodes {
		functions = append(functions, c.extractFunction(node))
	}

	return functions
}

// extractFunction extracts function data from a node.
func (c *Analyzer) extractFunction(n *node.Node) Function {
	variables := c.extractVariables(n)
	name := c.extractFunctionName(n)
	lineCount := c.traverser.CountLines(n)

	function := Function{
		Name:      name,
		LineCount: lineCount,
		Variables: variables,
		Cohesion:  0.0,
	}

	function.Cohesion = c.calculateFunctionLevelCohesion(function)

	return function
}

// extractFunctionName extracts the function name from a node.
func (c *Analyzer) extractFunctionName(n *node.Node) string {
	name, _ := c.extractor.ExtractName(n, "function_name")
	if name == "" {
		name, _ = common.ExtractFunctionName(n)
	}

	return name
}

// extractVariables extracts all variables from a function node.
func (c *Analyzer) extractVariables(n *node.Node) []string {
	var variables []string
	c.findVariables(n, &variables)

	return variables
}

// findVariables finds all variables in a function.
func (c *Analyzer) findVariables(n *node.Node, variables *[]string) {
	if n == nil {
		return
	}

	c.processVariableNode(n, variables)
	c.processChildren(n, variables)
}

// processVariableNode processes a single node for variable extraction.
func (c *Analyzer) processVariableNode(n *node.Node, variables *[]string) {
	if c.isVariableDeclaration(n) {
		c.addVariableIfValid(n, variables)
	}

	if c.isVariableIdentifier(n) {
		c.addVariableIfValid(n, variables)
	}
}

// isVariableDeclaration checks if a node represents a variable declaration.
func (c *Analyzer) isVariableDeclaration(n *node.Node) bool {
	return n.HasAnyType(node.UASTVariable, node.UASTParameter) &&
		n.HasAnyRole(node.RoleDeclaration)
}

// isVariableIdentifier checks if a node represents a variable identifier.
func (c *Analyzer) isVariableIdentifier(n *node.Node) bool {
	return n.HasAnyType(node.UASTIdentifier) &&
		n.HasAnyRole(node.RoleVariable, node.RoleName)
}

// addVariableIfValid adds a variable name to the list if it's valid.
func (c *Analyzer) addVariableIfValid(n *node.Node, variables *[]string) {
	if name, ok := c.extractor.ExtractName(n, "variable_name"); ok && name != "" {
		*variables = append(*variables, name)
	} else if varName, varOK := common.ExtractVariableName(n); varOK && varName != "" {
		*variables = append(*variables, varName)
	}
}

// processChildren recursively processes child nodes.
func (c *Analyzer) processChildren(n *node.Node, variables *[]string) {
	for _, child := range n.Children {
		c.findVariables(child, variables)
	}
}
