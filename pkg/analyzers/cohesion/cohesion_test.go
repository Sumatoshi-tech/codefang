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

	// LCOM-HS thresholds: lower is better.
	if lcom, exists := thresholds["lcom"]; exists {
		if green, ok := lcom["green"].(float64); !ok || green != 0.3 {
			t.Errorf("Expected LCOM green threshold to be 0.3, got %v", green)
		}

		if yellow, ok := lcom["yellow"].(float64); !ok || yellow != 0.6 {
			t.Errorf("Expected LCOM yellow threshold to be 0.6, got %v", yellow)
		}

		if red, ok := lcom["red"].(float64); !ok || red != 1.0 {
			t.Errorf("Expected LCOM red threshold to be 1.0, got %v", red)
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
			// Function 1: uses sharedVar and localVar1.
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
			// Function 2: uses sharedVar and localVar2.
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

	// LCOM-HS: 3 unique vars (sharedVar, localVar1, localVar2).
	// sharedVar accessed by 2 functions, localVar1 by 1, localVar2 by 1 = sumMA = 4.
	// LCOM = 1 - 4/(2*3) = 1 - 0.667 = 0.333.
	if lcom, ok := report["lcom"].(float64); ok {
		assert.InDelta(t, 1.0/3.0, lcom, 0.01, "LCOM-HS should be ~0.33 for partially shared variables")
	}
}

func TestAnalyzer_FormatReport(t *testing.T) {
	t.Parallel()

	analyzer := NewAnalyzer()

	// Create a test report.
	report := analyze.Report{
		"total_functions":   2,
		"lcom":              0.3,
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

	// Verify LCOM variant is present in aggregate.
	if agg, ok := jsonData["aggregate"].(map[string]any); ok {
		assert.Equal(t, "LCOM-HS (Henderson-Sellers)", agg["lcom_variant"])
	}
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
			"lcom":              0.3,
			"cohesion_score":    0.7,
			"function_cohesion": 0.6,
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
					"cohesion":       0.5,
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

	// Aggregation averages numeric keys: (0.3 + 0.0) / 2 = 0.15.
	if lcom, ok := result["lcom"].(float64); ok {
		assert.InDelta(t, 0.15, lcom, 0.01, "LCOM should be average of file LCOMs")
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
			"lcom":              0.4,
			"cohesion_score":    0.6,
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

	if lcom, ok := result["lcom"].(float64); !ok || lcom != 0.4 {
		t.Errorf("Expected lcom to be 0.4, got %v", lcom)
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

// TestAnalyzer_HelperFunctions tests core calculation helpers.
func TestAnalyzer_HelperFunctions(t *testing.T) {
	t.Parallel()

	analyzer := NewAnalyzer()

	// Test calculateCohesionScore.
	score1 := analyzer.calculateCohesionScore(0.0, 1)
	if score1 != 1.0 {
		t.Errorf("Expected cohesion score to be 1.0 for single function, got %f", score1)
	}

	// LCOM-HS of 0.3 -> cohesion = 1.0 - 0.3 = 0.7.
	score2 := analyzer.calculateCohesionScore(0.3, 3)
	assert.InDelta(t, 0.7, score2, 0.001)

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

	// Test getCohesionMessage with new thresholds.
	message1 := analyzer.getCohesionMessage(0.8)
	if !strings.Contains(message1, "Excellent") {
		t.Errorf("Expected excellent message for score 0.8, got: %s", message1)
	}

	message2 := analyzer.getCohesionMessage(0.2)
	if !strings.Contains(message2, "Poor") {
		t.Errorf("Expected poor message for score 0.2, got: %s", message2)
	}
}

// TestCalculateLCOM_HS verifies the Henderson-Sellers LCOM formula against known answers.
func TestCalculateLCOM_HS(t *testing.T) {
	t.Parallel()

	analyzer := NewAnalyzer()

	testCases := []struct {
		name      string
		functions []Function
		expected  float64
	}{
		{
			name:      "empty function list",
			functions: []Function{},
			expected:  0.0,
		},
		{
			name:      "single function",
			functions: []Function{{Name: "f1", Variables: []string{"a", "b"}}},
			expected:  0.0,
		},
		{
			name: "perfect cohesion - all functions share all variables",
			functions: []Function{
				{Name: "f1", Variables: []string{"a", "b"}},
				{Name: "f2", Variables: []string{"a", "b"}},
			},
			// 2 vars, each accessed by 2 funcs: sumMA = 4, LCOM = 1 - 4/(2*2) = 0.0.
			expected: 0.0,
		},
		{
			name: "no cohesion - no shared variables",
			functions: []Function{
				{Name: "f1", Variables: []string{"a"}},
				{Name: "f2", Variables: []string{"b"}},
			},
			// 2 vars, each accessed by 1 func: sumMA = 2, LCOM = 1 - 2/(2*2) = 0.5.
			expected: 0.5,
		},
		{
			name: "partial sharing - 3 functions, 4 vars",
			functions: []Function{
				{Name: "f1", Variables: []string{"a", "b"}},
				{Name: "f2", Variables: []string{"b", "c"}},
				{Name: "f3", Variables: []string{"c", "d"}},
			},
			// 4 unique vars: a(1), b(2), c(2), d(1). sumMA = 6.
			// LCOM = 1 - 6/(3*4) = 1 - 0.5 = 0.5.
			expected: 0.5,
		},
		{
			name: "no variables at all",
			functions: []Function{
				{Name: "f1", Variables: []string{}},
				{Name: "f2", Variables: []string{}},
			},
			expected: 0.0,
		},
	}

	for _, tc := range testCases {
		t.Run(tc.name, func(t *testing.T) {
			t.Parallel()

			result := analyzer.calculateLCOM(tc.functions)
			assert.InDelta(t, tc.expected, result, 0.001,
				"LCOM-HS mismatch for %s", tc.name)
		})
	}
}

// TestVariableSharingRatio verifies the function-level sharing ratio metric.
func TestVariableSharingRatio(t *testing.T) {
	t.Parallel()

	analyzer := NewAnalyzer()

	allFunctions := []Function{
		{Name: "f1", Variables: []string{"shared", "local1"}},
		{Name: "f2", Variables: []string{"shared", "local2"}},
		{Name: "f3", Variables: []string{"isolated"}},
	}

	testCases := []struct {
		name     string
		fn       Function
		expected float64
	}{
		{
			name: "f1 - one shared, one local",
			fn:   allFunctions[0],
			// "shared" is in f2, "local1" is not in f2 or f3.
			// shared=1, total=2, cohesion=0.5.
			expected: 0.5,
		},
		{
			name: "f2 - one shared, one local",
			fn:   allFunctions[1],
			// "shared" is in f1, "local2" is not elsewhere.
			// shared=1, total=2, cohesion=0.5.
			expected: 0.5,
		},
		{
			name: "f3 - completely isolated",
			fn:   allFunctions[2],
			// "isolated" is not in f1 or f2.
			// shared=0, total=1, cohesion=0.0.
			expected: 0.0,
		},
		{
			name: "function with no variables",
			fn:   Function{Name: "empty", Variables: []string{}},
			// No variables -> perfect cohesion (trivial function).
			expected: 1.0,
		},
	}

	for _, tc := range testCases {
		t.Run(tc.name, func(t *testing.T) {
			t.Parallel()

			result := analyzer.calculateFunctionLevelCohesion(tc.fn, allFunctions)
			assert.InDelta(t, tc.expected, result, 0.001,
				"Sharing ratio mismatch for %s", tc.name)
		})
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

	// Test function-level cohesion with no variables (trivial function).
	allFns := []Function{{Name: "f1", Variables: []string{}}}
	cohesion := analyzer.calculateFunctionLevelCohesion(allFns[0], allFns)
	assert.InDelta(t, 1.0, cohesion, 0.001, "Function with no variables should have perfect cohesion")

	// Test function-level cohesion where all variables are shared.
	allFns = []Function{
		{Name: "f1", Variables: []string{"a", "b"}},
		{Name: "f2", Variables: []string{"a", "b"}},
	}
	cohesion = analyzer.calculateFunctionLevelCohesion(allFns[0], allFns)
	assert.InDelta(t, 1.0, cohesion, 0.001, "Function with all shared variables should have perfect cohesion")
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

	// LCOM should be valid (0-1 range for LCOM-HS).
	if lcom, ok := report["lcom"].(float64); ok {
		assert.GreaterOrEqual(t, lcom, 0.0)
		assert.LessOrEqual(t, lcom, 1.0)
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
			"lcom":              float64(i) * 0.1,
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
		"lcom":              0.25,
		"cohesion_score":    0.75,
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
	assert.Contains(t, output, "lcom_variant:")
}

// --- Coverage for uncovered methods ---.

func TestAnalyzer_Flag(t *testing.T) {
	t.Parallel()

	analyzer := NewAnalyzer()
	assert.Equal(t, "cohesion-analysis", analyzer.Flag())
}

func TestAnalyzer_Description(t *testing.T) {
	t.Parallel()

	analyzer := NewAnalyzer()
	desc := analyzer.Description()
	assert.Contains(t, desc, "LCOM-HS")
}

func TestAnalyzer_Descriptor(t *testing.T) {
	t.Parallel()

	analyzer := NewAnalyzer()
	d := analyzer.Descriptor()
	assert.Equal(t, "static/cohesion", d.ID)
	assert.Contains(t, d.Description, "Henderson-Sellers")
	assert.Equal(t, analyze.ModeStatic, d.Mode)
}

func TestAnalyzer_ListConfigurationOptions(t *testing.T) {
	t.Parallel()

	analyzer := NewAnalyzer()
	opts := analyzer.ListConfigurationOptions()
	assert.Empty(t, opts)
}

func TestAnalyzer_Configure(t *testing.T) {
	t.Parallel()

	analyzer := NewAnalyzer()
	err := analyzer.Configure(map[string]any{"key": "value"})
	assert.NoError(t, err)
}

func TestAnalyzer_CreateVisitor(t *testing.T) {
	t.Parallel()

	analyzer := NewAnalyzer()
	visitor := analyzer.CreateVisitor()
	assert.NotNil(t, visitor)
}

func TestAnalyzer_FormatReportBinary(t *testing.T) {
	t.Parallel()

	analyzer := NewAnalyzer()

	report := analyze.Report{
		"total_functions":   2,
		"lcom":              0.2,
		"cohesion_score":    0.8,
		"function_cohesion": 0.7,
		"message":           "Good cohesion",
		"functions": []map[string]any{
			{"name": "fn1", "cohesion": 0.9},
		},
	}

	var buf bytes.Buffer

	err := analyzer.FormatReportBinary(report, &buf)
	require.NoError(t, err)
	assert.Positive(t, buf.Len())
}

func TestAnalyzer_GetCohesionAssessment(t *testing.T) {
	t.Parallel()

	analyzer := NewAnalyzer()

	tests := []struct {
		cohesion float64
		contains string
	}{
		{0.8, "Excellent"},
		{0.6, "Excellent"},
		{0.5, "Good"},
		{0.4, "Good"},
		{0.35, "Fair"},
		{0.3, "Fair"},
		{0.2, "Poor"},
		{0.0, "Poor"},
	}

	for _, tt := range tests {
		result := analyzer.getCohesionAssessment(tt.cohesion)
		assert.Contains(t, result, tt.contains, "cohesion=%.2f", tt.cohesion)
	}
}

func TestAnalyzer_GetVariableAssessment(t *testing.T) {
	t.Parallel()

	analyzer := NewAnalyzer()

	tests := []struct {
		count    int
		contains string
	}{
		{1, "Few"},
		{3, "Few"},
		{5, "Moderate"},
		{7, "Moderate"},
		{8, "Many"},
		{20, "Many"},
	}

	for _, tt := range tests {
		result := analyzer.getVariableAssessment(tt.count)
		assert.Contains(t, result, tt.contains, "count=%d", tt.count)
	}
}

func TestAnalyzer_GetSizeAssessment(t *testing.T) {
	t.Parallel()

	analyzer := NewAnalyzer()

	tests := []struct {
		lineCount int
		contains  string
	}{
		{5, "Small"},
		{10, "Small"},
		{15, "Medium"},
		{30, "Medium"},
		{31, "Large"},
		{100, "Large"},
	}

	for _, tt := range tests {
		result := analyzer.getSizeAssessment(tt.lineCount)
		assert.Contains(t, result, tt.contains, "lineCount=%d", tt.lineCount)
	}
}

func TestAnalyzer_GetCohesionMessage_AllBranches(t *testing.T) {
	t.Parallel()

	analyzer := NewAnalyzer()

	tests := []struct {
		score    float64
		contains string
	}{
		{0.8, "Excellent"},
		{0.7, "Excellent"},
		{0.5, "Good"},
		{0.4, "Good"},
		{0.35, "Fair"},
		{0.3, "Fair"},
		{0.2, "Poor"},
		{0.0, "Poor"},
	}

	for _, tt := range tests {
		result := analyzer.getCohesionMessage(tt.score)
		assert.Contains(t, result, tt.contains, "score=%.2f", tt.score)
	}
}

func TestAnalyzer_CreateReportSection(t *testing.T) {
	t.Parallel()

	analyzer := NewAnalyzer()

	report := analyze.Report{
		"total_functions":   2,
		"lcom":              0.3,
		"cohesion_score":    0.7,
		"function_cohesion": 0.6,
		"message":           "Test message",
	}

	section := analyzer.CreateReportSection(report)
	assert.NotNil(t, section)
}
