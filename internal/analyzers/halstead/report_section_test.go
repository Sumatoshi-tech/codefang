package halstead

import (
	"testing"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
)

func newTestHalsteadReport() analyze.Report {
	return analyze.Report{
		"total_functions": 4,
		"vocabulary":      25,
		"volume":          450.5,
		"difficulty":      12.3,
		"effort":          5541.15,
		"delivered_bugs":  0.15,
		"message":         "Good complexity - code is reasonably complex",
		"functions": []map[string]any{
			{"name": "ProcessData", "volume": 800.0, "effort": 25000.0},
			{"name": "HandleRequest", "volume": 200.0, "effort": 5000.0},
			{"name": "ParseConfig", "volume": 50.0, "effort": 1500.0},
			{"name": "GetName", "volume": 20.0, "effort": 200.0},
		},
	}
}

func TestHalsteadTitle(t *testing.T) {
	t.Parallel()

	section := NewReportSection(newTestHalsteadReport())
	if section.SectionTitle() != SectionTitle {
		t.Errorf("SectionTitle() = %q, want %q", section.SectionTitle(), SectionTitle)
	}
}

func TestHalsteadNilReport(t *testing.T) {
	t.Parallel()

	section := NewReportSection(nil)
	if section.SectionTitle() != SectionTitle {
		t.Errorf("SectionTitle() = %q, want %q", section.SectionTitle(), SectionTitle)
	}
}

func TestHalsteadScore_Good(t *testing.T) {
	t.Parallel()

	section := NewReportSection(newTestHalsteadReport())
	// Difficulty=12.3 -> <= 15 -> 0.8 (good).
	if section.Score() != ScoreGood {
		t.Errorf("Score() = %v, want %v for difficulty=12.3", section.Score(), ScoreGood)
	}
}

func TestHalsteadScore_Excellent(t *testing.T) {
	t.Parallel()

	report := analyze.Report{"difficulty": 3.0}

	section := NewReportSection(report)
	if section.Score() != ScoreExcellent {
		t.Errorf("Score() = %v, want %v", section.Score(), ScoreExcellent)
	}
}

func TestHalsteadScore_Fair(t *testing.T) {
	t.Parallel()

	report := analyze.Report{"difficulty": 25.0}

	section := NewReportSection(report)
	if section.Score() != ScoreFair {
		t.Errorf("Score() = %v, want %v", section.Score(), ScoreFair)
	}
}

func TestHalsteadScore_Poor(t *testing.T) {
	t.Parallel()

	report := analyze.Report{"difficulty": 50.0}

	section := NewReportSection(report)
	if section.Score() != ScorePoor {
		t.Errorf("Score() = %v, want %v", section.Score(), ScorePoor)
	}
}

func TestHalsteadScore_Empty(t *testing.T) {
	t.Parallel()

	section := NewReportSection(analyze.Report{})
	// Difficulty=0 -> excellent.
	if section.Score() != ScoreExcellent {
		t.Errorf("Score() = %v, want %v for empty", section.Score(), ScoreExcellent)
	}
}

func TestHalsteadStatusMessage(t *testing.T) {
	t.Parallel()

	section := NewReportSection(newTestHalsteadReport())

	want := "Good complexity - code is reasonably complex"
	if section.StatusMessage() != want {
		t.Errorf("StatusMessage() = %q, want %q", section.StatusMessage(), want)
	}
}

func TestHalsteadStatusMessage_Empty(t *testing.T) {
	t.Parallel()

	section := NewReportSection(analyze.Report{})
	if section.StatusMessage() == "" {
		t.Error("StatusMessage() should not be empty for empty report")
	}
}

func TestHalsteadKeyMetrics_Count(t *testing.T) {
	t.Parallel()

	section := NewReportSection(newTestHalsteadReport())

	const expectedCount = 10

	metrics := section.KeyMetrics()
	if len(metrics) != expectedCount {
		t.Errorf("KeyMetrics() count = %d, want %d", len(metrics), expectedCount)
	}
}

func TestHalsteadKeyMetrics_Labels(t *testing.T) {
	t.Parallel()

	section := NewReportSection(newTestHalsteadReport())
	metrics := section.KeyMetrics()

	expectedLabels := []string{
		MetricTotalFunctions, MetricDistinctOps, MetricDistinctOpnds,
		MetricTotalOps, MetricTotalOpnds, MetricVocabulary, MetricVolume,
		MetricDifficulty, MetricEffort, MetricEstBugs,
	}
	for i, expected := range expectedLabels {
		if metrics[i].Label != expected {
			t.Errorf("metrics[%d].Label = %q, want %q", i, metrics[i].Label, expected)
		}
	}
}

