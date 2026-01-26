package complexity //nolint:testpackage // testing internal implementation.

import (
	"testing"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
)

// --- Test helpers ---.

func newTestReport() analyze.Report {
	return analyze.Report{
		"total_functions":      10,
		"average_complexity":   2.5,
		"max_complexity":       8,
		"total_complexity":     25,
		"cognitive_complexity": 18,
		"nesting_depth":        3,
		"decision_points":      15,
		"message":              "Good complexity - functions have reasonable complexity",
		"functions": []map[string]any{
			{"name": "ProcessData", "cyclomatic_complexity": 8, "cognitive_complexity": 6, "nesting_depth": 3, "lines_of_code": 42},
			{"name": "HandleRequest", "cyclomatic_complexity": 5, "cognitive_complexity": 4, "nesting_depth": 2, "lines_of_code": 28},
			{"name": "ParseConfig", "cyclomatic_complexity": 3, "cognitive_complexity": 2, "nesting_depth": 1, "lines_of_code": 15},
			{"name": "ValidateInput", "cyclomatic_complexity": 2, "cognitive_complexity": 1, "nesting_depth": 1, "lines_of_code": 10},
			{"name": "FormatOutput", "cyclomatic_complexity": 1, "cognitive_complexity": 1, "nesting_depth": 0, "lines_of_code": 8},
			{"name": "SimpleHelper", "cyclomatic_complexity": 1, "cognitive_complexity": 0, "nesting_depth": 0, "lines_of_code": 5},
			{"name": "AnotherSimple", "cyclomatic_complexity": 1, "cognitive_complexity": 0, "nesting_depth": 0, "lines_of_code": 4},
			{"name": "GetName", "cyclomatic_complexity": 1, "cognitive_complexity": 0, "nesting_depth": 0, "lines_of_code": 3},
			{"name": "SetValue", "cyclomatic_complexity": 1, "cognitive_complexity": 0, "nesting_depth": 0, "lines_of_code": 3},
			{"name": "Init", "cyclomatic_complexity": 2, "cognitive_complexity": 1, "nesting_depth": 1, "lines_of_code": 12},
		},
	}
}

func newEmptyReport() analyze.Report {
	return analyze.Report{}
}

// --- Title tests ---.

func TestNewReportSection_Title(t *testing.T) {
	t.Parallel()

	section := NewReportSection(newTestReport())

	if section.SectionTitle() != SectionTitle {
		t.Errorf("SectionTitle() = %q, want %q", section.SectionTitle(), SectionTitle)
	}
}

func TestNewReportSection_NilReport(t *testing.T) {
	t.Parallel()

	section := NewReportSection(nil)

	if section.SectionTitle() != SectionTitle {
		t.Errorf("SectionTitle() = %q, want %q", section.SectionTitle(), SectionTitle)
	}
}

// --- Score tests ---.

func TestScore_Excellent(t *testing.T) {
	t.Parallel()

	report := analyze.Report{"average_complexity": 0.8}
	section := NewReportSection(report)

	got := section.Score()
	if got != 1.0 {
		t.Errorf("Score() = %v, want 1.0 for avg=0.8", got)
	}
}

func TestScore_Good(t *testing.T) {
	t.Parallel()

	report := analyze.Report{"average_complexity": 2.5}
	section := NewReportSection(report)

	got := section.Score()
	if got != 0.8 {
		t.Errorf("Score() = %v, want 0.8 for avg=2.5", got)
	}
}

func TestScore_Fair(t *testing.T) {
	t.Parallel()

	report := analyze.Report{"average_complexity": 4.5}
	section := NewReportSection(report)

	got := section.Score()
	if got != 0.6 {
		t.Errorf("Score() = %v, want 0.6 for avg=4.5", got)
	}
}

func TestScore_Moderate(t *testing.T) {
	t.Parallel()

	report := analyze.Report{"average_complexity": 6.5}
	section := NewReportSection(report)

	got := section.Score()
	if got != 0.4 {
		t.Errorf("Score() = %v, want 0.4 for avg=6.5", got)
	}
}

