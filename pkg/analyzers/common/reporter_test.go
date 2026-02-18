package common

import (
	"encoding/json"
	"strings"
	"testing"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
)

func TestNewReporter(t *testing.T) {
	t.Parallel()

	config := ReportConfig{
		Format:         "text",
		IncludeDetails: true,
		SortBy:         "name",
		SortOrder:      "asc",
		MaxItems:       10,
	}

	reporter := NewReporter(config)

	if reporter == nil {
		t.Fatal("NewReporter returned nil")
	}

	if reporter.config.Format != "text" {
		t.Errorf("expected format 'text', got '%s'", reporter.config.Format)
	}

	if reporter.formatter == nil {
		t.Error("expected formatter to be initialized")
	}
}

func TestNewReporter_DefaultConfig(t *testing.T) {
	t.Parallel()

	config := ReportConfig{}
	reporter := NewReporter(config)

	if reporter == nil {
		t.Fatal("NewReporter returned nil for empty config")
	}
}

func TestReporter_GenerateReport_Text(t *testing.T) {
	t.Parallel()

	config := ReportConfig{
		Format: "text",
	}
	reporter := NewReporter(config)

	report := analyze.Report{
		"analyzer_name": testAnalyzerName,
		"message":       "Test completed",
		"score":         0.85,
	}

	result, err := reporter.GenerateReport(report)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if result == "" {
		t.Error("expected non-empty result")
	}
}

func TestReporter_GenerateReport_JSON(t *testing.T) {
	t.Parallel()

	config := ReportConfig{
		Format: "json",
	}
	reporter := NewReporter(config)

	report := analyze.Report{
		"analyzer_name": testAnalyzerName,
		"message":       "Test completed",
		"score":         0.85,
	}

	result, err := reporter.GenerateReport(report)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	// Verify it's valid JSON.
	var parsed map[string]any

	err = json.Unmarshal([]byte(result), &parsed)
	if err != nil {
		t.Errorf("result is not valid JSON: %v", err)
	}

	if parsed["analyzer_name"] != testAnalyzerName {
		t.Errorf("expected analyzer_name '%s', got '%v'", testAnalyzerName, parsed["analyzer_name"])
	}
}

func TestReporter_GenerateReport_Summary(t *testing.T) {
	t.Parallel()

	config := ReportConfig{
		Format: "summary",
	}
	reporter := NewReporter(config)

	report := analyze.Report{
		"analyzer_name": testAnalyzerName,
		"message":       "Test completed",
		"score":         0.85,
		"total_items":   10,
	}

	result, err := reporter.GenerateReport(report)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if !strings.Contains(result, testAnalyzerName) {
		t.Error("expected summary to contain analyzer name")
	}

	if !strings.Contains(result, "Test completed") {
		t.Error("expected summary to contain message")
	}
}

func TestReporter_GenerateReport_DefaultFormat(t *testing.T) {
	t.Parallel()

	config := ReportConfig{
		Format: "unknown",
	}
	reporter := NewReporter(config)

	report := analyze.Report{
		"message": "Test",
	}

	result, err := reporter.GenerateReport(report)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	// Should default to text format.
	if result == "" {
		t.Error("expected non-empty result for default format")
	}
}

func TestReporter_GenerateSummaryReport_NilReport(t *testing.T) {
	t.Parallel()

	config := ReportConfig{
		Format: "summary",
	}
	reporter := NewReporter(config)

	result, err := reporter.GenerateReport(nil)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if !strings.Contains(result, "No report data available") {
		t.Errorf("expected 'No report data available', got '%s'", result)
	}
}

func TestReporter_GenerateSummaryReport_WithMetricKeys(t *testing.T) {
	t.Parallel()

	config := ReportConfig{
		Format:     "summary",
		MetricKeys: []string{"score"},
		CountKeys:  []string{"total_items"},
	}
	reporter := NewReporter(config)

	report := analyze.Report{
		"analyzer_name": testAnalyzerName,
		"score":         0.85,
		"total_items":   10,
		"ignored_field": "should not appear",
	}

	result, err := reporter.GenerateReport(report)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if !strings.Contains(result, "score") {
		t.Error("expected summary to contain configured metric key")
	}
}

func TestReporter_GenerateComparisonReport_Empty(t *testing.T) {
	t.Parallel()

	config := ReportConfig{
		Format: "text",
	}
	reporter := NewReporter(config)

	reports := map[string]analyze.Report{}

	result, err := reporter.GenerateComparisonReport(reports)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if !strings.Contains(result, "No reports to compare") {
		t.Errorf("expected 'No reports to compare', got '%s'", result)
	}
}

