package complexity //nolint:testpackage // testing internal implementation.

import (
	"bytes"
	"encoding/json"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

func TestAnalyzer_Basic(t *testing.T) {
	t.Parallel()

	analyzer := NewAnalyzer()

	// Test basic functionality.
	if analyzer.Name() != "complexity" {
		t.Errorf("Expected name 'complexity', got '%s'", analyzer.Name())
	}

	thresholds := analyzer.Thresholds()
	if len(thresholds) != 3 {
		t.Errorf("Expected 3 thresholds, got %d", len(thresholds))
	}

	// Test that expected thresholds exist.
	expectedThresholds := []string{"cyclomatic_complexity", "cognitive_complexity", "nesting_depth"}
	for _, expected := range expectedThresholds {
		if _, exists := thresholds[expected]; !exists {
			t.Errorf("Expected threshold '%s' to exist", expected)
		}
	}
}

func TestAnalyzer_NilRoot(t *testing.T) {
	t.Parallel()

	analyzer := NewAnalyzer()

	result, err := analyzer.Analyze(nil)
	if err != nil {
		t.Errorf("Expected no error for nil root, got %v", err)
	}

	if result == nil {
		t.Error("Expected non-nil result for nil root")

		return
	}

	// Check that we get the expected empty result structure.
	if total, ok := result["total_functions"]; !ok {
		t.Error("Expected 'total_functions' in result")
	} else if total != 0 {
		t.Errorf("Expected total_functions to be 0 for nil root, got %v", total)
	}
}

func TestAnalyzer_SimpleFunction(t *testing.T) {
	t.Parallel()

	analyzer := NewAnalyzer()

	// Create a simple function.
	functionNode := &node.Node{Type: node.UASTFunction}
	functionNode.Roles = []node.Role{node.RoleFunction, node.RoleDeclaration}

	// Add function name.
	nameNode := node.NewNodeWithToken(node.UASTIdentifier, "simpleFunction")
	nameNode.Roles = []node.Role{node.RoleName}
	functionNode.AddChild(nameNode)

	// Create a root node with the function.
	root := &node.Node{Type: node.UASTFile}
	root.AddChild(functionNode)

	result, err := analyzer.Analyze(root)
	if err != nil {
		t.Errorf("Expected no error, got %v", err)
	}

	if result == nil {
		t.Error("Expected non-nil result")

		return
	}

	if total, ok := result["total_functions"]; !ok {
		t.Error("Expected 'total_functions' in result")
	} else if total != 1 {
		t.Errorf("Expected total_functions to be 1, got %v", total)
	}

	if complexity, ok := result["total_complexity"]; !ok {
		t.Error("Expected 'total_complexity' in result")
	} else if complexity != 1 {
		t.Errorf("Expected total_complexity to be 1 for simple function, got %v", complexity)
	}
}

func TestAnalyzer_ExtractFunctionName(t *testing.T) {
	t.Parallel()

	analyzer := NewAnalyzer()

	// Test function with name.
	functionNode := &node.Node{Type: node.UASTFunction}
	nameNode := node.NewNodeWithToken(node.UASTIdentifier, "testFunction")
	nameNode.Roles = []node.Role{node.RoleName}
	functionNode.AddChild(nameNode)

	name := analyzer.extractFunctionName(functionNode)
	if name != "testFunction" {
		t.Errorf("Expected function name 'testFunction', got '%s'", name)
	}

	// Test function without name.
	anonymousFunction := &node.Node{Type: node.UASTFunction}

	name = analyzer.extractFunctionName(anonymousFunction)
	if name != "anonymous" {
		t.Errorf("Expected anonymous function name 'anonymous', got '%s'", name)
	}
}

func TestAnalyzer_IsDecisionPoint(t *testing.T) {
	t.Parallel()

	analyzer := NewAnalyzer()

	// Test decision point types.
	decisionTypes := []string{
		node.UASTIf, node.UASTSwitch, node.UASTCase, node.UASTTry, node.UASTCatch,
		node.UASTThrow, node.UASTBreak, node.UASTContinue, node.UASTReturn,
	}

	for _, nodeType := range decisionTypes {
		testNode := &node.Node{Type: node.Type(nodeType)}
		testNode.Roles = []node.Role{node.RoleCondition}

		if !analyzer.isDecisionPoint(testNode) {
			t.Errorf("Expected node type '%s' to be a decision point", nodeType)
		}
	}

	// Test non-decision point type.
	regularNode := &node.Node{Type: node.UASTIdentifier}
	if analyzer.isDecisionPoint(regularNode) {
		t.Error("Expected identifier node to not be a decision point")
	}

	// Test logical operators.
	logicalOpNode := &node.Node{Type: node.UASTBinaryOp}

	logicalOpNode.Props = map[string]string{"operator": "&&"}
	if !analyzer.isDecisionPoint(logicalOpNode) {
		t.Error("Expected binary op with '&&' to be a decision point")
	}

	// Test non-logical operator.
	arithmeticOpNode := &node.Node{Type: node.UASTBinaryOp}

	arithmeticOpNode.Props = map[string]string{"operator": "+"}
	if analyzer.isDecisionPoint(arithmeticOpNode) {
		t.Error("Expected binary op with '+' to not be a decision point")
	}
}

func TestAnalyzer_WithIfStatement(t *testing.T) {
	t.Parallel()

	analyzer := NewAnalyzer()

	// Create a function with an if statement.
	functionNode := &node.Node{Type: node.UASTFunction}
	functionNode.Roles = []node.Role{node.RoleFunction, node.RoleDeclaration}

	// Add function name.
	nameNode := node.NewNodeWithToken(node.UASTIdentifier, "testFunction")
	nameNode.Roles = []node.Role{node.RoleName}
	functionNode.AddChild(nameNode)

	// Add if statement.
	ifNode := &node.Node{Type: node.UASTIf}
	ifNode.Roles = []node.Role{node.RoleCondition}
	functionNode.AddChild(ifNode)

	// Create a root node with the function.
	root := &node.Node{Type: node.UASTFile}
	root.AddChild(functionNode)

	result, err := analyzer.Analyze(root)
	if err != nil {
		t.Errorf("Expected no error, got %v", err)
	}

	if result == nil {
		t.Error("Expected non-nil result")

		return
	}

	if total, ok := result["total_complexity"]; !ok {
		t.Error("Expected 'total_complexity' in result")
	} else if total != 2 {
		t.Errorf("Expected total_complexity to be 2 for function with if, got %v", total)
	}
}

func TestCognitiveComplexityCalculator_NestedStructures(t *testing.T) {
	t.Parallel()

	calculator := NewCognitiveComplexityCalculator()

	// Create a function with nested if statements.
	functionNode := &node.Node{Type: node.UASTFunction}
	functionNode.Roles = []node.Role{node.RoleFunction}

	// First level if.
	ifNode1 := &node.Node{Type: node.UASTIf}
	ifNode1.Roles = []node.Role{node.RoleCondition}

	// Nested if inside first if.
	ifNode2 := &node.Node{Type: node.UASTIf}
	ifNode2.Roles = []node.Role{node.RoleCondition}
	ifNode1.AddChild(ifNode2)

	functionNode.AddChild(ifNode1)

	complexity := calculator.CalculateCognitiveComplexity(functionNode)

	// Nested structures should increase complexity.
	if complexity < 2 {
		t.Errorf("Expected complexity >= 2 for nested ifs, got %d", complexity)
	}
}

// --- FormatReportJSON/YAML Tests ---

func TestAnalyzer_FormatReportJSON(t *testing.T) {
	t.Parallel()

	analyzer := NewAnalyzer()

	report := analyze.Report{
		"total_functions":      3,
		"average_complexity":   5.5,
		"max_complexity":       15,
		"total_complexity":     55,
		"cognitive_complexity": 25,
		"nesting_depth":        3,
		"decision_points":      20,
		"message":              "Test message",
		"functions": []map[string]any{
			{
				"name":                  "testFunc",
				"cyclomatic_complexity": 10,
				"cognitive_complexity":  12,
				"nesting_depth":         3,
				"lines_of_code":         50,
			},
		},
	}

	var buf bytes.Buffer
	err := analyzer.FormatReportJSON(report, &buf)
	require.NoError(t, err)

	var result map[string]any
	err = json.Unmarshal(buf.Bytes(), &result)
	require.NoError(t, err)

	// Verify metrics structure
	assert.Contains(t, result, "function_complexity")
	assert.Contains(t, result, "distribution")
	assert.Contains(t, result, "high_risk_functions")
	assert.Contains(t, result, "aggregate")
}

func TestAnalyzer_FormatReportYAML(t *testing.T) {
	t.Parallel()

	analyzer := NewAnalyzer()

	report := analyze.Report{
		"total_functions":      3,
		"average_complexity":   5.5,
		"max_complexity":       15,
		"total_complexity":     55,
		"cognitive_complexity": 25,
		"nesting_depth":        3,
		"decision_points":      20,
		"message":              "Test message",
		"functions": []map[string]any{
			{
				"name":                  "testFunc",
				"cyclomatic_complexity": 10,
				"cognitive_complexity":  12,
				"nesting_depth":         3,
				"lines_of_code":         50,
			},
		},
	}

	var buf bytes.Buffer
	err := analyzer.FormatReportYAML(report, &buf)
	require.NoError(t, err)

	output := buf.String()
	assert.Contains(t, output, "function_complexity:")
	assert.Contains(t, output, "distribution:")
	assert.Contains(t, output, "aggregate:")
}
