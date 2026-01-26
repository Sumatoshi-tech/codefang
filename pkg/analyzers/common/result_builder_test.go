package common //nolint:testpackage // testing internal implementation.

import (
	"testing"
)

func TestNewResultBuilder(t *testing.T) {
	t.Parallel()

	builder := NewResultBuilder()
	if builder == nil {
		t.Fatal("NewResultBuilder returned nil")
	}
}

func TestResultBuilder_BuildEmptyResult(t *testing.T) {
	t.Parallel()

	builder := NewResultBuilder()
	result := builder.BuildEmptyResult("test_analyzer")

	if result["analyzer_name"] != "test_analyzer" {
		t.Errorf("expected analyzer_name 'test_analyzer', got '%v'", result["analyzer_name"])
	}

	if result["total_items"] != 0 {
		t.Errorf("expected total_items 0, got '%v'", result["total_items"])
	}

	if result["message"] != "No data found" {
		t.Errorf("expected message 'No data found', got '%v'", result["message"])
	}
}

func TestResultBuilder_BuildCustomEmptyResult(t *testing.T) {
	t.Parallel()

	builder := NewResultBuilder()
	fields := map[string]any{
		"custom_field": "custom_value",
		"count":        42,
	}
	result := builder.BuildCustomEmptyResult(fields)

	if result["custom_field"] != "custom_value" {
		t.Errorf("expected custom_field 'custom_value', got '%v'", result["custom_field"])
	}

	if result["count"] != 42 {
		t.Errorf("expected count 42, got '%v'", result["count"])
	}
}

func TestResultBuilder_BuildBasicResult(t *testing.T) {
	t.Parallel()

	builder := NewResultBuilder()
	result := builder.BuildBasicResult("test_analyzer", 10, "Test message")

	if result["analyzer_name"] != "test_analyzer" {
		t.Errorf("expected analyzer_name 'test_analyzer', got '%v'", result["analyzer_name"])
	}

	if result["total_items"] != 10 {
		t.Errorf("expected total_items 10, got '%v'", result["total_items"])
	}

	if result["message"] != "Test message" {
		t.Errorf("expected message 'Test message', got '%v'", result["message"])
	}
}

func TestResultBuilder_BuildDetailedResult(t *testing.T) {
	t.Parallel()

	builder := NewResultBuilder()
	fields := map[string]any{
		"score":      0.85,
		"complexity": 15,
	}
	result := builder.BuildDetailedResult("test_analyzer", fields)

	if result["analyzer_name"] != "test_analyzer" {
		t.Errorf("expected analyzer_name 'test_analyzer', got '%v'", result["analyzer_name"])
	}

	if result["score"] != 0.85 {
		t.Errorf("expected score 0.85, got '%v'", result["score"])
	}

	if result["complexity"] != 15 {
		t.Errorf("expected complexity 15, got '%v'", result["complexity"])
	}
}

func TestResultBuilder_BuildCollectionResult(t *testing.T) {
	t.Parallel()

	builder := NewResultBuilder()
	items := []map[string]any{
		{"name": "item1", "value": 100},
		{"name": "item2", "value": 200},
	}
	metrics := map[string]any{
		"average": 150.0,
	}
	result := builder.BuildCollectionResult("test_analyzer", "items", items, metrics, "Collection built")

	if result["analyzer_name"] != "test_analyzer" {
		t.Errorf("expected analyzer_name 'test_analyzer', got '%v'", result["analyzer_name"])
	}

	if result["total_items"] != 2 {
		t.Errorf("expected total_items 2, got '%v'", result["total_items"])
	}

	if result["message"] != "Collection built" {
		t.Errorf("expected message 'Collection built', got '%v'", result["message"])
	}

	if result["average"] != 150.0 {
		t.Errorf("expected average 150.0, got '%v'", result["average"])
	}

	resultItems, ok := result["items"].([]map[string]any)
	if !ok || len(resultItems) != 2 {
		t.Error("expected items to be a slice of 2 maps")
	}
}

func TestResultBuilder_BuildMetricResult(t *testing.T) {
	t.Parallel()

	builder := NewResultBuilder()
	metrics := map[string]any{
		"lines_of_code":        1000,
		"cyclomatic":           15,
		"cognitive_complexity": 20,
	}
	result := builder.BuildMetricResult("test_analyzer", metrics, "Metrics computed")

	if result["analyzer_name"] != "test_analyzer" {
		t.Errorf("expected analyzer_name 'test_analyzer', got '%v'", result["analyzer_name"])
	}

	if result["message"] != "Metrics computed" {
		t.Errorf("expected message 'Metrics computed', got '%v'", result["message"])
	}

	if result["lines_of_code"] != 1000 {
		t.Errorf("expected lines_of_code 1000, got '%v'", result["lines_of_code"])
	}

	if result["cyclomatic"] != 15 {
		t.Errorf("expected cyclomatic 15, got '%v'", result["cyclomatic"])
	}

	if result["cognitive_complexity"] != 20 {
		t.Errorf("expected cognitive_complexity 20, got '%v'", result["cognitive_complexity"])
	}
}
