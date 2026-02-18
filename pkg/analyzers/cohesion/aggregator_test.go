package cohesion

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
}

func TestAggregatorConfig(t *testing.T) {
	t.Parallel()

	config := buildAggregatorConfig()

	// Verify numeric keys.
	expectedNumericKeys := []string{"lcom", "cohesion_score", "function_cohesion"}
	if len(config.numericKeys) != len(expectedNumericKeys) {
		t.Errorf("Expected %d numeric keys, got %d", len(expectedNumericKeys), len(config.numericKeys))
	}

	for _, expected := range expectedNumericKeys {
		found := slices.Contains(config.numericKeys, expected)

		if !found {
			t.Errorf("Expected numeric key '%s' not found", expected)
		}
	}

	// Verify count keys.
	expectedCountKeys := []string{"total_functions"}
	if len(config.countKeys) != len(expectedCountKeys) {
		t.Errorf("Expected %d count keys, got %d", len(expectedCountKeys), len(config.countKeys))
	}

	// Verify message builder exists.
	if config.messageBuilder == nil {
		t.Error("Expected messageBuilder to be non-nil")
	}

	// Verify empty result builder exists.
	if config.emptyResultBuilder == nil {
		t.Error("Expected emptyResultBuilder to be non-nil")
	}
}

func TestCohesionGetNumericKeys(t *testing.T) {
	t.Parallel()

	keys := getNumericKeys()

	expectedKeys := []string{"lcom", "cohesion_score", "function_cohesion"}
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

func TestCohesionGetCountKeys(t *testing.T) {
	t.Parallel()

	keys := getCountKeys()

	expectedKeys := []string{"total_functions"}
	if len(keys) != len(expectedKeys) {
		t.Errorf("Expected %d count keys, got %d", len(expectedKeys), len(keys))
	}

	if keys[0] != "total_functions" {
		t.Errorf("Expected 'total_functions', got '%s'", keys[0])
	}
}

func TestBuildMessageBuilder(t *testing.T) {
	t.Parallel()

	messageBuilder := buildMessageBuilder()

	if messageBuilder == nil {
		t.Fatal("Expected non-nil messageBuilder")
	}

	// Test the message builder function.
	message := messageBuilder(0.9)
	if message == "" {
		t.Error("Expected non-empty message")
	}
}

func TestCohesionGetCohesionMessage(t *testing.T) {
	t.Parallel()

	tests := []struct {
		contains string
		score    float64
	}{
		{"Excellent", 0.9},
		{"Excellent", 0.8},
		{"Good", 0.7},
		{"Good", 0.6},
		{"Fair", 0.5},
		{"Fair", 0.3},
		{"Poor", 0.2},
		{"Poor", 0.1},
	}

	for _, tt := range tests {
		message := getCohesionMessage(tt.score)
		if message == "" {
			t.Errorf("getCohesionMessage(%v) returned empty string", tt.score)
		}
		// Just verify the function returns a message.
		if len(message) < 10 {
			t.Errorf("getCohesionMessage(%v) returned too short message: %s", tt.score, message)
		}
	}
}

func TestBuildEmptyResultBuilder(t *testing.T) {
	t.Parallel()

	emptyResultBuilder := buildEmptyResultBuilder()

	if emptyResultBuilder == nil {
		t.Fatal("Expected non-nil emptyResultBuilder")
	}

	result := emptyResultBuilder()
	if result == nil {
		t.Fatal("Expected non-nil result from emptyResultBuilder")
	}
}

func TestCreateEmptyResult(t *testing.T) {
	t.Parallel()

	result := createEmptyResult()

	if result == nil {
		t.Fatal("Expected non-nil result")
	}

	expectedFields := []string{
		"total_functions", "lcom", "cohesion_score",
		"function_cohesion", "message",
	}

	for _, field := range expectedFields {
		if _, ok := result[field]; !ok {
			t.Errorf("Expected field '%s' in empty result", field)
		}
	}

	if result["total_functions"] != 0 {
		t.Errorf("Expected total_functions=0, got %v", result["total_functions"])
	}

	if result["cohesion_score"] != 1.0 {
		t.Errorf("Expected cohesion_score=1.0, got %v", result["cohesion_score"])
	}

	if result["message"] != "No functions found" {
		t.Errorf("Expected message='No functions found', got %v", result["message"])
	}
}

func TestAggregator_Aggregate_WithNilReport(t *testing.T) {
	t.Parallel()

	aggregator := NewAggregator()

	// Aggregate with nil report should not panic.
	aggregator.Aggregate(map[string]analyze.Report{"cohesion": nil})

	result := aggregator.GetResult()

	if result == nil {
		t.Fatal("Expected non-nil result")
	}

	// Should return empty result values.
	if total, ok := result["total_functions"]; !ok || total != 0 {
		t.Errorf("Expected total_functions=0 for nil report, got %v", total)
	}
}

func TestAggregator_MultipleAggregations(t *testing.T) {
	t.Parallel()

	aggregator := NewAggregator()

	report1 := analyze.Report{
		"total_functions":   2,
		"lcom":              1.0,
		"cohesion_score":    0.8,
		"function_cohesion": 0.75,
	}

	report2 := analyze.Report{
		"total_functions":   3,
		"lcom":              2.0,
		"cohesion_score":    0.6,
		"function_cohesion": 0.65,
	}

	aggregator.Aggregate(map[string]analyze.Report{"file1": report1})
	aggregator.Aggregate(map[string]analyze.Report{"file2": report2})

	result := aggregator.GetResult()

	// Check total functions are summed.
	if total, ok := result["total_functions"]; !ok || total != 5 {
		t.Errorf("Expected total_functions=5, got %v", total)
	}
}
