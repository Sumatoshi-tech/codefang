package comments

import (
	"testing"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
)

func newTestCommentsReport() analyze.Report {
	return analyze.Report{
		"total_comments":         20,
		"good_comments":          16,
		"bad_comments":           4,
		"overall_score":          0.75,
		"total_functions":        10,
		"documented_functions":   7,
		"good_comments_ratio":    0.8,
		"documentation_coverage": 0.7,
		"message":                "Good comment quality with room for improvement",
		"functions": []map[string]interface{}{
			{"function": "ProcessData", "assessment": "✅ Well Documented"},
			{"function": "HandleRequest", "assessment": "❌ No Comment"},
			{"function": "ParseConfig", "assessment": "✅ Well Documented"},
			{"function": "ValidateInput", "assessment": "❌ No Comment"},
			{"function": "FormatOutput", "assessment": "✅ Well Documented"},
		},
	}
}

func TestCommentsTitle(t *testing.T) {
	section := NewCommentsReportSection(newTestCommentsReport())
	if section.SectionTitle() != SectionTitle {
		t.Errorf("SectionTitle() = %q, want %q", section.SectionTitle(), SectionTitle)
	}
}

func TestCommentsNilReport(t *testing.T) {
	section := NewCommentsReportSection(nil)
	if section.SectionTitle() != SectionTitle {
		t.Errorf("SectionTitle() = %q, want %q", section.SectionTitle(), SectionTitle)
	}
}

func TestCommentsScore(t *testing.T) {
	section := NewCommentsReportSection(newTestCommentsReport())
	const expectedScore = 0.75
	if section.Score() != expectedScore {
		t.Errorf("Score() = %v, want %v", section.Score(), expectedScore)
	}
}

func TestCommentsScore_Empty(t *testing.T) {
	section := NewCommentsReportSection(analyze.Report{})
	if section.Score() != 0 {
		t.Errorf("Score() = %v, want 0 for empty report", section.Score())
	}
}

func TestCommentsStatusMessage(t *testing.T) {
	section := NewCommentsReportSection(newTestCommentsReport())
	want := "Good comment quality with room for improvement"
	if section.StatusMessage() != want {
		t.Errorf("StatusMessage() = %q, want %q", section.StatusMessage(), want)
	}
}

func TestCommentsStatusMessage_Empty(t *testing.T) {
	section := NewCommentsReportSection(analyze.Report{})
	if section.StatusMessage() == "" {
		t.Error("StatusMessage() should not be empty for empty report")
	}
}

func TestCommentsKeyMetrics_Count(t *testing.T) {
	section := NewCommentsReportSection(newTestCommentsReport())
	const expectedCount = 6
	metrics := section.KeyMetrics()
	if len(metrics) != expectedCount {
		t.Errorf("KeyMetrics() count = %d, want %d", len(metrics), expectedCount)
	}
}

func TestCommentsKeyMetrics_Labels(t *testing.T) {
	section := NewCommentsReportSection(newTestCommentsReport())
	metrics := section.KeyMetrics()
	expectedLabels := []string{
		MetricTotalComments, MetricGoodComments, MetricBadComments,
		MetricDocCoverage, MetricGoodRatio, MetricTotalFunctions,
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

func TestCommentsKeyMetrics_Values(t *testing.T) {
	section := NewCommentsReportSection(newTestCommentsReport())
	metrics := section.KeyMetrics()
	if metrics[0].Value != "20" {
		t.Errorf("Total Comments = %q, want %q", metrics[0].Value, "20")
	}
	if metrics[3].Value != "70.0%" {
		t.Errorf("Doc Coverage = %q, want %q", metrics[3].Value, "70.0%")
	}
}

func TestCommentsDistribution(t *testing.T) {
	section := NewCommentsReportSection(newTestCommentsReport())
	dist := section.Distribution()
	const expectedCategories = 2
	if len(dist) != expectedCategories {
		t.Fatalf("Distribution() count = %d, want %d", len(dist), expectedCategories)
	}
	if dist[0].Label != DistLabelDocumented {
		t.Errorf("dist[0].Label = %q, want %q", dist[0].Label, DistLabelDocumented)
	}
	const expectedDocumented = 7
	const expectedUndocumented = 3
	if dist[0].Count != expectedDocumented {
		t.Errorf("Documented count = %d, want %d", dist[0].Count, expectedDocumented)
	}
	if dist[1].Count != expectedUndocumented {
		t.Errorf("Undocumented count = %d, want %d", dist[1].Count, expectedUndocumented)
	}
}

func TestCommentsDistribution_Empty(t *testing.T) {
	section := NewCommentsReportSection(analyze.Report{})
	if section.Distribution() != nil {
		t.Error("Distribution() should be nil for empty report")
	}
}

func TestCommentsTopIssues(t *testing.T) {
	section := NewCommentsReportSection(newTestCommentsReport())
	const topN = 1
	issues := section.TopIssues(topN)
	if len(issues) != topN {
		t.Fatalf("TopIssues(%d) count = %d, want %d", topN, len(issues), topN)
	}
	// Sorted alphabetically, first undocumented is HandleRequest
	if issues[0].Name != "HandleRequest" {
		t.Errorf("issues[0].Name = %q, want %q", issues[0].Name, "HandleRequest")
	}
	if issues[0].Severity != analyze.SeverityPoor {
		t.Errorf("issues[0].Severity = %q, want %q", issues[0].Severity, analyze.SeverityPoor)
	}
}

func TestCommentsAllIssues(t *testing.T) {
	section := NewCommentsReportSection(newTestCommentsReport())
	const expectedUndocumented = 2
	issues := section.AllIssues()
	if len(issues) != expectedUndocumented {
		t.Errorf("AllIssues() count = %d, want %d", len(issues), expectedUndocumented)
	}
}

func TestCommentsTopIssues_Empty(t *testing.T) {
	section := NewCommentsReportSection(analyze.Report{})
	const n = 5
	issues := section.TopIssues(n)
	if len(issues) != 0 {
		t.Errorf("TopIssues(%d) should be empty for empty report, got %d", n, len(issues))
	}
}

func TestCommentsImplementsInterface(t *testing.T) {
	var _ analyze.ReportSection = (*CommentsReportSection)(nil)
}
