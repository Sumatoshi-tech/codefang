package common

import (
	"testing"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
)

func TestNewAggregator(t *testing.T) {
	messageBuilder := func(score float64) string {
		return "Test message"
	}

	emptyResultBuilder := func() analyze.Report {
		return analyze.Report{
			"custom_empty": true,
		}
	}

	aggregator := NewAggregator(
		"test_analyzer",
		[]string{"score", "complexity"},
		[]string{"total_items"},
		"items",
		"name",
		messageBuilder,
		emptyResultBuilder,
	)

	if aggregator == nil {
		t.Fatal("NewAggregator returned nil")
	}

	if aggregator.analyzerName != "test_analyzer" {
		t.Errorf("expected analyzer name 'test_analyzer', got '%s'", aggregator.analyzerName)
	}

	if aggregator.metricsProcessor == nil {
		t.Error("expected metricsProcessor to be initialized")
	}

	if aggregator.dataCollector == nil {
		t.Error("expected dataCollector to be initialized")
	}

	if aggregator.resultBuilder == nil {
		t.Error("expected resultBuilder to be initialized")
	}

	if aggregator.messageBuilder == nil {
		t.Error("expected messageBuilder to be initialized")
	}

	if aggregator.emptyResultBuilder == nil {
		t.Error("expected emptyResultBuilder to be initialized")
	}

	// Verify empty result builder works
	result := aggregator.GetResult()
	if result["custom_empty"] != true {
		t.Error("expected custom empty result")
	}
}

func TestNewAggregator_NilEmptyResultBuilder(t *testing.T) {
	aggregator := NewAggregator(
		"test_analyzer",
		[]string{"score"},
		[]string{"count"},
		"items",
		"name",
		nil,
		nil, // nil emptyResultBuilder should use default
	)

	if aggregator == nil {
		t.Fatal("NewAggregator returned nil")
	}

	// With nil emptyResultBuilder, should use default empty result
	result := aggregator.GetResult()
	if result["analyzer_name"] != "test_analyzer" {
		t.Errorf("expected analyzer_name 'test_analyzer', got '%v'", result["analyzer_name"])
	}
}

func TestAggregator_Aggregate_SingleReport(t *testing.T) {
	aggregator := NewAggregator(
		"test_analyzer",
		[]string{"score"},
		[]string{"count"},
		"items",
		"name",
		nil,
		nil,
	)

	reports := map[string]analyze.Report{
		"file1": {
			"score": 0.85,
			"count": 10,
			"items": []map[string]interface{}{
				{"name": "item1", "value": 100},
			},
		},
	}

	aggregator.Aggregate(reports)

	if aggregator.metricsProcessor.GetReportCount() != 1 {
		t.Errorf("expected 1 report processed, got %d", aggregator.metricsProcessor.GetReportCount())
	}
}

func TestAggregator_Aggregate_MultipleReports(t *testing.T) {
	aggregator := NewAggregator(
		"test_analyzer",
		[]string{"score"},
		[]string{"count"},
		"items",
		"name",
		nil,
		nil,
	)

	reports := map[string]analyze.Report{
		"file1": {
			"score": 0.80,
			"count": 10,
		},
		"file2": {
			"score": 0.90,
			"count": 20,
		},
	}

	aggregator.Aggregate(reports)

	if aggregator.metricsProcessor.GetReportCount() != 2 {
		t.Errorf("expected 2 reports processed, got %d", aggregator.metricsProcessor.GetReportCount())
	}
}

func TestAggregator_Aggregate_NilReports(t *testing.T) {
	aggregator := NewAggregator(
		"test_analyzer",
		[]string{"score"},
		[]string{"count"},
		"items",
		"name",
		nil,
		nil,
	)

	reports := map[string]analyze.Report{
		"file1": nil,
		"file2": {"score": 0.85},
	}

	aggregator.Aggregate(reports)

	// Only non-nil report should be processed
	if aggregator.metricsProcessor.GetReportCount() != 1 {
		t.Errorf("expected 1 report processed (nil skipped), got %d", aggregator.metricsProcessor.GetReportCount())
	}
}

func TestAggregator_GetResult_Empty(t *testing.T) {
	aggregator := NewAggregator(
		"test_analyzer",
		[]string{"score"},
		[]string{"count"},
		"items",
		"name",
		nil,
		nil,
	)

	result := aggregator.GetResult()

	if result == nil {
		t.Fatal("expected non-nil result")
	}

	// Should return empty result from resultBuilder
	if result["analyzer_name"] != "test_analyzer" {
		t.Errorf("expected analyzer_name 'test_analyzer', got '%v'", result["analyzer_name"])
	}
}

