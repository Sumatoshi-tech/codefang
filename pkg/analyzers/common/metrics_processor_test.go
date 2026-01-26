package common //nolint:testpackage // testing internal implementation.

import (
	"testing"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
)

func TestNewMetricsProcessor(t *testing.T) {
	t.Parallel()

	processor := NewMetricsProcessor([]string{"score", "avg"}, []string{"count", "total"})
	if processor == nil {
		t.Fatal("NewMetricsProcessor returned nil")
	}

	if len(processor.numericKeys) != 2 {
		t.Errorf("expected 2 numeric keys, got %d", len(processor.numericKeys))
	}

	if len(processor.countKeys) != 2 {
		t.Errorf("expected 2 count keys, got %d", len(processor.countKeys))
	}
}

func TestMetricsProcessor_ProcessReport_NilReport(t *testing.T) {
	t.Parallel()

	processor := NewMetricsProcessor([]string{"score"}, []string{"count"})
	processor.ProcessReport(nil)

	if processor.GetReportCount() != 0 {
		t.Error("expected 0 reports for nil input")
	}
}

func TestMetricsProcessor_ProcessReport_Numeric(t *testing.T) {
	t.Parallel()

	processor := NewMetricsProcessor([]string{"score"}, []string{})

	report := analyze.Report{
		"score": 0.85,
	}
	processor.ProcessReport(report)

	if processor.GetReportCount() != 1 {
		t.Errorf("expected 1 report, got %d", processor.GetReportCount())
	}

	if processor.GetMetric("score") != 0.85 {
		t.Errorf("expected score 0.85, got %f", processor.GetMetric("score"))
	}
}

func TestMetricsProcessor_ProcessReport_Count(t *testing.T) {
	t.Parallel()

	processor := NewMetricsProcessor([]string{}, []string{"total"})

	report := analyze.Report{
		"total": 100,
	}
	processor.ProcessReport(report)

	if processor.GetCount("total") != 100 {
		t.Errorf("expected count 100, got %d", processor.GetCount("total"))
	}
}

func TestMetricsProcessor_ProcessReport_MultipleReports(t *testing.T) {
	t.Parallel()

	processor := NewMetricsProcessor([]string{"score"}, []string{"count"})

	report1 := analyze.Report{"score": 0.8, "count": 10}
	report2 := analyze.Report{"score": 0.9, "count": 20}

	processor.ProcessReport(report1)
	processor.ProcessReport(report2)

	if processor.GetReportCount() != 2 {
		t.Errorf("expected 2 reports, got %d", processor.GetReportCount())
	}
	// Total score should be 1.7.
	totalScore := processor.GetMetric("score")
	if totalScore < 1.69 || totalScore > 1.71 {
		t.Errorf("expected total score ~1.7, got %f", totalScore)
	}
	// Total count should be 30.
	if processor.GetCount("count") != 30 {
		t.Errorf("expected total count 30, got %d", processor.GetCount("count"))
	}
}

func TestMetricsProcessor_CalculateAverages(t *testing.T) {
	t.Parallel()

	processor := NewMetricsProcessor([]string{"score"}, []string{})

	report1 := analyze.Report{"score": 0.8}
	report2 := analyze.Report{"score": 0.9}

	processor.ProcessReport(report1)
	processor.ProcessReport(report2)

	averages := processor.CalculateAverages()

	avg := averages["score"]
	if avg < 0.84 || avg > 0.86 {
		t.Errorf("expected average ~0.85, got %f", avg)
	}
}

func TestMetricsProcessor_CalculateAverages_Empty(t *testing.T) {
	t.Parallel()

	processor := NewMetricsProcessor([]string{"score"}, []string{})
	averages := processor.CalculateAverages()

	if len(averages) != 0 {
		t.Error("expected empty averages for no reports")
	}
}

func TestMetricsProcessor_GetCounts(t *testing.T) {
	t.Parallel()

	processor := NewMetricsProcessor([]string{}, []string{"total"})

	report := analyze.Report{"total": 50}
	processor.ProcessReport(report)

	counts := processor.GetCounts()
	if counts["total"] != 50 {
		t.Errorf("expected count 50, got %d", counts["total"])
	}
}

func TestMetricsProcessor_GetReportCount(t *testing.T) {
	t.Parallel()

	processor := NewMetricsProcessor([]string{}, []string{})

	if processor.GetReportCount() != 0 {
		t.Error("expected 0 initial report count")
	}

	processor.ProcessReport(analyze.Report{})
	processor.ProcessReport(analyze.Report{})

	if processor.GetReportCount() != 2 {
		t.Errorf("expected 2 reports, got %d", processor.GetReportCount())
	}
}

func TestMetricsProcessor_GetMetric(t *testing.T) {
	t.Parallel()

	processor := NewMetricsProcessor([]string{"score"}, []string{})

	// Non-existent metric should return 0.
	if processor.GetMetric("nonexistent") != 0 {
		t.Error("expected 0 for non-existent metric")
	}

	processor.ProcessReport(analyze.Report{"score": 0.75})

	if processor.GetMetric("score") != 0.75 {
		t.Errorf("expected 0.75, got %f", processor.GetMetric("score"))
	}
}

