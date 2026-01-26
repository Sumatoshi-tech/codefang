package cohesion //nolint:testpackage // testing internal implementation.

import (
	"testing"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
)

func newTestCohesionReport() analyze.Report {
	return analyze.Report{
		"total_functions":   5,
		"lcom":              1.5,
		"cohesion_score":    0.7,
		"function_cohesion": 0.65,
		"message":           "Good cohesion - functions have reasonable focus",
		"functions": []map[string]any{
			{"name": "ProcessData", "cohesion": 0.9},
			{"name": "HandleRequest", "cohesion": 0.7},
			{"name": "ParseConfig", "cohesion": 0.5},
			{"name": "ValidateInput", "cohesion": 0.2},
			{"name": "FormatOutput", "cohesion": 0.85},
		},
	}
}

func TestCohesionTitle(t *testing.T) {
	t.Parallel()

	section := NewCohesionReportSection(newTestCohesionReport())
	if section.SectionTitle() != SectionTitle {
		t.Errorf("SectionTitle() = %q, want %q", section.SectionTitle(), SectionTitle)
	}
}

func TestCohesionNilReport(t *testing.T) {
	t.Parallel()

	section := NewCohesionReportSection(nil)
	if section.SectionTitle() != SectionTitle {
		t.Errorf("SectionTitle() = %q, want %q", section.SectionTitle(), SectionTitle)
	}
}

func TestCohesionScore(t *testing.T) {
	t.Parallel()

	section := NewCohesionReportSection(newTestCohesionReport())

	const expectedScore = 0.7
	if section.Score() != expectedScore {
		t.Errorf("Score() = %v, want %v", section.Score(), expectedScore)
	}
}

func TestCohesionScore_Empty(t *testing.T) {
	t.Parallel()

	section := NewCohesionReportSection(analyze.Report{})
	if section.Score() != 0 {
		t.Errorf("Score() = %v, want 0 for empty report", section.Score())
	}
}

func TestCohesionStatusMessage(t *testing.T) {
	t.Parallel()

	section := NewCohesionReportSection(newTestCohesionReport())

	want := "Good cohesion - functions have reasonable focus"
	if section.StatusMessage() != want {
		t.Errorf("StatusMessage() = %q, want %q", section.StatusMessage(), want)
	}
}

func TestCohesionStatusMessage_Empty(t *testing.T) {
	t.Parallel()

	section := NewCohesionReportSection(analyze.Report{})
	if section.StatusMessage() == "" {
		t.Error("StatusMessage() should not be empty for empty report")
	}
}

func TestCohesionKeyMetrics_Count(t *testing.T) {
	t.Parallel()

	section := NewCohesionReportSection(newTestCohesionReport())

	const expectedCount = 4

	metrics := section.KeyMetrics()
	if len(metrics) != expectedCount {
		t.Errorf("KeyMetrics() count = %d, want %d", len(metrics), expectedCount)
	}
}

func TestCohesionKeyMetrics_Labels(t *testing.T) {
	t.Parallel()

	section := NewCohesionReportSection(newTestCohesionReport())
	metrics := section.KeyMetrics()

	expectedLabels := []string{MetricTotalFunctions, MetricLCOM, MetricCohesionScore, MetricFunctionCohesion}
	for i, expected := range expectedLabels {
		if metrics[i].Label != expected {
			t.Errorf("metrics[%d].Label = %q, want %q", i, metrics[i].Label, expected)
		}
	}
}

func TestCohesionDistribution(t *testing.T) {
	t.Parallel()

	section := NewCohesionReportSection(newTestCohesionReport())
	dist := section.Distribution()

	const expectedCategories = 4
	if len(dist) != expectedCategories {
		t.Fatalf("Distribution() count = %d, want %d", len(dist), expectedCategories)
	}
	// Excellent (>0.8): ProcessData(0.9), FormatOutput(0.85) = 2
	// Good (0.6-0.8): HandleRequest(0.7) = 1
	// Fair (0.3-0.6): ParseConfig(0.5) = 1
	// Poor (<0.3): ValidateInput(0.2) = 1.
	const expectedExcellent = 2

	const expectedGood = 1

	const expectedFair = 1

	const expectedPoor = 1

	if dist[0].Count != expectedExcellent {
		t.Errorf("Excellent count = %d, want %d", dist[0].Count, expectedExcellent)
	}

	if dist[1].Count != expectedGood {
		t.Errorf("Good count = %d, want %d", dist[1].Count, expectedGood)
	}

	if dist[2].Count != expectedFair {
		t.Errorf("Fair count = %d, want %d", dist[2].Count, expectedFair)
	}

	if dist[3].Count != expectedPoor {
		t.Errorf("Poor count = %d, want %d", dist[3].Count, expectedPoor)
	}
}

func TestCohesionDistribution_Empty(t *testing.T) {
	t.Parallel()

	section := NewCohesionReportSection(analyze.Report{})
	if section.Distribution() != nil {
		t.Error("Distribution() should be nil for empty report")
	}
}

func TestCohesionTopIssues_SortedAscending(t *testing.T) {
	t.Parallel()

	section := NewCohesionReportSection(newTestCohesionReport())

	const topN = 2

	issues := section.TopIssues(topN)
	if len(issues) != topN {
		t.Fatalf("TopIssues(%d) count = %d, want %d", topN, len(issues), topN)
	}
	// Lowest cohesion first: ValidateInput(0.2), ParseConfig(0.5).
	if issues[0].Name != "ValidateInput" {
		t.Errorf("issues[0].Name = %q, want %q", issues[0].Name, "ValidateInput")
	}
}

func TestCohesionTopIssues_Severity(t *testing.T) {
	t.Parallel()

	section := NewCohesionReportSection(newTestCohesionReport())

	const topN = 3

	issues := section.TopIssues(topN)
	// ValidateInput(0.2) -> poor, ParseConfig(0.5) -> fair, HandleRequest(0.7) -> good.
	if issues[0].Severity != analyze.SeverityPoor {
		t.Errorf("issues[0].Severity = %q, want %q", issues[0].Severity, analyze.SeverityPoor)
	}

	if issues[1].Severity != analyze.SeverityFair {
		t.Errorf("issues[1].Severity = %q, want %q", issues[1].Severity, analyze.SeverityFair)
	}

	if issues[2].Severity != analyze.SeverityGood {
		t.Errorf("issues[2].Severity = %q, want %q", issues[2].Severity, analyze.SeverityGood)
	}
}

func TestCohesionAllIssues(t *testing.T) {
	t.Parallel()

	section := NewCohesionReportSection(newTestCohesionReport())

	const totalFunctions = 5

	issues := section.AllIssues()
	if len(issues) != totalFunctions {
		t.Errorf("AllIssues() count = %d, want %d", len(issues), totalFunctions)
	}
}

func TestCohesionTopIssues_Empty(t *testing.T) {
	t.Parallel()

	section := NewCohesionReportSection(analyze.Report{})

	const n = 5
	if len(section.TopIssues(n)) != 0 {
		t.Error("TopIssues should be empty for empty report")
	}
}

func TestCohesionImplementsInterface(t *testing.T) {
	t.Parallel()

	var _ analyze.ReportSection = (*CohesionReportSection)(nil)
}