func TestReporter_GenerateComparisonReport_Text(t *testing.T) {
	t.Parallel()

	config := ReportConfig{
		Format: "text",
	}
	reporter := NewReporter(config)

	reports := map[string]analyze.Report{
		"report1": {"score": 0.85, "count": 10},
		"report2": {"score": 0.92, "count": 15},
	}

	result, err := reporter.GenerateComparisonReport(reports)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if !strings.Contains(result, "COMPARISON REPORT") {
		t.Error("expected comparison report header")
	}

	if !strings.Contains(result, "report1") || !strings.Contains(result, "report2") {
		t.Error("expected both report names in comparison")
	}
}

func TestReporter_GenerateComparisonReport_JSON(t *testing.T) {
	t.Parallel()

	config := ReportConfig{
		Format: "json",
	}
	reporter := NewReporter(config)

	reports := map[string]analyze.Report{
		"report1": {"score": 0.85},
		"report2": {"score": 0.92},
	}

	result, err := reporter.GenerateComparisonReport(reports)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	// Verify it's valid JSON.
	var parsed map[string]any

	err = json.Unmarshal([]byte(result), &parsed)
	if err != nil {
		t.Errorf("result is not valid JSON: %v", err)
	}

	if _, ok := parsed["comparison"]; !ok {
		t.Error("expected 'comparison' key in JSON output")
	}

	if _, ok := parsed["reports"]; !ok {
		t.Error("expected 'reports' key in JSON output")
	}
}

func TestReporter_GenerateComparisonReport_Summary(t *testing.T) {
	t.Parallel()

	config := ReportConfig{
		Format: "summary",
	}
	reporter := NewReporter(config)

	reports := map[string]analyze.Report{
		"report1": {"analyzer_name": "analyzer1", "score": 0.85},
		"report2": {"analyzer_name": "analyzer2", "score": 0.92},
	}

	result, err := reporter.GenerateComparisonReport(reports)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if !strings.Contains(result, "Comparison Summary") {
		t.Error("expected comparison summary header")
	}
}

func TestReporter_GenerateComparisonReport_SingleReport(t *testing.T) {
	t.Parallel()

	config := ReportConfig{
		Format:     "text",
		MetricKeys: []string{"score"},
	}
	reporter := NewReporter(config)

	// Single report - comparison metrics should be empty.
	reports := map[string]analyze.Report{
		"report1": {"score": 0.85},
	}

	result, err := reporter.GenerateComparisonReport(reports)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if !strings.Contains(result, "report1") {
		t.Error("expected report name in output")
	}
}

func TestReporter_ToFloat(t *testing.T) {
	t.Parallel()

	reporter := NewReporter(ReportConfig{})

	tests := []struct {
		input    any
		name     string
		expected float64
		ok       bool
	}{
		{float64(1.5), "float64", 1.5, true},
		{int(10), "int", 10.0, true},
		{int32(20), "int32", 20.0, true},
		{int64(30), "int64", 30.0, true},
		{"not a number", "string", 0, false},
		{nil, "nil", 0, false},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			result, ok := reporter.toFloat(tt.input)
			if ok != tt.ok {
				t.Errorf("expected ok=%v, got ok=%v", tt.ok, ok)
			}

			if ok && result != tt.expected {
				t.Errorf("expected %v, got %v", tt.expected, result)
			}
		})
	}
}

func TestReporter_ToInt(t *testing.T) {
	t.Parallel()

	reporter := NewReporter(ReportConfig{})

	tests := []struct {
		input    any
		name     string
		expected int
		ok       bool
	}{
		{int(10), "int", 10, true},
		{int32(20), "int32", 20, true},
		{int64(30), "int64", 30, true},
		{float64(40.5), "float64", 40, true},
		{"not a number", "string", 0, false},
		{nil, "nil", 0, false},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			result, ok := reporter.toInt(tt.input)
			if ok != tt.ok {
				t.Errorf("expected ok=%v, got ok=%v", tt.ok, ok)
			}

			if ok && result != tt.expected {
				t.Errorf("expected %v, got %v", tt.expected, result)
			}
		})
	}
}