func TestHalsteadDistribution(t *testing.T) {
	t.Parallel()

	section := NewReportSection(newTestHalsteadReport())
	dist := section.Distribution()

	const expectedCategories = 4
	if len(dist) != expectedCategories {
		t.Fatalf("Distribution() count = %d, want %d", len(dist), expectedCategories)
	}
	// Low (<=100): ParseConfig(50), GetName(20) = 2
	// Medium (101-1000): ProcessData(800), HandleRequest(200) = 2.
	const expectedLow = 2

	const expectedMedium = 2

	if dist[0].Count != expectedLow {
		t.Errorf("Low count = %d, want %d", dist[0].Count, expectedLow)
	}

	if dist[1].Count != expectedMedium {
		t.Errorf("Medium count = %d, want %d", dist[1].Count, expectedMedium)
	}
}

func TestHalsteadDistribution_HighVolume(t *testing.T) {
	t.Parallel()

	report := analyze.Report{
		"functions": []map[string]any{
			{"name": "Big", "volume": 3000.0, "effort": 100000.0},
			{"name": "Huge", "volume": 8000.0, "effort": 200000.0},
		},
	}
	section := NewReportSection(report)
	dist := section.Distribution()

	const expectedHigh = 1

	const expectedVHigh = 1

	if dist[2].Count != expectedHigh {
		t.Errorf("High count = %d, want %d", dist[2].Count, expectedHigh)
	}

	if dist[3].Count != expectedVHigh {
		t.Errorf("Very High count = %d, want %d", dist[3].Count, expectedVHigh)
	}
}

func TestHalsteadDistribution_Empty(t *testing.T) {
	t.Parallel()

	section := NewReportSection(analyze.Report{})
	if section.Distribution() != nil {
		t.Error("Distribution() should be nil for empty report")
	}
}

func TestHalsteadTopIssues_SortedByEffort(t *testing.T) {
	t.Parallel()

	section := NewReportSection(newTestHalsteadReport())

	const topN = 2

	issues := section.TopIssues(topN)
	if len(issues) != topN {
		t.Fatalf("TopIssues(%d) count = %d, want %d", topN, len(issues), topN)
	}
	// Highest effort first: ProcessData(25000), HandleRequest(5000).
	if issues[0].Name != "ProcessData" {
		t.Errorf("issues[0].Name = %q, want %q", issues[0].Name, "ProcessData")
	}
}

func TestHalsteadTopIssues_Severity(t *testing.T) {
	t.Parallel()

	section := NewReportSection(newTestHalsteadReport())

	const topN = 3

	issues := section.TopIssues(topN)
	// ProcessData(25000) -> fair, HandleRequest(5000) -> good, ParseConfig(1500) -> good.
	if issues[0].Severity != analyze.SeverityFair {
		t.Errorf("issues[0].Severity = %q, want %q", issues[0].Severity, analyze.SeverityFair)
	}

	if issues[1].Severity != analyze.SeverityGood {
		t.Errorf("issues[1].Severity = %q, want %q", issues[1].Severity, analyze.SeverityGood)
	}
}

func TestHalsteadTopIssues_SeverityPoor(t *testing.T) {
	t.Parallel()

	report := analyze.Report{
		"functions": []map[string]any{
			{"name": "Monster", "effort": 60000.0},
		},
	}
	section := NewReportSection(report)

	issues := section.TopIssues(1)
	if issues[0].Severity != analyze.SeverityPoor {
		t.Errorf("Severity = %q, want %q for effort=60000", issues[0].Severity, analyze.SeverityPoor)
	}
}

func TestHalsteadAllIssues(t *testing.T) {
	t.Parallel()

	section := NewReportSection(newTestHalsteadReport())

	const totalFunctions = 4

	issues := section.AllIssues()
	if len(issues) != totalFunctions {
		t.Errorf("AllIssues() count = %d, want %d", len(issues), totalFunctions)
	}
}

func TestHalsteadTopIssues_Empty(t *testing.T) {
	t.Parallel()

	section := NewReportSection(analyze.Report{})

	const n = 5
	if len(section.TopIssues(n)) != 0 {
		t.Error("TopIssues should be empty for empty report")
	}
}

func TestHalsteadImplementsInterface(t *testing.T) {
	t.Parallel()

	var _ analyze.ReportSection = (*ReportSection)(nil)
}
