package complexity

import (
	"slices"
	"testing"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
)

func TestNewAggregator(t *testing.T) {
	t.Parallel()

	aggregator := NewAggregator()

	if aggregator == nil {
		t.Fatal("Expected non-nil aggregator")
	}

	if aggregator.Aggregator == nil {
		t.Error("Expected embedded Aggregator to be non-nil")
	}

	if aggregator.detailedFunctions == nil {
		t.Error("Expected detailedFunctions slice to be initialized")
	}
}

func TestAggregator_Aggregate_SingleReport(t *testing.T) {
	t.Parallel()

	aggregator := NewAggregator()

	report := analyze.Report{
		"total_functions":      2,
		"average_complexity":   3.5,
		"max_complexity":       5,
		"total_complexity":     7,
		"cognitive_complexity": 10,
		"nesting_depth":        3,
		"decision_points":      4,
		"functions": []map[string]any{
			{"name": "func1", "cyclomatic_complexity": 3},
			{"name": "func2", "cyclomatic_complexity": 4},
		},
	}

	aggregator.Aggregate(map[string]analyze.Report{"complexity": report})

	result := aggregator.GetResult()

	if result == nil {
		t.Fatal("Expected non-nil result")
	}

	if total, ok := result["total_functions"]; !ok || total != 2 {
		t.Errorf("Expected total_functions=2, got %v", total)
	}

	if functions, ok := result["functions"].([]map[string]any); ok {
		if len(functions) != 2 {
			t.Errorf("Expected 2 functions in result, got %d", len(functions))
		}
	}
}

func TestAggregator_Aggregate_MultipleReports(t *testing.T) {
	t.Parallel()

	aggregator := NewAggregator()

	report1 := analyze.Report{
		"total_functions":      2,
		"average_complexity":   2.0,
		"max_complexity":       3,
		"total_complexity":     4,
		"cognitive_complexity": 6,
		"nesting_depth":        2,
		"decision_points":      3,
		"functions": []map[string]any{
			{"name": "file1_func1", "cyclomatic_complexity": 2},
			{"name": "file1_func2", "cyclomatic_complexity": 2},
		},
	}

	report2 := analyze.Report{
		"total_functions":      3,
		"average_complexity":   4.0,
		"max_complexity":       6,
		"total_complexity":     12,
		"cognitive_complexity": 15,
		"nesting_depth":        4,
		"decision_points":      8,
		"functions": []map[string]any{
			{"name": "file2_func1", "cyclomatic_complexity": 3},
			{"name": "file2_func2", "cyclomatic_complexity": 4},
			{"name": "file2_func3", "cyclomatic_complexity": 5},
		},
	}

	aggregator.Aggregate(map[string]analyze.Report{"complexity": report1})
	aggregator.Aggregate(map[string]analyze.Report{"complexity": report2})

	result := aggregator.GetResult()

	if result == nil {
		t.Fatal("Expected non-nil result")
	}

	// Check aggregated totals.
	if total, ok := result["total_functions"]; !ok || total != 5 {
		t.Errorf("Expected total_functions=5, got %v", total)
	}

	// Max_complexity tracks the true max across files (max(3, 6) = 6).
	if maxComplexity, ok := result["max_complexity"]; !ok || maxComplexity != 6 {
		t.Errorf("Expected max_complexity=6, got %v", maxComplexity)
	}

	// Average_complexity is derived from total_complexity / total_functions = 16 / 5 = 3.2.
	if avgComplexity, ok := result["average_complexity"].(float64); !ok || avgComplexity != 3.2 {
		t.Errorf("Expected average_complexity=3.2, got %v", result["average_complexity"])
	}

	// Check detailed functions are collected.
	if functions, ok := result["functions"].([]map[string]any); ok {
		if len(functions) != 5 {
			t.Errorf("Expected 5 functions in result, got %d", len(functions))
		}
	}
}

func TestAggregator_Aggregate_EmptyReport(t *testing.T) {
	t.Parallel()

	aggregator := NewAggregator()

	// Aggregate with nil report.
	aggregator.Aggregate(map[string]analyze.Report{"complexity": nil})

	result := aggregator.GetResult()

	if result == nil {
		t.Fatal("Expected non-nil result")
	}

	// Should return empty result.
	if total, ok := result["total_functions"]; !ok || total != 0 {
		t.Errorf("Expected total_functions=0 for empty report, got %v", total)
	}
}

func TestAggregator_GetResult_NoAggregation(t *testing.T) {
	t.Parallel()

	aggregator := NewAggregator()

	result := aggregator.GetResult()

	if result == nil {
		t.Fatal("Expected non-nil result")
	}

	// Should return empty result.
	if total, ok := result["total_functions"]; !ok || total != 0 {
		t.Errorf("Expected total_functions=0, got %v", total)
	}

	if msg, ok := result["message"]; !ok || msg != "No functions found" {
		t.Errorf("Expected message='No functions found', got %v", msg)
	}
}

func TestAggregator_DetailedFunctionsCollection(t *testing.T) {
	t.Parallel()

	aggregator := NewAggregator()

	report := analyze.Report{
		"total_functions":    1,
		"average_complexity": 5.0,
		"max_complexity":     5,
		"total_complexity":   5,
		"functions": []map[string]any{
			{
				"name":                  "testFunc",
				"cyclomatic_complexity": 5,
				"cognitive_complexity":  8,
				"nesting_depth":         3,
			},
		},
	}

	aggregator.Aggregate(map[string]analyze.Report{"complexity": report})

	result := aggregator.GetResult()

	functions, ok := result["functions"].([]map[string]any)
	if !ok {
		t.Fatal("Expected functions to be []map[string]any")
	}

	if len(functions) != 1 {
		t.Fatalf("Expected 1 function, got %d", len(functions))
	}

	if name, nameOK := functions[0]["name"]; !nameOK || name != "testFunc" {
		t.Errorf("Expected function name='testFunc', got %v", name)
	}
}