func TestScore_Poor(t *testing.T) {
	t.Parallel()

	report := analyze.Report{"average_complexity": 9.0}
	section := NewReportSection(report)

	got := section.Score()
	if got != 0.2 {
		t.Errorf("Score() = %v, want 0.2 for avg=9.0", got)
	}
}

func TestScore_Critical(t *testing.T) {
	t.Parallel()

	report := analyze.Report{"average_complexity": 15.0}
	section := NewReportSection(report)

	got := section.Score()
	if got != 0.1 {
		t.Errorf("Score() = %v, want 0.1 for avg=15.0", got)
	}
}

func TestScore_EmptyReport(t *testing.T) {
	t.Parallel()

	section := NewReportSection(newEmptyReport())

	got := section.Score()
	if got != 1.0 {
		t.Errorf("Score() = %v, want 1.0 for empty report (avg=0)", got)
	}
}

// --- StatusMessage tests ---.

func TestStatusMessage_FromReport(t *testing.T) {
	t.Parallel()

	section := NewReportSection(newTestReport())

	got := section.StatusMessage()

	want := "Good complexity - functions have reasonable complexity"
	if got != want {
		t.Errorf("StatusMessage() = %q, want %q", got, want)
	}
}

func TestStatusMessage_EmptyReport(t *testing.T) {
	t.Parallel()

	section := NewReportSection(newEmptyReport())

	got := section.StatusMessage()
	if got == "" {
		t.Errorf("StatusMessage() should not be empty for empty report")
	}
}

// --- KeyMetrics tests ---.

func TestKeyMetrics_Count(t *testing.T) {
	t.Parallel()

	section := NewReportSection(newTestReport())

	metrics := section.KeyMetrics()

	const expectedMetricCount = 6
	if len(metrics) != expectedMetricCount {
		t.Errorf("KeyMetrics() count = %d, want %d", len(metrics), expectedMetricCount)
	}
}

func TestKeyMetrics_Labels(t *testing.T) {
	t.Parallel()

	section := NewReportSection(newTestReport())

	metrics := section.KeyMetrics()
	expectedLabels := []string{
		"Total Functions",
		"Avg Complexity",
		"Max Complexity",
		"Total Complexity",
		"Cognitive Total",
		"Decision Points",
	}

	for i, expected := range expectedLabels {
		if i >= len(metrics) {
			t.Fatalf("Missing metric at index %d: want %q", i, expected)
		}

		if metrics[i].Label != expected {
			t.Errorf("metrics[%d].Label = %q, want %q", i, metrics[i].Label, expected)
		}
	}
}

func TestKeyMetrics_Values(t *testing.T) {
	t.Parallel()

	section := NewReportSection(newTestReport())

	metrics := section.KeyMetrics()
	if len(metrics) == 0 {
		t.Fatal("KeyMetrics() returned empty")
	}

	// Total Functions = 10.
	if metrics[0].Value != "10" {
		t.Errorf("Total Functions value = %q, want %q", metrics[0].Value, "10")
	}
	// Avg Complexity = 2.5.
	if metrics[1].Value != "2.5" {
		t.Errorf("Avg Complexity value = %q, want %q", metrics[1].Value, "2.5")
	}
}

func TestKeyMetrics_EmptyReport(t *testing.T) {
	t.Parallel()

	section := NewReportSection(newEmptyReport())

	metrics := section.KeyMetrics()
	if len(metrics) == 0 {
		t.Error("KeyMetrics() should return metrics even for empty report")
	}
}

// --- Distribution tests ---.

func TestDistribution_Categories(t *testing.T) {
	t.Parallel()

	section := NewReportSection(newTestReport())

	dist := section.Distribution()

	const expectedDistCategories = 4
	if len(dist) != expectedDistCategories {
		t.Fatalf("Distribution() count = %d, want %d", len(dist), expectedDistCategories)
	}

	expectedLabels := []string{
		"Simple (1-5)",
		"Moderate (6-10)",
		"Complex (11-20)",
		"Very Complex (>20)",
	}

	for i, expected := range expectedLabels {
		if dist[i].Label != expected {
			t.Errorf("dist[%d].Label = %q, want %q", i, dist[i].Label, expected)
		}
	}
}

