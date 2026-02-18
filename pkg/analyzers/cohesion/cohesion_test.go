package cohesion

import (
	"bytes"
	"encoding/json"
	"fmt"
	"math"
	"strings"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

func TestAnalyzer_Name(t *testing.T) {
	t.Parallel()

	analyzer := NewAnalyzer()

	expected := "cohesion"
	if got := analyzer.Name(); got != expected {
		t.Errorf("Analyzer.Name() = %v, want %v", got, expected)
	}
}

func TestAnalyzer_Thresholds(t *testing.T) {
	t.Parallel()

	analyzer := NewAnalyzer()
	thresholds := analyzer.Thresholds()

	// Check that all expected metrics are present.
	expectedMetrics := []string{"lcom", "cohesion_score", "function_cohesion"}
	for _, metric := range expectedMetrics {
		if _, exists := thresholds[metric]; !exists {
			t.Errorf("Expected threshold for metric '%s' not found", metric)
		}
	}

	// Check specific threshold values.
	if lcom, exists := thresholds["lcom"]; exists {
		if red, ok := lcom["red"].(float64); !ok || red != 4.0 {
			t.Errorf("Expected LCOM red threshold to be 4.0, got %v", red)
		}

		if yellow, ok := lcom["yellow"].(float64); !ok || yellow != 2.0 {
			t.Errorf("Expected LCOM yellow threshold to be 2.0, got %v", yellow)
		}

		if green, ok := lcom["green"].(float64); !ok || green != 1.0 {
			t.Errorf("Expected LCOM green threshold to be 1.0, got %v", green)
		}
	}
}

func TestAnalyzer_Analyze_NilRoot(t *testing.T) {
	t.Parallel()

	analyzer := NewAnalyzer()

	_, err := analyzer.Analyze(nil)
	if err == nil {
		t.Error("Expected error when root node is nil")
	}

	if !strings.Contains(err.Error(), "root node is nil") {
		t.Errorf("Expected error message to contain 'root node is nil', got: %v", err.Error())
	}
}

func TestAnalyzer_Analyze_NoFunctions(t *testing.T) {
	t.Parallel()

	analyzer := NewAnalyzer()

	// Create a simple UAST with no functions.
	root := &node.Node{
		Type: "File",
		Children: []*node.Node{
			{
				Type:  "Comment",
				Token: "// This is a comment",
			},
		},
	}

	report, err := analyzer.Analyze(root)
	if err != nil {
		t.Fatalf("Unexpected error: %v", err)
	}

	// Check expected values for no functions.
	if totalFunctions, ok := report["total_functions"].(int); !ok || totalFunctions != 0 {
		t.Errorf("Expected total_functions to be 0, got %v", totalFunctions)
	}

	if lcom, ok := report["lcom"].(float64); !ok || lcom != 0.0 {
		t.Errorf("Expected lcom to be 0.0, got %v", lcom)
	}

	if cohesionScore, ok := report["cohesion_score"].(float64); !ok || cohesionScore != 1.0 {
		t.Errorf("Expected cohesion_score to be 1.0, got %v", cohesionScore)
	}

	if message, ok := report["message"].(string); !ok || message != "No functions found" {
		t.Errorf("Expected message to be 'No functions found', got %v", message)
	}
}

func TestAnalyzer_Analyze_SingleFunction(t *testing.T) {
	t.Parallel()

	analyzer := NewAnalyzer()

	// Create a UAST with a single function.
	root := &node.Node{
		Type: "File",
		Children: []*node.Node{
			{
				Type:  "Function",
				Roles: []node.Role{"Function", "Declaration"},
				Props: map[string]string{"name": "simpleFunction"},
				Children: []*node.Node{
					{
						Type:  "Parameter",
						Roles: []node.Role{"Parameter"},
						Props: map[string]string{"name": "x"},
						Children: []*node.Node{
							{
								Type:  "Identifier",
								Token: "x",
								Roles: []node.Role{"Name"},
							},
						},
					},
					{
						Type:  "Block",
						Roles: []node.Role{"Body"},
						Children: []*node.Node{
							{
								Type:  "Return",
								Roles: []node.Role{"Return"},
								Children: []*node.Node{
									{
										Type:  "Identifier",
										Token: "x",
										Roles: []node.Role{"Name"},
									},
								},
							},
						},
					},
				},
			},
		},
	}

	report, err := analyzer.Analyze(root)
	if err != nil {
		t.Fatalf("Unexpected error: %v", err)
	}

	// Check expected values for single function.
	if totalFunctions, ok := report["total_functions"].(int); !ok || totalFunctions != 1 {
		t.Errorf("Expected total_functions to be 1, got %v", totalFunctions)
	}

	if lcom, ok := report["lcom"].(float64); !ok || lcom != 0.0 {
		t.Errorf("Expected lcom to be 0.0 for single function, got %v", lcom)
	}

	if cohesionScore, ok := report["cohesion_score"].(float64); !ok || cohesionScore != 1.0 {
		t.Errorf("Expected cohesion_score to be 1.0 for single function, got %v", cohesionScore)
	}
}

func TestAnalyzer_Analyze_MultipleFunctions(t *testing.T) {
	t.Parallel()

	analyzer := NewAnalyzer()

	// Create a UAST with multiple functions that share variables.
	root := &node.Node{
		Type: "File",
		Children: []*node.Node{
			// Function 1.
			{
				Type:  "Function",
				Roles: []node.Role{"Function", "Declaration"},
				Props: map[string]string{"name": "function1"},
				Children: []*node.Node{
					{
						Type:  "Parameter",
						Roles: []node.Role{"Parameter"},
						Props: map[string]string{"name": "sharedVar"},
						Children: []*node.Node{
							{
								Type:  "Identifier",
								Token: "sharedVar",
								Roles: []node.Role{"Name"},
							},
						},
					},
					{
						Type:  "Variable",
						Roles: []node.Role{"Variable", "Declaration"},
						Props: map[string]string{"name": "localVar1"},
						Children: []*node.Node{
							{
								Type:  "Identifier",
								Token: "localVar1",
								Roles: []node.Role{"Name"},
							},
						},
					},
				},
			},
			// Function 2.
			{
				Type:  "Function",
				Roles: []node.Role{"Function", "Declaration"},
				Props: map[string]string{"name": "function2"},
				Children: []*node.Node{
					{
						Type:  "Parameter",
						Roles: []node.Role{"Parameter"},
						Props: map[string]string{"name": "sharedVar"},
						Children: []*node.Node{
							{
								Type:  "Identifier",
								Token: "sharedVar",
								Roles: []node.Role{"Name"},
							},
						},
					},
					{
						Type:  "Variable",
						Roles: []node.Role{"Variable", "Declaration"},
						Props: map[string]string{"name": "localVar2"},
						Children: []*node.Node{
							{
								Type:  "Identifier",
								Token: "localVar2",
								Roles: []node.Role{"Name"},
							},
						},
					},
				},
			},
		},
	}

	report, err := analyzer.Analyze(root)
	if err != nil {
		t.Fatalf("Unexpected error: %v", err)
	}

	// Check expected values for multiple functions.
	if totalFunctions, ok := report["total_functions"].(int); !ok || totalFunctions != 2 {
		t.Errorf("Expected total_functions to be 2, got %v", totalFunctions)
	}

	// Functions share a variable, so LCOM should be lower (can be negative when functions share many variables).
	if lcom, ok := report["lcom"].(float64); !ok {
		t.Errorf("Expected lcom to be a float64, got %v", lcom)
	}
	// LCOM can be negative when functions share many variables (good cohesion)
	// LCOM can be positive when functions don't share variables (poor cohesion).
}

func TestAnalyzer_FormatReport(t *testing.T) {
	t.Parallel()

	analyzer := NewAnalyzer()

	// Create a test report.
	report := analyze.Report{
		"total_functions":   2,
		"lcom":              1.5,
		"cohesion_score":    0.7,
		"function_cohesion": 0.6,
		"message":           "Good cohesion - functions are generally well-organized",
		"functions": []map[string]any{
			{
				"name":           "testFunction1",
				"line_count":     5,
				"variable_count": 3,
				"cohesion":       0.8,
			},
			{
				"name":           "testFunction2",
				"line_count":     8,
				"variable_count": 6,
				"cohesion":       0.4,
			},
		},
	}

	var buf bytes.Buffer

	err := analyzer.FormatReport(report, &buf)
	if err != nil {
		t.Fatalf("Unexpected error: %v", err)
	}

	output := buf.String()

	// Check that the report contains expected sections from SectionRenderer.
	expectedSections := []string{
		"COHESION",
		"Score: 7/10",
		"Good cohesion",
		"Total Functions",
		"LCOM Score",
		"Cohesion Score",
	}

	for _, section := range expectedSections {
		if !strings.Contains(output, section) {
			t.Errorf("Expected output to contain '%s', but it was not found", section)
		}
	}
}

func TestAnalyzer_FormatReportJSON(t *testing.T) {
	t.Parallel()

	analyzer := NewAnalyzer()

	// Create a test report.
	report := analyze.Report{
		"total_functions":   1,
		"lcom":              0.0,
		"cohesion_score":    1.0,
		"function_cohesion": 1.0,
		"message":           "Excellent cohesion",
		"functions": []map[string]any{
			{
				"name":           "simpleFunction",
				"line_count":     3,
				"variable_count": 1,
				"cohesion":       1.0,
			},
		},
	}

	var buf bytes.Buffer

	err := analyzer.FormatReportJSON(report, &buf)
	require.NoError(t, err)

	output := buf.String()

	// Verify it's valid JSON.
	var jsonData map[string]any

	err = json.Unmarshal([]byte(output), &jsonData)
	require.NoError(t, err, "Generated output is not valid JSON")

	// Check that the JSON contains metrics structure fields.
	assert.Contains(t, jsonData, "function_cohesion")
	assert.Contains(t, jsonData, "distribution")
	assert.Contains(t, jsonData, "low_cohesion_functions")
	assert.Contains(t, jsonData, "aggregate")
}

func TestAnalyzer_CreateAggregator(t *testing.T) {
	t.Parallel()

	analyzer := NewAnalyzer()
	aggregator := analyzer.CreateAggregator()

	if aggregator == nil {
		t.Error("Expected CreateAggregator to return a non-nil aggregator")
	}

	// Check that it's the right type.
	if _, ok := aggregator.(*Aggregator); !ok {
		t.Error("Expected CreateAggregator to return a Aggregator")
	}
}

func TestAggregator_Aggregate(t *testing.T) {
	t.Parallel()

	aggregator := NewAggregator()

	// Create test results.
	results := map[string]analyze.Report{
		"file1": {
			"total_functions":   2,
			"lcom":              1.0,
			"cohesion_score":    0.8,
			"function_cohesion": 0.7,
			"functions": []map[string]any{
				{
					"name":           "function1",
					"line_count":     5,
					"variable_count": 2,
					"cohesion":       0.8,
				},
				{
					"name":           "function2",
					"line_count":     8,
					"variable_count": 4,
					"cohesion":       0.6,
				},
			},
		},
		"file2": {
			"total_functions":   1,
			"lcom":              0.0,
			"cohesion_score":    1.0,
			"function_cohesion": 1.0,
			"functions": []map[string]any{
				{
					"name":           "function3",
					"line_count":     3,
					"variable_count": 1,
					"cohesion":       1.0,
				},
			},
		},
	}

	aggregator.Aggregate(results)

	// Check aggregated values through the result.
	result := aggregator.GetResult()

	if totalFunctions, ok := result["total_functions"].(int); !ok || totalFunctions != 3 {
		t.Errorf("Expected total_functions to be 3, got %v", totalFunctions)
	}

	if lcom, ok := result["lcom"].(float64); !ok || lcom != 0.5 {
		t.Errorf("Expected lcom to be 0.5, got %v", lcom)
	}

	if cohesionScore, ok := result["cohesion_score"].(float64); !ok || cohesionScore != 0.9 {
		t.Errorf("Expected cohesion_score to be 0.9, got %v", cohesionScore)
	}

	if functions, ok := result["functions"].([]map[string]any); !ok || len(functions) != 3 {
		t.Errorf("Expected 3 functions, got %d", len(functions))
	}
}

func TestAggregator_GetResult(t *testing.T) {
	t.Parallel()

	aggregator := NewAggregator()

	// Create test results to populate the aggregator.
	results := map[string]analyze.Report{
		"file1": {
			"total_functions":   2,
			"lcom":              2.0,
			"cohesion_score":    1.5,
			"function_cohesion": 0.8,
			"functions": []map[string]any{
				{
					"name":           "func1",
					"line_count":     5,
					"variable_count": 2,
					"cohesion":       0.8,
				},
				{
					"name":           "func2",
					"line_count":     8,
					"variable_count": 4,
					"cohesion":       0.7,
				},
			},
		},
	}

	aggregator.Aggregate(results)
	result := aggregator.GetResult()

	// Check result structure.
	if totalFunctions, ok := result["total_functions"].(int); !ok || totalFunctions != 2 {
		t.Errorf("Expected total_functions to be 2, got %v", totalFunctions)
	}

	if lcom, ok := result["lcom"].(float64); !ok || lcom != 2.0 {
		t.Errorf("Expected lcom to be 2.0, got %v", lcom)
	}

	if cohesionScore, ok := result["cohesion_score"].(float64); !ok || cohesionScore != 1.5 {
		t.Errorf("Expected cohesion_score to be 1.5, got %v", cohesionScore)
	}

	if functions, ok := result["functions"].([]map[string]any); !ok || len(functions) != 2 {
		t.Errorf("Expected 2 functions in result, got %d", len(functions))
	}
}

func TestAggregator_GetResult_NoFunctions(t *testing.T) {
	t.Parallel()

	aggregator := NewAggregator()

	result := aggregator.GetResult()

	// Check expected values for no functions.
	if totalFunctions, ok := result["total_functions"].(int); !ok || totalFunctions != 0 {
		t.Errorf("Expected total_functions to be 0, got %v", totalFunctions)
	}

	if lcom, ok := result["lcom"].(float64); !ok || lcom != 0.0 {
		t.Errorf("Expected lcom to be 0.0, got %v", lcom)
	}

	if cohesionScore, ok := result["cohesion_score"].(float64); !ok || cohesionScore != 1.0 {
		t.Errorf("Expected cohesion_score to be 1.0, got %v", cohesionScore)
	}
}

func TestAnalyzer_HelperFunctions(t *testing.T) {
	t.Parallel()

	analyzer := NewAnalyzer()

	// Test haveSharedVariables.
	fn1 := Function{
		Name:      "func1",
		Variables: []string{"a", "b", "c"},
	}
	fn2 := Function{
		Name:      "func2",
		Variables: []string{"b", "d", "e"},
	}
	fn3 := Function{
		Name:      "func3",
		Variables: []string{"x", "y", "z"},
	}

	if !analyzer.haveSharedVariables(fn1, fn2) {
		t.Error("Expected fn1 and fn2 to have shared variables")
	}

	if analyzer.haveSharedVariables(fn1, fn3) {
		t.Error("Expected fn1 and fn3 to not have shared variables")
	}

	// Test calculateCohesionScore.
	score1 := analyzer.calculateCohesionScore(0.0, 1)
	if score1 != 1.0 {
		t.Errorf("Expected cohesion score to be 1.0 for single function, got %f", score1)
	}

	score2 := analyzer.calculateCohesionScore(2.0, 3)
	if score2 <= 0.0 || score2 > 1.0 {
		t.Errorf("Expected cohesion score to be between 0 and 1, got %f", score2)
	}

	// Test calculateFunctionCohesion.
	functions := []Function{
		{Cohesion: 0.8},
		{Cohesion: 0.6},
		{Cohesion: 1.0},
	}
	avgCohesion := analyzer.calculateFunctionCohesion(functions)

	expected := (0.8 + 0.6 + 1.0) / 3.0
	if math.Abs(avgCohesion-expected) > 0.0001 {
		t.Errorf("Expected average cohesion to be %f, got %f", expected, avgCohesion)
	}

	// Test getCohesionMessage.
	message1 := analyzer.getCohesionMessage(0.9)
	if !strings.Contains(message1, "Excellent") {
		t.Errorf("Expected excellent message for score 0.9, got: %s", message1)
	}

	message2 := analyzer.getCohesionMessage(0.2)
	if !strings.Contains(message2, "Poor") {
		t.Errorf("Expected poor message for score 0.2, got: %s", message2)
	}
}

func TestAnalyzer_EdgeCases(t *testing.T) {
	t.Parallel()

	analyzer := NewAnalyzer()

	// Test with empty function list.
	lcom := analyzer.calculateLCOM([]Function{})
	if lcom != 0.0 {
		t.Errorf("Expected LCOM to be 0.0 for empty function list, got %f", lcom)
	}

	// Test with single function.
	lcom = analyzer.calculateLCOM([]Function{{Name: "single"}})
	if lcom != 0.0 {
		t.Errorf("Expected LCOM to be 0.0 for single function, got %f", lcom)
	}

	// Test function-level cohesion with zero lines.
	fn := Function{LineCount: 0, Variables: []string{"a", "b"}}

	cohesion := analyzer.calculateFunctionLevelCohesion(fn)
	if cohesion != 1.0 {
		t.Errorf("Expected cohesion to be 1.0 for zero lines, got %f", cohesion)
	}

	// Test function-level cohesion with high variable density.
	fn = Function{LineCount: 2, Variables: []string{"a", "b", "c", "d", "e"}}

	cohesion = analyzer.calculateFunctionLevelCohesion(fn)
	if cohesion < 0.0 || cohesion > 1.0 {
		t.Errorf("Expected cohesion to be between 0 and 1, got %f", cohesion)
	}
}

func TestAnalyzer_ImprovedFunctionCohesion(t *testing.T) {
	t.Parallel()

	analyzer := NewAnalyzer()

	testCases := []struct {
		name        string
		description string
		function    Function
		expectedMin float64
		expectedMax float64
	}{
		{
			name: "Small function with few variables",
			function: Function{
				Name:      "register",
				LineCount: 3,
				Variables: []string{"analyzer", "name"},
			},
			expectedMin: 1.0,
			expectedMax: 1.0,
			description: "Small, focused functions should have perfect cohesion",
		},
		{
			name: "Small function at boundary",
			function: Function{
				Name:      "process",
				LineCount: 5,
				Variables: []string{"input", "output", "temp"},
			},
			expectedMin: 1.0,
			expectedMax: 1.0,
			description: "Small functions with 3 or fewer variables should have perfect cohesion",
		},
		{
			name: "Small function with more variables",
			function: Function{
				Name:      "validate",
				LineCount: 4,
				Variables: []string{"a", "b", "c", "d", "e"},
			},
			expectedMin: 0.7,
			expectedMax: 0.8,
			description: "Small functions with more variables should have gentle penalty",
		},
		{
			name: "Medium function with low density",
			function: Function{
				Name:      "process",
				LineCount: 10,
				Variables: []string{"input", "output", "temp"},
			},
			expectedMin: 0.7,
			expectedMax: 1.0,
			description: "Medium functions with low variable density should have good cohesion",
		},
		{
			name: "Large function with moderate density",
			function: Function{
				Name:      "complex",
				LineCount: 20,
				Variables: []string{"a", "b", "c", "d", "e", "f", "g", "h", "i", "j"},
			},
			expectedMin: 0.2,
			expectedMax: 0.8,
			description: "Large functions should use logarithmic scaling",
		},
		{
			name: "Single line function",
			function: Function{
				Name:      "getter",
				LineCount: 1,
				Variables: []string{"field"},
			},
			expectedMin: 1.0,
			expectedMax: 1.0,
			description: "Single line functions should have perfect cohesion",
		},
	}

	for _, tc := range testCases {
		t.Run(tc.name, func(t *testing.T) {
			t.Parallel()

			cohesion := analyzer.calculateFunctionLevelCohesion(tc.function)

			if cohesion < tc.expectedMin || cohesion > tc.expectedMax {
				t.Errorf("%s: Expected cohesion between %.2f and %.2f, got %.2f",
					tc.description, tc.expectedMin, tc.expectedMax, cohesion)
			}

			// Ensure cohesion is always between 0 and 1.
			if cohesion < 0.0 || cohesion > 1.0 {
				t.Errorf("Cohesion must be between 0 and 1, got %.2f", cohesion)
			}
		})
	}
}
func TestAnalyzer_Integration(t *testing.T) {
	t.Parallel()

	analyzer := NewAnalyzer()

	// Create a realistic UAST structure.
	root := &node.Node{
		Type: "File",
		Children: []*node.Node{
			// Class/Struct.
			{
				Type:  "Class",
				Roles: []node.Role{"Class", "Declaration"},
				Props: map[string]string{"name": "Calculator"},
				Children: []*node.Node{
					// Method 1.
					{
						Type:  "Method",
						Roles: []node.Role{"Function", "Declaration", "Member"},
						Props: map[string]string{"name": "add"},
						Children: []*node.Node{
							{
								Type:  "Parameter",
								Roles: []node.Role{"Parameter"},
								Props: map[string]string{"name": "a"},
								Children: []*node.Node{
									{
										Type:  "Identifier",
										Token: "a",
										Roles: []node.Role{"Name"},
									},
								},
							},
							{
								Type:  "Parameter",
								Roles: []node.Role{"Parameter"},
								Props: map[string]string{"name": "b"},
								Children: []*node.Node{
									{
										Type:  "Identifier",
										Token: "b",
										Roles: []node.Role{"Name"},
									},
								},
							},
							{
								Type:  "Block",
								Roles: []node.Role{"Body"},
								Children: []*node.Node{
									{
										Type:  "Variable",
										Roles: []node.Role{"Variable", "Declaration"},
										Props: map[string]string{"name": "result"},
										Children: []*node.Node{
											{
												Type:  "Identifier",
												Token: "result",
												Roles: []node.Role{"Name"},
											},
										},
									},
								},
							},
						},
					},
					// Method 2.
					{
						Type:  "Method",
						Roles: []node.Role{"Function", "Declaration", "Member"},
						Props: map[string]string{"name": "multiply"},
						Children: []*node.Node{
							{
								Type:  "Parameter",
								Roles: []node.Role{"Parameter"},
								Props: map[string]string{"name": "x"},
								Children: []*node.Node{
									{
										Type:  "Identifier",
										Token: "x",
										Roles: []node.Role{"Name"},
									},
								},
							},
							{
								Type:  "Parameter",
								Roles: []node.Role{"Parameter"},
								Props: map[string]string{"name": "y"},
								Children: []*node.Node{
									{
										Type:  "Identifier",
										Token: "y",
										Roles: []node.Role{"Name"},
									},
								},
							},
						},
					},
				},
			},
			// Standalone function.
			{
				Type:  "Function",
				Roles: []node.Role{"Function", "Declaration"},
				Props: map[string]string{"name": "main"},
				Children: []*node.Node{
					{
						Type:  "Block",
						Roles: []node.Role{"Body"},
						Children: []*node.Node{
							{
								Type:  "Variable",
								Roles: []node.Role{"Variable", "Declaration"},
								Props: map[string]string{"name": "calc"},
								Children: []*node.Node{
									{
										Type:  "Identifier",
										Token: "calc",
										Roles: []node.Role{"Name"},
									},
								},
							},
						},
					},
				},
			},
		},
	}

	report, err := analyzer.Analyze(root)
	if err != nil {
		t.Fatalf("Unexpected error: %v", err)
	}

	// Verify the analysis found the expected functions.
	if totalFunctions, ok := report["total_functions"].(int); !ok || totalFunctions != 3 {
		t.Errorf("Expected 3 functions (2 methods + 1 function), got %v", totalFunctions)
	}

	// Verify functions are present in the report.
	if functions, fnOK := report["functions"].([]map[string]any); fnOK {
		functionNames := make(map[string]bool)

		for _, fn := range functions {
			if name, nameOK := fn["name"].(string); nameOK {
				functionNames[name] = true
			}
		}

		expectedNames := []string{"add", "multiply", "main"}
		for _, name := range expectedNames {
			if !functionNames[name] {
				t.Errorf("Expected function '%s' to be found in analysis", name)
			}
		}
	}

	// Test aggregator with this result.
	aggregator := analyzer.CreateAggregator()
	results := map[string]analyze.Report{"test": report}
	aggregator.Aggregate(results)

	finalResult := aggregator.GetResult()
	if finalResult == nil {
		t.Error("Expected GetResult to return a non-nil report")
	}
}

// Benchmark tests.
func BenchmarkAnalyzer_Analyze(b *testing.B) {
	analyzer := NewAnalyzer()

	// Create a complex UAST for benchmarking.
	root := createComplexUAST()

	b.ResetTimer()

	for b.Loop() {
		_, err := analyzer.Analyze(root)
		if err != nil {
			b.Fatalf("Unexpected error: %v", err)
		}
	}
}

func BenchmarkAggregator_Aggregate(b *testing.B) {
	aggregator := NewAggregator()

	// Create test results for benchmarking.
	results := createBenchmarkResults()

	b.ResetTimer()

	for b.Loop() {
		aggregator.Aggregate(results)
	}
}

// Helper functions for benchmarks.
func createComplexUAST() *node.Node {
	// Create a UAST with many functions for benchmarking.
	children := make([]*node.Node, 0, 100)

	for i := range 50 {
		children = append(children, &node.Node{
			Type:  "Function",
			Roles: []node.Role{"Function", "Declaration"},
			Props: map[string]string{"name": fmt.Sprintf("function%d", i)},
			Children: []*node.Node{
				{
					Type:  "Parameter",
					Roles: []node.Role{"Parameter"},
					Props: map[string]string{"name": "param"},
					Children: []*node.Node{
						{
							Type:  "Identifier",
							Token: "param",
							Roles: []node.Role{"Name"},
						},
					},
				},
				{
					Type:  "Block",
					Roles: []node.Role{"Body"},
					Children: []*node.Node{
						{
							Type:  "Variable",
							Roles: []node.Role{"Variable", "Declaration"},
							Props: map[string]string{"name": "localVar"},
							Children: []*node.Node{
								{
									Type:  "Identifier",
									Token: "localVar",
									Roles: []node.Role{"Name"},
								},
							},
						},
					},
				},
			},
		})
	}

	return &node.Node{
		Type:     "File",
		Children: children,
	}
}

func createBenchmarkResults() map[string]analyze.Report {
	results := make(map[string]analyze.Report)

	for i := range 10 {
		results[fmt.Sprintf("file%d", i)] = analyze.Report{
			"total_functions":   5,
			"lcom":              float64(i),
			"cohesion_score":    0.5 + float64(i)*0.05,
			"function_cohesion": 0.6 + float64(i)*0.04,
			"functions": []map[string]any{
				{
					"name":           fmt.Sprintf("func%d", i),
					"line_count":     10,
					"variable_count": 3,
					"cohesion":       0.7,
				},
			},
		}
	}

	return results
}

// --- FormatReportYAML Test ---.

func TestAnalyzer_FormatReportYAML(t *testing.T) {
	t.Parallel()

	analyzer := NewAnalyzer()

	report := analyze.Report{
		"total_functions":   3,
		"lcom":              2.5,
		"cohesion_score":    0.7,
		"function_cohesion": 0.65,
		"message":           "Test message",
		"functions": []map[string]any{
			{"name": "testFunc", "cohesion": 0.8},
		},
	}

	var buf bytes.Buffer

	err := analyzer.FormatReportYAML(report, &buf)
	require.NoError(t, err)

	output := buf.String()
	assert.Contains(t, output, "function_cohesion:")
	assert.Contains(t, output, "distribution:")
	assert.Contains(t, output, "aggregate:")
}
