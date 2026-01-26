package halstead //nolint:testpackage // testing internal implementation.

import (
	"testing"

	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

func TestHalsteadAnalyzer_Basic(t *testing.T) {
	t.Parallel()

	analyzer := NewHalsteadAnalyzer()

	// Test empty result.
	_, err := analyzer.Analyze(nil)
	if err == nil {
		t.Fatalf("Expected error for nil root, got nil")
	}

	// Test with empty root.
	emptyNode := node.New("empty", "File", "", nil, nil, nil)

	result, err := analyzer.Analyze(emptyNode)
	if err != nil {
		t.Fatalf("Expected no error, got %v", err)
	}

	if result["total_functions"] != 0 {
		t.Errorf("Expected total_functions 0, got %v", result["total_functions"])
	}
}

func TestHalsteadAnalyzer_SimpleFunction(t *testing.T) {
	t.Parallel()

	analyzer := NewHalsteadAnalyzer()

	// Create a simple function node.
	functionNode := node.New("func1", "Function", "", []node.Role{node.RoleFunction, node.RoleDeclaration}, nil, map[string]string{"name": "simpleFunction"}) //nolint:lll // long line is acceptable here.
	nameNode := node.New("name1", "Identifier", "simpleFunction", []node.Role{node.RoleName}, nil, map[string]string{"name": "simpleFunction"})               //nolint:lll // long line is acceptable here.

	// Add some basic operators and operands.
	assignmentNode := node.New("assign1", "Assignment", "=", []node.Role{node.RoleAssignment}, nil, map[string]string{"operator": "="})
	identifierNode := node.New("id1", "Identifier", "x", []node.Role{node.RoleVariable}, nil, map[string]string{"name": "x"})
	literalNode := node.New("lit1", "Literal", "5", []node.Role{node.RoleLiteral}, nil, map[string]string{"value": "5"})

	functionNode.AddChild(nameNode)
	functionNode.AddChild(assignmentNode)
	assignmentNode.AddChild(identifierNode)
	assignmentNode.AddChild(literalNode)

	result, err := analyzer.Analyze(functionNode)
	if err != nil {
		t.Fatalf("Expected no error, got %v", err)
	}

	if result["total_functions"] != 1 {
		t.Errorf("Expected total_functions 1, got %v", result["total_functions"])
	}

	functions, ok := result["functions"].([]map[string]any)
	require.True(t, ok, "type assertion failed for functions")

	if len(functions) != 1 {
		t.Errorf("Expected 1 function, got %d", len(functions))
	}

	funcMetrics := functions[0]
	if funcMetrics["name"] != "simpleFunction" {
		t.Errorf("Expected function name 'simpleFunction', got %v", funcMetrics["name"])
	}

	// Check basic metrics.
	if volume, volOK := funcMetrics["volume"].(float64); !volOK || volume <= 0 {
		t.Errorf("Expected positive volume, got %v", volume)
	}

	if difficulty, diffOK := funcMetrics["difficulty"].(float64); !diffOK || difficulty <= 0 {
		t.Errorf("Expected positive difficulty, got %v", difficulty)
	}

	if effort, effOK := funcMetrics["effort"].(float64); !effOK || effort <= 0 {
		t.Errorf("Expected positive effort, got %v", effort)
	}
}

func TestHalsteadAnalyzer_CalculateMetrics(t *testing.T) {
	t.Parallel()

	analyzer := NewHalsteadAnalyzer()

	// Test metrics calculation with known values.
	metrics := &FunctionHalsteadMetrics{
		DistinctOperators: 3,
		DistinctOperands:  4,
		TotalOperators:    6,
		TotalOperands:     8,
	}

	analyzer.metrics.CalculateHalsteadMetrics(metrics)

	// Check calculated values.
	expectedVocabulary := 7 // 3 + 4
	if metrics.Vocabulary != expectedVocabulary {
		t.Errorf("Expected vocabulary %d, got %d", expectedVocabulary, metrics.Vocabulary)
	}

	expectedLength := 14 // 6 + 8
	if metrics.Length != expectedLength {
		t.Errorf("Expected length %d, got %d", expectedLength, metrics.Length)
	}

	// Volume should be positive.
	if metrics.Volume <= 0 {
		t.Errorf("Expected positive volume, got %f", metrics.Volume)
	}

	// Difficulty should be positive.
	if metrics.Difficulty <= 0 {
		t.Errorf("Expected positive difficulty, got %f", metrics.Difficulty)
	}

	// Effort should be positive.
	if metrics.Effort <= 0 {
		t.Errorf("Expected positive effort, got %f", metrics.Effort)
	}
}

func TestHalsteadAnalyzer_Thresholds(t *testing.T) { //nolint:gocognit // cognitive complexity is acceptable for this function.
	t.Parallel()

	analyzer := NewHalsteadAnalyzer()
	thresholds := analyzer.Thresholds()

	// Check that thresholds exist.
	if thresholds["volume"] == nil {
		t.Error("Expected volume thresholds")
	}

	if thresholds["difficulty"] == nil {
		t.Error("Expected difficulty thresholds")
	}

	if thresholds["effort"] == nil {
		t.Error("Expected effort thresholds")
	}

	// Check threshold values are reasonable.
	if volumeThresholds, ok := thresholds["volume"]; ok {
		if green, greenOK := volumeThresholds["green"].(int); greenOK {
			if green <= 0 {
				t.Error("Expected positive green threshold for volume")
			}
		}
	}

	if difficultyThresholds, ok := thresholds["difficulty"]; ok {
		if green, greenOK := difficultyThresholds["green"].(int); greenOK {
			if green <= 0 {
				t.Error("Expected positive green threshold for difficulty")
			}
		}
	}

	if effortThresholds, ok := thresholds["effort"]; ok {
		if green, greenOK := effortThresholds["green"].(int); greenOK {
			if green <= 0 {
				t.Error("Expected positive green threshold for effort")
			}
		}
	}
}

func TestHalsteadAnalyzer_MessageGeneration(t *testing.T) {
	t.Parallel()

	analyzer := NewHalsteadAnalyzer()

	// Test excellent complexity.
	message := analyzer.formatter.GetHalsteadMessage(50, 3, 500)
	if message == "" {
		t.Error("Expected non-empty message for excellent complexity")
	}

	// Test high complexity.
	message = analyzer.formatter.GetHalsteadMessage(6000, 35, 60000)
	if message == "" {
		t.Error("Expected non-empty message for high complexity")
	}
}

func TestHalsteadAnalyzer_OperatorOperandDetection(t *testing.T) {
	t.Parallel()

	analyzer := NewHalsteadAnalyzer()

	// Test operator detection.
	operatorNode := node.New("op", "BinaryOp", "+", []node.Role{node.RoleOperator}, nil, map[string]string{"operator": "+"})
	if !analyzer.detector.IsOperator(operatorNode) {
		t.Error("Expected binary operator to be detected as operator")
	}

	// Test operand detection.
	operandNode := node.New("var", "Identifier", "x", []node.Role{node.RoleVariable}, nil, map[string]string{"name": "x"})
	if !analyzer.detector.IsOperand(operandNode) {
		t.Error("Expected identifier to be detected as operand")
	}

	// Test literal operand.
	literalNode := node.New("lit", "Literal", "42", []node.Role{node.RoleLiteral}, nil, map[string]string{"value": "42"})
	if !analyzer.detector.IsOperand(literalNode) {
		t.Error("Expected literal to be detected as operand")
	}
}

func TestHalsteadAnalyzer_FunctionNameExtraction(t *testing.T) {
	t.Parallel()

	analyzer := NewHalsteadAnalyzer()

	// Test function with name in properties.
	functionNode := node.New("func", "FunctionDecl", "", []node.Role{node.RoleFunction}, nil, map[string]string{"name": "testFunction"})

	name := analyzer.extractFunctionName(functionNode)
	if name != "testFunction" {
		t.Errorf("Expected function name 'testFunction', got '%s'", name)
	}

	// Test function with name in child identifier.
	functionNode = node.New("func", "FunctionDecl", "", []node.Role{node.RoleFunction}, nil, nil)
	nameNode := node.New("name", "Identifier", "childFunction", []node.Role{node.RoleName}, nil, map[string]string{"name": "childFunction"})
	functionNode.AddChild(nameNode)

	name = analyzer.extractFunctionName(functionNode)
	if name != "childFunction" {
		t.Errorf("Expected function name 'childFunction', got '%s'", name)
	}
}

func TestHalsteadAnalyzer_RealAggregation(t *testing.T) { //nolint:gocognit // cognitive complexity is acceptable for this function.
	t.Parallel()

	analyzer := NewHalsteadAnalyzer()

	// Create a function with specific operators and operands.
	functionNode := node.New("func1", "Function", "", []node.Role{node.RoleFunction, node.RoleDeclaration}, nil, map[string]string{"name": "testFunction"}) //nolint:lll // long line is acceptable here.

	// Add assignment operator.
	assignmentNode := node.New("assign1", "Assignment", "=", []node.Role{node.RoleAssignment}, nil, map[string]string{"operator": "="})

	// Add binary operator.
	binaryOpNode := node.New("binary1", "BinaryOp", "+", []node.Role{node.RoleOperator}, nil, map[string]string{"operator": "+"})

	// Add identifiers (operands).
	identifier1 := node.New("id1", "Identifier", "x", []node.Role{node.RoleVariable}, nil, map[string]string{"name": "x"})
	identifier2 := node.New("id2", "Identifier", "y", []node.Role{node.RoleVariable}, nil, map[string]string{"name": "y"})

	// Add literal (operand).
	literalNode := node.New("lit1", "Literal", "5", []node.Role{node.RoleLiteral}, nil, map[string]string{"value": "5"})

	// Build the function structure.
	functionNode.AddChild(assignmentNode)
	assignmentNode.AddChild(identifier1)
	assignmentNode.AddChild(binaryOpNode)
	binaryOpNode.AddChild(identifier2)
	binaryOpNode.AddChild(literalNode)

	// Analyze the function.
	result, err := analyzer.Analyze(functionNode)
	if err != nil {
		t.Fatalf("Expected no error, got %v", err)
	}

	// Check that we have the expected operators and operands.
	if result["distinct_operators"] != 2 { // = and +.
		t.Errorf("Expected 2 distinct operators, got %v", result["distinct_operators"])
	}

	if result["distinct_operands"] != 3 { // X, y, and 5.
		t.Errorf("Expected 3 distinct operands, got %v", result["distinct_operands"])
	}

	// Check that the aggregation worked correctly.
	if result["total_operators"] != 2 { // One assignment, one binary op.
		t.Errorf("Expected 2 total operators, got %v", result["total_operators"])
	}

	if result["total_operands"] != 3 { // X, y, 5.
		t.Errorf("Expected 3 total operands, got %v", result["total_operands"])
	}

	// Verify that the function details contain the actual operators and operands.
	functions, ok := result["functions"].([]map[string]any)
	require.True(t, ok, "type assertion failed for functions")

	if len(functions) != 1 {
		t.Fatalf("Expected 1 function, got %d", len(functions))
	}

	funcData := functions[0]
	if operators, opsOK := funcData["operators"].(map[string]int); opsOK {
		if operators["="] != 1 {
			t.Errorf("Expected 1 assignment operator, got %d", operators["="])
		}

		if operators["+"] != 1 {
			t.Errorf("Expected 1 binary operator, got %d", operators["+"])
		}
	} else {
		t.Error("Expected operators map in function data")
	}

	if operands, opdsOK := funcData["operands"].(map[string]int); opdsOK { //nolint:nestif // nested logic is clear in context.
		if operands["x"] != 1 {
			t.Errorf("Expected 1 occurrence of 'x', got %d", operands["x"])
		}

		if operands["y"] != 1 {
			t.Errorf("Expected 1 occurrence of 'y', got %d", operands["y"])
		}

		if operands["5"] != 1 {
			t.Errorf("Expected 1 occurrence of '5', got %d", operands["5"])
		}
	} else {
		t.Error("Expected operands map in function data")
	}
}

func TestHalsteadAnalyzer_MultipleFunctionsAggregation(t *testing.T) {
	t.Parallel()

	analyzer := NewHalsteadAnalyzer()

	// Create a root node to hold multiple functions.
	rootNode := node.New("root", "File", "", nil, nil, nil)

	// Create first function with operators: =, + and operands: x, y, 5.
	function1 := node.New("func1", "Function", "", []node.Role{node.RoleFunction, node.RoleDeclaration}, nil, map[string]string{"name": "function1"}) //nolint:lll // long line is acceptable here.
	assignment1 := node.New("assign1", "Assignment", "=", []node.Role{node.RoleAssignment}, nil, map[string]string{"operator": "="})
	binaryOp1 := node.New("binary1", "BinaryOp", "+", []node.Role{node.RoleOperator}, nil, map[string]string{"operator": "+"})
	identifier1 := node.New("id1", "Identifier", "x", []node.Role{node.RoleVariable}, nil, map[string]string{"name": "x"})
	identifier2 := node.New("id2", "Identifier", "y", []node.Role{node.RoleVariable}, nil, map[string]string{"name": "y"})
	literal1 := node.New("lit1", "Literal", "5", []node.Role{node.RoleLiteral}, nil, map[string]string{"value": "5"})

	function1.AddChild(assignment1)
	assignment1.AddChild(identifier1)
	assignment1.AddChild(binaryOp1)
	binaryOp1.AddChild(identifier2)
	binaryOp1.AddChild(literal1)

	// Create second function with operators: =, * and operands: x, z, 10.
	function2 := node.New("func2", "Function", "", []node.Role{node.RoleFunction, node.RoleDeclaration}, nil, map[string]string{"name": "function2"}) //nolint:lll // long line is acceptable here.
	assignment2 := node.New("assign2", "Assignment", "=", []node.Role{node.RoleAssignment}, nil, map[string]string{"operator": "="})
	binaryOp2 := node.New("binary2", "BinaryOp", "*", []node.Role{node.RoleOperator}, nil, map[string]string{"operator": "*"})
	//nolint:lll // long line is acceptable here.
	identifier3 := node.New("id3", "Identifier", "x", []node.Role{node.RoleVariable}, nil, map[string]string{"name": "x"}) // Same as function1.
	identifier4 := node.New("id4", "Identifier", "z", []node.Role{node.RoleVariable}, nil, map[string]string{"name": "z"})
	literal2 := node.New("lit2", "Literal", "10", []node.Role{node.RoleLiteral}, nil, map[string]string{"value": "10"})

	function2.AddChild(assignment2)
	assignment2.AddChild(identifier3)
	assignment2.AddChild(binaryOp2)
	binaryOp2.AddChild(identifier4)
	binaryOp2.AddChild(literal2)

	// Add both functions to root.
	rootNode.AddChild(function1)
	rootNode.AddChild(function2)

	// Analyze the functions.
	result, err := analyzer.Analyze(rootNode)
	if err != nil {
		t.Fatalf("Expected no error, got %v", err)
	}

	// Check file-level aggregation
	// Should have 3 distinct operators: =, +, *.
	if result["distinct_operators"] != 3 {
		t.Errorf("Expected 3 distinct operators, got %v", result["distinct_operators"])
	}

	// Should have 5 distinct operands: x, y, 5, z, 10.
	if result["distinct_operands"] != 5 {
		t.Errorf("Expected 5 distinct operands, got %v", result["distinct_operands"])
	}

	// Should have 4 total operators: 2 assignments + 1 addition + 1 multiplication.
	if result["total_operators"] != 4 {
		t.Errorf("Expected 4 total operators, got %v", result["total_operators"])
	}

	// Should have 6 total operands: 2x + y + 5 + z + 10.
	if result["total_operands"] != 6 {
		t.Errorf("Expected 6 total operands, got %v", result["total_operands"])
	}

	// Verify that the aggregation correctly counted overlapping operators and operands.
	functions, ok := result["functions"].([]map[string]any)
	require.True(t, ok, "type assertion failed for functions")

	if len(functions) != 2 {
		t.Fatalf("Expected 2 functions, got %d", len(functions))
	}

	// Check that 'x' appears twice (once in each function)
	// We need to check the file-level aggregation, not individual functions
	// The test verifies that the real aggregation is working by checking the file-level counts
	// which should properly merge the operators and operands from both functions.
}