func TestGetNumericKeys(t *testing.T) {
	t.Parallel()

	keys := getNumericKeys()

	// Average_complexity is a derived metric, not aggregated via numericKeys.
	expectedKeys := []string{"cognitive_complexity", "nesting_depth"}
	if len(keys) != len(expectedKeys) {
		t.Errorf("Expected %d numeric keys, got %d", len(expectedKeys), len(keys))
	}

	for _, expected := range expectedKeys {
		found := slices.Contains(keys, expected)

		if !found {
			t.Errorf("Expected numeric key '%s' not found", expected)
		}
	}
}

func TestGetCountKeys(t *testing.T) {
	t.Parallel()

	keys := getCountKeys()

	// Max_complexity is tracked separately (true max, not sum).
	expectedKeys := []string{"total_functions", "total_complexity", "decision_points"}
	if len(keys) != len(expectedKeys) {
		t.Errorf("Expected %d count keys, got %d", len(expectedKeys), len(keys))
	}

	for _, expected := range expectedKeys {
		found := slices.Contains(keys, expected)

		if !found {
			t.Errorf("Expected count key '%s' not found", expected)
		}
	}
}

func TestBuildComplexityMessage(t *testing.T) {
	t.Parallel()

	tests := []struct {
		expected string
		score    float64
	}{
		{"Excellent complexity - functions are simple and maintainable", 0.5},
		{"Excellent complexity - functions are simple and maintainable", 1.0},
		{"Good complexity - functions have reasonable complexity", 2.0},
		{"Good complexity - functions have reasonable complexity", 3.0},
		{"Fair complexity - some functions could be simplified", 5.0},
		{"Fair complexity - some functions could be simplified", 7.0},
		{"High complexity - functions are complex and should be refactored", 10.0},
		{"High complexity - functions are complex and should be refactored", 15.0},
	}

	for _, tt := range tests {
		result := buildComplexityMessage(tt.score)
		if result != tt.expected {
			t.Errorf("buildComplexityMessage(%v) = %q, expected %q", tt.score, result, tt.expected)
		}
	}
}

func TestBuildEmptyComplexityResult(t *testing.T) {
	t.Parallel()

	result := buildEmptyComplexityResult()

	if result == nil {
		t.Fatal("Expected non-nil result")
	}

	expectedFields := []string{"total_functions", "average_complexity", "max_complexity", "total_complexity", "message"}
	for _, field := range expectedFields {
		if _, ok := result[field]; !ok {
			t.Errorf("Expected field '%s' in empty result", field)
		}
	}

	if result["total_functions"] != 0 {
		t.Errorf("Expected total_functions=0, got %v", result["total_functions"])
	}

	if result["message"] != "No functions found" {
		t.Errorf("Expected message='No functions found', got %v", result["message"])
	}
}

func TestAggregator_ExtractFunctionsFromReport(t *testing.T) {
	t.Parallel()

	aggregator := NewAggregator()

	// Test with valid functions.
	report := analyze.Report{
		"functions": []map[string]any{
			{"name": "func1"},
			{"name": "func2"},
		},
	}

	aggregator.extractFunctionsFromReport(report)

	if len(aggregator.detailedFunctions) != 2 {
		t.Errorf("Expected 2 functions extracted, got %d", len(aggregator.detailedFunctions))
	}

	// Test with no functions key.
	aggregator2 := NewAggregator()
	reportNoFunctions := analyze.Report{
		"total_functions": 0,
	}

	aggregator2.extractFunctionsFromReport(reportNoFunctions)

	if len(aggregator2.detailedFunctions) != 0 {
		t.Errorf("Expected 0 functions extracted, got %d", len(aggregator2.detailedFunctions))
	}
}

func TestAggregator_CollectDetailedFunctions(t *testing.T) {
	t.Parallel()

	aggregator := NewAggregator()

	results := map[string]analyze.Report{
		"file1": {
			"functions": []map[string]any{
				{"name": "file1_func1"},
			},
		},
		"file2": {
			"functions": []map[string]any{
				{"name": "file2_func1"},
				{"name": "file2_func2"},
			},
		},
		"file3": nil, // Nil report should be skipped.
	}

	aggregator.collectDetailedFunctions(results)

	if len(aggregator.detailedFunctions) != 3 {
		t.Errorf("Expected 3 functions collected, got %d", len(aggregator.detailedFunctions))
	}
}

func TestAggregator_AddDetailedFunctionsToResult(t *testing.T) {
	t.Parallel()

	aggregator := NewAggregator()
	aggregator.detailedFunctions = []map[string]any{
		{"name": "func1"},
		{"name": "func2"},
	}

	result := analyze.Report{}
	aggregator.addDetailedFunctionsToResult(result)

	functions, fnOK := result["functions"].([]map[string]any)
	if !fnOK {
		t.Fatal("Expected functions to be added to result")
	}

	if len(functions) != 2 {
		t.Errorf("Expected 2 functions in result, got %d", len(functions))
	}

	// Test with empty detailed functions.
	aggregator2 := NewAggregator()
	result2 := analyze.Report{}
	aggregator2.addDetailedFunctionsToResult(result2)

	if _, hasFunc := result2["functions"]; hasFunc {
		t.Error("Expected no functions key when detailedFunctions is empty")
	}
}
