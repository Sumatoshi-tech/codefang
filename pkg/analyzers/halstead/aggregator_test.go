package halstead //nolint:testpackage // testing internal implementation.

import (
	"slices"
	"testing"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
)

func TestNewHalsteadAggregator(t *testing.T) {
	t.Parallel()

	aggregator := NewHalsteadAggregator()

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

func TestHalsteadAggregator_Aggregate_SingleReport(t *testing.T) {
	t.Parallel()

	aggregator := NewHalsteadAggregator()

	report := analyze.Report{
		"total_functions":    2,
		"volume":             150.5,
		"difficulty":         10.2,
		"effort":             1535.1,
		"time_to_program":    85.28,
		"delivered_bugs":     0.05,
		"distinct_operators": 8,
		"distinct_operands":  12,
		"total_operators":    25,
		"total_operands":     30,
		"vocabulary":         20,
		"length":             55,
		"estimated_length":   52.3,
		"functions": []map[string]any{
			{"name": "func1", "volume": 75.0},
			{"name": "func2", "volume": 75.5},
		},
	}

	aggregator.Aggregate(map[string]analyze.Report{"halstead": report})

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

func TestHalsteadAggregator_Aggregate_MultipleReports(t *testing.T) {
	t.Parallel()

	aggregator := NewHalsteadAggregator()

	report1 := analyze.Report{
		"total_functions":    2,
		"volume":             100.0,
		"difficulty":         5.0,
		"effort":             500.0,
		"time_to_program":    27.78,
		"delivered_bugs":     0.033,
		"distinct_operators": 5,
		"distinct_operands":  10,
		"total_operators":    15,
		"total_operands":     20,
		"vocabulary":         15,
		"length":             35,
		"estimated_length":   33.0,
		"functions": []map[string]any{
			{"name": "file1_func1", "volume": 50.0},
			{"name": "file1_func2", "volume": 50.0},
		},
	}

	report2 := analyze.Report{
		"total_functions":    3,
		"volume":             200.0,
		"difficulty":         8.0,
		"effort":             1600.0,
		"time_to_program":    88.89,
		"delivered_bugs":     0.067,
		"distinct_operators": 7,
		"distinct_operands":  15,
		"total_operators":    25,
		"total_operands":     35,
		"vocabulary":         22,
		"length":             60,
		"estimated_length":   58.0,
		"functions": []map[string]any{
			{"name": "file2_func1", "volume": 60.0},
			{"name": "file2_func2", "volume": 70.0},
			{"name": "file2_func3", "volume": 70.0},
		},
	}

	aggregator.Aggregate(map[string]analyze.Report{"halstead": report1})
	aggregator.Aggregate(map[string]analyze.Report{"halstead": report2})

	result := aggregator.GetResult()

	if result == nil {
		t.Fatal("Expected non-nil result")
	}

	// Check aggregated totals.
	if total, ok := result["total_functions"]; !ok || total != 5 {
		t.Errorf("Expected total_functions=5, got %v", total)
	}

	// Check detailed functions are collected.
	if functions, ok := result["functions"].([]map[string]any); ok {
		if len(functions) != 5 {
			t.Errorf("Expected 5 functions in result, got %d", len(functions))
		}
	}
}

func TestHalsteadAggregator_Aggregate_EmptyReport(t *testing.T) {
	t.Parallel()

	aggregator := NewHalsteadAggregator()

	// Aggregate with nil report.
	aggregator.Aggregate(map[string]analyze.Report{"halstead": nil})

	result := aggregator.GetResult()

	if result == nil {
		t.Fatal("Expected non-nil result")
	}

	// Should return empty result.
	if total, ok := result["total_functions"]; !ok || total != 0 {
		t.Errorf("Expected total_functions=0 for empty report, got %v", total)
	}
}

func TestHalsteadAggregator_GetResult_NoAggregation(t *testing.T) {
	t.Parallel()

	aggregator := NewHalsteadAggregator()

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

func TestHalsteadAggregator_DetailedFunctionsCollection(t *testing.T) {
	t.Parallel()

	aggregator := NewHalsteadAggregator()

	report := analyze.Report{
		"total_functions": 1,
		"volume":          100.0,
		"difficulty":      5.0,
		"effort":          500.0,
		"functions": []map[string]any{
			{
				"name":       "testFunc",
				"volume":     100.0,
				"difficulty": 5.0,
				"effort":     500.0,
			},
		},
	}

	aggregator.Aggregate(map[string]analyze.Report{"halstead": report})

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

func TestHalsteadGetNumericKeys(t *testing.T) {
	t.Parallel()

	keys := getNumericKeys()

	expectedKeys := []string{
		"volume", "difficulty", "effort", "time_to_program", "delivered_bugs",
		"distinct_operators", "distinct_operands", "total_operators", "total_operands",
		"vocabulary", "length", "estimated_length",
	}

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

func TestHalsteadGetCountKeys(t *testing.T) {
	t.Parallel()

	keys := getCountKeys()

	expectedKeys := []string{"total_functions"}
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

func TestBuildHalsteadMessage(t *testing.T) {
	t.Parallel()

	tests := []struct {
		expected string
		volume   float64
	}{
		{"Low Halstead complexity - well-structured code", 50.0},
		{"Low Halstead complexity - well-structured code", 99.0},
		{"Moderate Halstead complexity - acceptable", 100.0},
		{"Moderate Halstead complexity - acceptable", 500.0},
		{"Moderate Halstead complexity - acceptable", 999.0},
		{"High Halstead complexity - consider refactoring", 1000.0},
		{"High Halstead complexity - consider refactoring", 3000.0},
		{"High Halstead complexity - consider refactoring", 4999.0},
		{"Very high Halstead complexity - significant refactoring recommended", 5000.0},
		{"Very high Halstead complexity - significant refactoring recommended", 10000.0},
	}

	for _, tt := range tests {
		result := buildHalsteadMessage(tt.volume)
		if result != tt.expected {
			t.Errorf("buildHalsteadMessage(%v) = %q, expected %q", tt.volume, result, tt.expected)
		}
	}
}

func TestBuildEmptyHalsteadResult(t *testing.T) {
	t.Parallel()

	result := buildEmptyHalsteadResult()

	if result == nil {
		t.Fatal("Expected non-nil result")
	}

	expectedFields := []string{
		"total_functions", "volume", "difficulty", "effort", "time_to_program",
		"delivered_bugs", "distinct_operators", "distinct_operands",
		"total_operators", "total_operands", "vocabulary", "length",
		"estimated_length", "message",
	}

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

func TestHalsteadAggregator_ExtractFunctionsFromReport(t *testing.T) {
	t.Parallel()

	aggregator := NewHalsteadAggregator()

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
	aggregator2 := NewHalsteadAggregator()
	reportNoFunctions := analyze.Report{
		"total_functions": 0,
	}

	aggregator2.extractFunctionsFromReport(reportNoFunctions)

	if len(aggregator2.detailedFunctions) != 0 {
		t.Errorf("Expected 0 functions extracted, got %d", len(aggregator2.detailedFunctions))
	}
}

func TestHalsteadAggregator_CollectDetailedFunctions(t *testing.T) {
	t.Parallel()

	aggregator := NewHalsteadAggregator()

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

func TestHalsteadAggregator_AddDetailedFunctionsToResult(t *testing.T) {
	t.Parallel()

	aggregator := NewHalsteadAggregator()
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
	aggregator2 := NewHalsteadAggregator()
	result2 := analyze.Report{}
	aggregator2.addDetailedFunctionsToResult(result2)

	if _, hasFunc := result2["functions"]; hasFunc {
		t.Error("Expected no functions key when detailedFunctions is empty")
	}
}