func TestReporter_ExtractKeyMetrics_WithConfiguredKeys(t *testing.T) {
	t.Parallel()

	config := ReportConfig{
		MetricKeys: []string{"score", "complexity"},
	}
	reporter := NewReporter(config)

	report := analyze.Report{
		"score":        0.85,
		"complexity":   15.0,
		"other_metric": 100.0,
		"string_field": "not a metric",
	}

	metrics := reporter.extractKeyMetrics(report)

	if len(metrics) != 2 {
		t.Errorf("expected 2 metrics, got %d", len(metrics))
	}

	if metrics["score"] != 0.85 {
		t.Errorf("expected score=0.85, got %v", metrics["score"])
	}

	if metrics["complexity"] != 15.0 {
		t.Errorf("expected complexity=15.0, got %v", metrics["complexity"])
	}

	if _, ok := metrics["other_metric"]; ok {
		t.Error("other_metric should not be included")
	}
}

func TestReporter_ExtractKeyMetrics_WithoutConfiguredKeys(t *testing.T) {
	t.Parallel()

	config := ReportConfig{}
	reporter := NewReporter(config)

	report := analyze.Report{
		"score":        0.85,
		"count":        10,
		"string_field": "not a metric",
	}

	metrics := reporter.extractKeyMetrics(report)

	// Should extract all numeric values.
	if _, ok := metrics["score"]; !ok {
		t.Error("expected score to be extracted")
	}

	if _, ok := metrics["count"]; !ok {
		t.Error("expected count to be extracted")
	}
}

func TestReporter_ExtractCounts_WithConfiguredKeys(t *testing.T) {
	t.Parallel()

	config := ReportConfig{
		CountKeys: []string{"total_items"},
	}
	reporter := NewReporter(config)

	report := analyze.Report{
		"total_items":  10,
		"other_count":  20,
		"string_field": "not a count",
	}

	counts := reporter.extractCounts(report)

	if len(counts) != 1 {
		t.Errorf("expected 1 count, got %d", len(counts))
	}

	if counts["total_items"] != 10 {
		t.Errorf("expected total_items=10, got %v", counts["total_items"])
	}
}

func TestReporter_CompareMetrics_WithConfiguredKeys(t *testing.T) {
	t.Parallel()

	config := ReportConfig{
		Format:     "text",
		MetricKeys: []string{"score"},
	}
	reporter := NewReporter(config)

	reports := map[string]analyze.Report{
		"report1": {"score": 0.85, "other": 1.0},
		"report2": {"score": 0.92, "other": 2.0},
	}

	result := reporter.compareMetrics(reports)

	if !strings.Contains(result, "score") {
		t.Error("expected comparison to include configured metric")
	}

	// Should only compare configured metrics.
	if strings.Contains(result, "other") {
		t.Error("should not compare non-configured metrics")
	}
}

func TestReporter_CompareMetricsData(t *testing.T) {
	t.Parallel()

	config := ReportConfig{
		MetricKeys: []string{"score"},
	}
	reporter := NewReporter(config)

	reports := map[string]analyze.Report{
		"report1": {"score": 0.85},
		"report2": {"score": 0.92},
	}

	data := reporter.compareMetricsData(reports)

	scoreData, ok := data["score"].(map[string]float64)
	if !ok {
		t.Fatal("expected score comparison data")
	}

	if scoreData["report1"] != 0.85 {
		t.Errorf("expected report1 score=0.85, got %v", scoreData["report1"])
	}

	if scoreData["report2"] != 0.92 {
		t.Errorf("expected report2 score=0.92, got %v", scoreData["report2"])
	}
}

func TestReporter_FormatMetricComparison_Empty(t *testing.T) {
	t.Parallel()

	reporter := NewReporter(ReportConfig{})

	result := reporter.formatMetricComparison("test", map[string]float64{})

	if result != "" {
		t.Errorf("expected empty string for empty values, got '%s'", result)
	}
}

func TestReporter_FormatMetricComparison_Sorted(t *testing.T) {
	t.Parallel()

	reporter := NewReporter(ReportConfig{})

	values := map[string]float64{
		"low":    0.5,
		"medium": 0.75,
		"high":   0.9,
	}

	result := reporter.formatMetricComparison("score", values)

	// Should be sorted by value descending.
	lines := strings.Split(result, "\n")
	if len(lines) < 4 {
		t.Fatalf("expected at least 4 lines, got %d", len(lines))
	}

	// First line is the metric name.
	if !strings.Contains(lines[0], "score") {
		t.Error("expected first line to contain metric name")
	}

	// Values should be sorted descending.
	if !strings.Contains(lines[1], "high") {
		t.Error("expected 'high' to be first (highest value)")
	}
}
