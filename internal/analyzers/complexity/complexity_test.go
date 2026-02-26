package complexity

import (
	"bytes"
	"context"
	"encoding/json"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/uast"
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

func TestAnalyzer_MetadataAndFormatting(t *testing.T) {
	t.Parallel()

	analyzer := NewAnalyzer()
	assert.Equal(t, "complexity-analysis", analyzer.Flag())
	assert.Equal(t, analyzer.Descriptor().Description, analyzer.Description())
	assert.Empty(t, analyzer.ListConfigurationOptions())
	require.NoError(t, analyzer.Configure(nil))
	assert.NotNil(t, analyzer.CreateAggregator())
	assert.NotNil(t, analyzer.CreateVisitor())

	report := analyze.Report{
		"total_functions":      1,
		"average_complexity":   2.0,
		"max_complexity":       2,
		"total_complexity":     2,
		"cognitive_complexity": 1,
		"nesting_depth":        1,
		"decision_points":      1,
		"message":              "ok",
		"functions": []map[string]any{
			{
				"name":                  "f",
				"cyclomatic_complexity": 2,
				"cognitive_complexity":  1,
				"nesting_depth":         1,
				"lines_of_code":         2,
			},
		},
	}

	var textOut bytes.Buffer
	require.NoError(t, analyzer.FormatReport(report, &textOut))
	assert.Contains(t, textOut.String(), "COMPLEXITY")

	var binaryOut bytes.Buffer
	require.NoError(t, analyzer.FormatReportBinary(report, &binaryOut))
	assert.NotEmpty(t, binaryOut.Bytes())
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
		node.UASTIf, node.UASTLoop, node.UASTCase, node.UASTCatch,
	}

	for _, nodeType := range decisionTypes {
		testNode := &node.Node{Type: node.Type(nodeType)}

		if !analyzer.isDecisionPoint(testNode) {
			t.Errorf("Expected node type '%s' to be a decision point", nodeType)
		}
	}

	// Default switch case should not increment cyclomatic complexity.
	defaultCase := &node.Node{Type: node.UASTCase, Token: "default:\n\treturn 0"}
	if analyzer.isDecisionPoint(defaultCase) {
		t.Error("Expected default case to not be a decision point")
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

func TestAnalyzer_GoldenParity_GoMethodologySample(t *testing.T) {
	t.Parallel()

	const source = `package sample

func Linear(x int) int {
	return x
}

func IfElse(flag bool) int {
	if flag {
		return 1
	}
	return 0
}

func LoopAndGuards(items []int) int {
	total := 0
	for _, v := range items {
		if v < 0 {
			continue
		}
		if v == 0 {
			break
		}
		total += v
	}
	return total
}

func SwitchBranches(x int) int {
	switch x {
	case 1:
		return 1
	case 2, 3:
		return 2
	default:
		return 0
	}
}

func BoolChain(a, b, c, d bool) bool {
	if a && b && c || d {
		return true
	}
	return false
}

func NestedIf(a, b, c bool) int {
	if a {
		if b {
			if c {
				return 1
			}
		}
	}
	return 0
}

func ElseIfChain(x int) int {
	if x < 0 {
		return -1
	} else if x == 0 {
		return 0
	} else if x == 1 {
		return 1
	}
	return 2
}
`

	parser, err := uast.NewParser()
	require.NoError(t, err)

	root, err := parser.Parse(context.Background(), "sample.go", []byte(source))
	require.NoError(t, err)

	analyzer := NewAnalyzer()
	report, err := analyzer.Analyze(root)
	require.NoError(t, err)

	functions, ok := report["functions"].([]map[string]any)
	require.True(t, ok)
	require.Len(t, functions, 7)

	type functionMetrics struct {
		cognitive  int
		cyclomatic int
	}

	got := make(map[string]functionMetrics, len(functions))
	for _, fn := range functions {
		name, nameOK := fn["name"].(string)
		cyclomatic, cyclomaticOK := fn["cyclomatic_complexity"].(int)
		cognitive, cognitiveOK := fn["cognitive_complexity"].(int)
		require.True(t, nameOK && cyclomaticOK && cognitiveOK)

		got[name] = functionMetrics{
			cyclomatic: cyclomatic,
			cognitive:  cognitive,
		}
	}

	// Golden references:
	// - Cyclomatic: gocyclo v0.6.0
	// - Cognitive: gocognit v1.2.1.
	expected := map[string]functionMetrics{
		"Linear":         {cyclomatic: 1, cognitive: 0},
		"IfElse":         {cyclomatic: 2, cognitive: 1},
		"LoopAndGuards":  {cyclomatic: 4, cognitive: 5},
		"SwitchBranches": {cyclomatic: 3, cognitive: 1},
		"BoolChain":      {cyclomatic: 5, cognitive: 3},
		"NestedIf":       {cyclomatic: 4, cognitive: 6},
		"ElseIfChain":    {cyclomatic: 4, cognitive: 3},
	}

	for name, want := range expected {
		gotMetrics, exists := got[name]
		require.True(t, exists, "missing function metrics for %s", name)
		assert.Equal(t, want.cyclomatic, gotMetrics.cyclomatic, "cyclomatic mismatch for %s", name)
		assert.Equal(t, want.cognitive, gotMetrics.cognitive, "cognitive mismatch for %s", name)
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

// --- FormatReportJSON/YAML Tests ---.

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

	// Verify metrics structure.
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