func TestAggregator_GetResult_WithCustomEmptyBuilder(t *testing.T) {
	emptyResultBuilder := func() analyze.Report {
		return analyze.Report{
			"custom_field": "custom_value",
		}
	}

	aggregator := NewAggregator(
		"test_analyzer",
		[]string{"score"},
		[]string{"count"},
		"items",
		"name",
		nil,
		emptyResultBuilder,
	)

	result := aggregator.GetResult()

	if result["custom_field"] != "custom_value" {
		t.Errorf("expected custom empty result, got %v", result)
	}
}

func TestAggregator_GetResult_WithData(t *testing.T) {
	messageBuilder := func(score float64) string {
		if score >= 0.8 {
			return "Good score"
		}
		return "Needs improvement"
	}

	aggregator := NewAggregator(
		"test_analyzer",
		[]string{"score"},
		[]string{"count"},
		"items",
		"name",
		messageBuilder,
		nil,
	)

	reports := map[string]analyze.Report{
		"file1": {
			"score": 0.85,
			"count": 10,
			"items": []map[string]interface{}{
				{"name": "item1"},
			},
		},
	}

	aggregator.Aggregate(reports)
	result := aggregator.GetResult()

	if result == nil {
		t.Fatal("expected non-nil result")
	}

	if result["analyzer_name"] != "test_analyzer" {
		t.Errorf("expected analyzer_name 'test_analyzer', got '%v'", result["analyzer_name"])
	}

	// Check message was built
	if msg, ok := result["message"].(string); !ok || msg == "" {
		t.Error("expected message to be set")
	}
}

func TestAggregator_GetResult_WithoutMessageBuilder(t *testing.T) {
	aggregator := NewAggregator(
		"test_analyzer",
		[]string{"score"},
		[]string{"count"},
		"items",
		"name",
		nil, // No message builder
		nil,
	)

	reports := map[string]analyze.Report{
		"file1": {"score": 0.85},
	}

	aggregator.Aggregate(reports)
	result := aggregator.GetResult()

	// Should default to "Analysis completed"
	if msg, ok := result["message"].(string); !ok || msg != "Analysis completed" {
		t.Errorf("expected default message 'Analysis completed', got '%v'", result["message"])
	}
}

func TestAggregator_GetMetricsProcessor(t *testing.T) {
	aggregator := NewAggregator(
		"test_analyzer",
		[]string{"score"},
		[]string{"count"},
		"items",
		"name",
		nil,
		nil,
	)

	processor := aggregator.GetMetricsProcessor()

	if processor == nil {
		t.Error("expected non-nil metrics processor")
	}
}

func TestAggregator_GetDataCollector(t *testing.T) {
	aggregator := NewAggregator(
		"test_analyzer",
		[]string{"score"},
		[]string{"count"},
		"items",
		"name",
		nil,
		nil,
	)

	collector := aggregator.GetDataCollector()

	if collector == nil {
		t.Error("expected non-nil data collector")
	}
}

func TestAggregator_GetResultBuilder(t *testing.T) {
	aggregator := NewAggregator(
		"test_analyzer",
		[]string{"score"},
		[]string{"count"},
		"items",
		"name",
		nil,
		nil,
	)

	builder := aggregator.GetResultBuilder()

	if builder == nil {
		t.Error("expected non-nil result builder")
	}
}

func TestAggregator_FullWorkflow(t *testing.T) {
	messageBuilder := func(score float64) string {
		if score >= 0.8 {
			return "Excellent"
		}
		return "Good"
	}

	aggregator := NewAggregator(
		"complexity_analyzer",
		[]string{"average_complexity"},
		[]string{"total_functions"},
		"functions",
		"function_name",
		messageBuilder,
		nil,
	)

	// Simulate aggregating results from multiple files
	reports := map[string]analyze.Report{
		"file1.go": {
			"average_complexity": 5.0,
			"total_functions":    10,
			"functions": []map[string]interface{}{
				{"function_name": "func1", "complexity": 3},
				{"function_name": "func2", "complexity": 7},
			},
		},
		"file2.go": {
			"average_complexity": 8.0,
			"total_functions":    5,
			"functions": []map[string]interface{}{
				{"function_name": "func3", "complexity": 8},
			},
		},
	}

	aggregator.Aggregate(reports)
	result := aggregator.GetResult()

	if result == nil {
		t.Fatal("expected non-nil result")
	}

	if result["analyzer_name"] != "complexity_analyzer" {
		t.Errorf("expected analyzer_name 'complexity_analyzer', got '%v'", result["analyzer_name"])
	}

	// Verify report count
	if aggregator.GetMetricsProcessor().GetReportCount() != 2 {
		t.Errorf("expected 2 reports processed, got %d", aggregator.GetMetricsProcessor().GetReportCount())
	}
}