func TestDistribution_Counts(t *testing.T) {
	t.Parallel()

	section := NewReportSection(newTestReport())

	dist := section.Distribution()
	if len(dist) == 0 {
		t.Fatal("Distribution() returned empty")
	}

	// From test report: CC values are 8,5,3,2,1,1,1,1,1,2
	// Simple (1-5): 5+3+2+1+1+1+1+1+2 = 9 functions
	// Moderate (6-10): 8 = 1 function.
	const expectedSimple = 9

	const expectedModerate = 1

	if dist[0].Count != expectedSimple {
		t.Errorf("Simple count = %d, want %d", dist[0].Count, expectedSimple)
	}

	if dist[1].Count != expectedModerate {
		t.Errorf("Moderate count = %d, want %d", dist[1].Count, expectedModerate)
	}
}

func TestDistribution_Percentages(t *testing.T) {
	t.Parallel()

	section := NewReportSection(newTestReport())

	dist := section.Distribution()
	if len(dist) == 0 {
		t.Fatal("Distribution() returned empty")
	}

	var totalPercent float64
	for _, item := range dist {
		totalPercent += item.Percent
	}

	if totalPercent < 0.99 || totalPercent > 1.01 {
		t.Errorf("Distribution percentages sum = %v, want ~1.0", totalPercent)
	}
}

func TestDistribution_EmptyFunctions(t *testing.T) {
	t.Parallel()

	section := NewReportSection(newEmptyReport())

	dist := section.Distribution()
	if len(dist) != 0 {
		t.Errorf("Distribution() should be empty for empty report, got %d items", len(dist))
	}
}

// --- Issues tests ---.

func TestTopIssues_SortedByComplexity(t *testing.T) {
	t.Parallel()

	section := NewReportSection(newTestReport())

	const topN = 3

	issues := section.TopIssues(topN)
	if len(issues) != topN {
		t.Fatalf("TopIssues(%d) count = %d, want %d", topN, len(issues), topN)
	}

	// Highest complexity first: ProcessData(8), HandleRequest(5), ParseConfig(3).
	if issues[0].Name != "ProcessData" {
		t.Errorf("issues[0].Name = %q, want %q", issues[0].Name, "ProcessData")
	}

	if issues[1].Name != "HandleRequest" {
		t.Errorf("issues[1].Name = %q, want %q", issues[1].Name, "HandleRequest")
	}

	if issues[2].Name != "ParseConfig" {
		t.Errorf("issues[2].Name = %q, want %q", issues[2].Name, "ParseConfig")
	}
}

func TestTopIssues_Value(t *testing.T) {
	t.Parallel()

	section := NewReportSection(newTestReport())

	issues := section.TopIssues(1)
	if len(issues) == 0 {
		t.Fatal("TopIssues(1) returned empty")
	}

	if issues[0].Value != "CC=8" {
		t.Errorf("issues[0].Value = %q, want %q", issues[0].Value, "CC=8")
	}
}

func TestTopIssues_Severity(t *testing.T) {
	t.Parallel()

	section := NewReportSection(newTestReport())

	issues := section.TopIssues(3)
	if len(issues) < 3 {
		t.Fatal("TopIssues(3) returned fewer than 3 issues")
	}

	// CC=8 is moderate (6-10) -> fair.
	if issues[0].Severity != analyze.SeverityFair {
		t.Errorf("issues[0].Severity = %q, want %q for CC=8", issues[0].Severity, analyze.SeverityFair)
	}
	// CC=5 is simple (1-5) -> good.
	if issues[1].Severity != analyze.SeverityGood {
		t.Errorf("issues[1].Severity = %q, want %q for CC=5", issues[1].Severity, analyze.SeverityGood)
	}
}