func TestMetricsProcessor_GetCount(t *testing.T) {
	t.Parallel()

	processor := NewMetricsProcessor([]string{}, []string{"total"})

	// Non-existent count should return 0.
	if processor.GetCount("nonexistent") != 0 {
		t.Error("expected 0 for non-existent count")
	}

	processor.ProcessReport(analyze.Report{"total": 42})

	if processor.GetCount("total") != 42 {
		t.Errorf("expected 42, got %d", processor.GetCount("total"))
	}
}

func TestMetricsProcessor_extractFloat(t *testing.T) { //nolint:tparallel // parallel test pattern is intentional.
	t.Parallel()

	processor := NewMetricsProcessor([]string{}, []string{})

	tests := []struct {
		value    any
		name     string
		expected float64
		ok       bool
	}{
		{3.14, "float64", 3.14, true},
		{42, "int", 42.0, true},
		{int32(10), "int32", 10.0, true},
		{int64(100), "int64", 100.0, true},
		{"invalid", "string", 0, false},
		{nil, "nil", 0, false},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result, ok := processor.extractFloat(tt.value)
			if ok != tt.ok {
				t.Errorf("expected ok=%v, got ok=%v", tt.ok, ok)
			}

			if ok && result != tt.expected {
				t.Errorf("expected %f, got %f", tt.expected, result)
			}
		})
	}
}

func TestMetricsProcessor_extractInt(t *testing.T) { //nolint:tparallel // parallel test pattern is intentional.
	t.Parallel()

	processor := NewMetricsProcessor([]string{}, []string{})

	tests := []struct {
		value    any
		name     string
		expected int
		ok       bool
	}{
		{42, "int", 42, true},
		{int32(10), "int32", 10, true},
		{int64(100), "int64", 100, true},
		{3.9, "float64", 3, true}, // Truncates.
		{"invalid", "string", 0, false},
		{nil, "nil", 0, false},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result, ok := processor.extractInt(tt.value)
			if ok != tt.ok {
				t.Errorf("expected ok=%v, got ok=%v", tt.ok, ok)
			}

			if ok && result != tt.expected {
				t.Errorf("expected %d, got %d", tt.expected, result)
			}
		})
	}
}

func TestMetricsProcessor_isNumericMetric(t *testing.T) {
	t.Parallel()

	processor := NewMetricsProcessor([]string{"score", "average"}, []string{})

	if !processor.isNumericMetric("score") {
		t.Error("expected 'score' to be numeric")
	}

	if !processor.isNumericMetric("average") {
		t.Error("expected 'average' to be numeric")
	}

	if processor.isNumericMetric("count") {
		t.Error("expected 'count' to not be numeric")
	}
}

func TestMetricsProcessor_isCountMetric(t *testing.T) {
	t.Parallel()

	processor := NewMetricsProcessor([]string{}, []string{"total", "count"})

	if !processor.isCountMetric("total") {
		t.Error("expected 'total' to be count")
	}

	if !processor.isCountMetric("count") {
		t.Error("expected 'count' to be count")
	}

	if processor.isCountMetric("score") {
		t.Error("expected 'score' to not be count")
	}
}

func TestMetricsProcessor_IntegrationWorkflow(t *testing.T) {
	t.Parallel()

	processor := NewMetricsProcessor(
		[]string{"complexity_score", "maintainability"},
		[]string{"function_count", "line_count"},
	)

	// Simulate processing multiple file reports.
	reports := []analyze.Report{
		{
			"complexity_score": 0.75,
			"maintainability":  0.9,
			"function_count":   10,
			"line_count":       200,
		},
		{
			"complexity_score": 0.85,
			"maintainability":  0.8,
			"function_count":   5,
			"line_count":       100,
		},
		{
			"complexity_score": 0.65,
			"maintainability":  0.95,
			"function_count":   15,
			"line_count":       300,
		},
	}

	for _, report := range reports {
		processor.ProcessReport(report)
	}

	// Verify report count.
	if processor.GetReportCount() != 3 {
		t.Errorf("expected 3 reports, got %d", processor.GetReportCount())
	}

	// Verify averages.
	averages := processor.CalculateAverages()

	expectedComplexityAvg := (0.75 + 0.85 + 0.65) / 3.0
	if averages["complexity_score"] != expectedComplexityAvg {
		t.Errorf("expected complexity avg %f, got %f", expectedComplexityAvg, averages["complexity_score"])
	}

	// Verify counts.
	counts := processor.GetCounts()
	if counts["function_count"] != 30 {
		t.Errorf("expected function count 30, got %d", counts["function_count"])
	}

	if counts["line_count"] != 600 {
		t.Errorf("expected line count 600, got %d", counts["line_count"])
	}
}