func TestTopIssues_LimitExceedsCount(t *testing.T) {
	t.Parallel()

	section := NewReportSection(newTestReport())

	const limit = 50

	const totalFunctions = 10

	issues := section.TopIssues(limit)
	if len(issues) != totalFunctions {
		t.Errorf("TopIssues(%d) = %d items, want %d (total functions)", limit, len(issues), totalFunctions)
	}
}

func TestAllIssues_ReturnsAll(t *testing.T) {
	t.Parallel()

	section := NewReportSection(newTestReport())

	const totalFunctions = 10

	issues := section.AllIssues()
	if len(issues) != totalFunctions {
		t.Errorf("AllIssues() = %d items, want %d", len(issues), totalFunctions)
	}
}

func TestTopIssues_EmptyReport(t *testing.T) {
	t.Parallel()

	section := NewReportSection(newEmptyReport())

	issues := section.TopIssues(5)
	if len(issues) != 0 {
		t.Errorf("TopIssues(5) should be empty for empty report, got %d items", len(issues))
	}
}

// --- Edge case tests for full coverage ---.

func TestDistribution_VeryComplexFunctions(t *testing.T) {
	t.Parallel()

	report := analyze.Report{
		"functions": []map[string]any{
			{"name": "Monster", "cyclomatic_complexity": 25},
			{"name": "BigBeast", "cyclomatic_complexity": 15},
			{"name": "Simple", "cyclomatic_complexity": 2},
		},
	}
	section := NewReportSection(report)

	dist := section.Distribution()
	// Simple: 1, Moderate: 0, Complex: 1, Very Complex: 1.
	const expectedVeryComplex = 1

	const expectedComplex = 1

	if dist[3].Count != expectedVeryComplex {
		t.Errorf("Very Complex count = %d, want %d", dist[3].Count, expectedVeryComplex)
	}

	if dist[2].Count != expectedComplex {
		t.Errorf("Complex count = %d, want %d", dist[2].Count, expectedComplex)
	}
}

func TestTopIssues_SeverityPoor(t *testing.T) {
	t.Parallel()

	report := analyze.Report{
		"functions": []map[string]any{
			{"name": "VeryComplex", "cyclomatic_complexity": 15},
		},
	}
	section := NewReportSection(report)

	issues := section.TopIssues(1)
	if len(issues) == 0 {
		t.Fatal("TopIssues(1) returned empty")
	}

	if issues[0].Severity != analyze.SeverityPoor {
		t.Errorf("Severity = %q, want %q for CC=15", issues[0].Severity, analyze.SeverityPoor)
	}
}

func TestGetFloat64_IntValue(t *testing.T) {
	t.Parallel()

	report := analyze.Report{KeyAvgComplexity: 5}
	section := NewReportSection(report)

	// Score for avg=5.0 should be 0.6 (fair).
	if section.Score() != ScoreFair {
		t.Errorf("Score() = %v, want %v for int avg=5", section.Score(), ScoreFair)
	}
}

func TestGetInt_Float64Value(t *testing.T) {
	t.Parallel()

	report := analyze.Report{
		KeyTotalFunctions: 42.0,
	}
	section := NewReportSection(report)

	metrics := section.KeyMetrics()
	if metrics[0].Value != "42" {
		t.Errorf("Total Functions = %q, want %q from float64 value", metrics[0].Value, "42")
	}
}

func TestGetIntFromMap_Float64Value(t *testing.T) {
	t.Parallel()

	report := analyze.Report{
		"functions": []map[string]any{
			{"name": "Func", "cyclomatic_complexity": 7.0},
		},
	}
	section := NewReportSection(report)

	issues := section.TopIssues(1)
	if len(issues) == 0 {
		t.Fatal("TopIssues(1) returned empty")
	}

	if issues[0].Value != "CC=7" {
		t.Errorf("Value = %q, want %q", issues[0].Value, "CC=7")
	}
}

// --- Interface compliance ---.

func TestReportSection_ImplementsInterface(t *testing.T) {
	t.Parallel()

	var _ analyze.ReportSection = (*ReportSection)(nil)
}
