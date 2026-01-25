package imports

import (
	"testing"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
)

func newTestImportsReport() analyze.Report {
	return analyze.Report{
		"imports":     []string{"os", "fmt", "strings", "errors"},
		"count":       4,
		"total_files": 10,
		"import_counts": map[string]int{
			"os":      8,
			"fmt":     12,
			"strings": 3,
			"errors":  5,
		},
	}
}

func newSimpleImportsReport() analyze.Report {
	return analyze.Report{
		"imports": []string{"os", "fmt"},
		"count":   2,
	}
}

func TestImportsTitle(t *testing.T) {
	section := NewImportsReportSection(newTestImportsReport())
	if section.SectionTitle() != SectionTitle {
		t.Errorf("SectionTitle() = %q, want %q", section.SectionTitle(), SectionTitle)
	}
}

func TestImportsNilReport(t *testing.T) {
	section := NewImportsReportSection(nil)
	if section.SectionTitle() != SectionTitle {
		t.Errorf("SectionTitle() = %q, want %q", section.SectionTitle(), SectionTitle)
	}
}

func TestImportsScore_InfoOnly(t *testing.T) {
	section := NewImportsReportSection(newTestImportsReport())
	if section.Score() != analyze.ScoreInfoOnly {
		t.Errorf("Score() = %v, want %v", section.Score(), analyze.ScoreInfoOnly)
	}
}

func TestImportsStatusMessage(t *testing.T) {
	section := NewImportsReportSection(newTestImportsReport())
	want := "Found 4 unique imports"
	if section.StatusMessage() != want {
		t.Errorf("StatusMessage() = %q, want %q", section.StatusMessage(), want)
	}
}

func TestImportsStatusMessage_Empty(t *testing.T) {
	section := NewImportsReportSection(analyze.Report{})
	if section.StatusMessage() != DefaultStatusMessage {
		t.Errorf("StatusMessage() = %q, want %q", section.StatusMessage(), DefaultStatusMessage)
	}
}

func TestImportsKeyMetrics_Count(t *testing.T) {
	section := NewImportsReportSection(newTestImportsReport())
	const expectedCount = 2
	metrics := section.KeyMetrics()
	if len(metrics) != expectedCount {
		t.Errorf("KeyMetrics() count = %d, want %d", len(metrics), expectedCount)
	}
}

func TestImportsKeyMetrics_Labels(t *testing.T) {
	section := NewImportsReportSection(newTestImportsReport())
	metrics := section.KeyMetrics()
	if metrics[0].Label != MetricUniqueImports {
		t.Errorf("metrics[0].Label = %q, want %q", metrics[0].Label, MetricUniqueImports)
	}
	if metrics[1].Label != MetricTotalFiles {
		t.Errorf("metrics[1].Label = %q, want %q", metrics[1].Label, MetricTotalFiles)
	}
}

func TestImportsKeyMetrics_Values(t *testing.T) {
	section := NewImportsReportSection(newTestImportsReport())
	metrics := section.KeyMetrics()
	if metrics[0].Value != "4" {
		t.Errorf("Unique Imports = %q, want %q", metrics[0].Value, "4")
	}
	if metrics[1].Value != "10" {
		t.Errorf("Total Files = %q, want %q", metrics[1].Value, "10")
	}
}

func TestImportsDistribution_Nil(t *testing.T) {
	section := NewImportsReportSection(newTestImportsReport())
	if section.Distribution() != nil {
		t.Error("Distribution() should be nil for imports")
	}
}

func TestImportsTopIssues_FromCounts(t *testing.T) {
	section := NewImportsReportSection(newTestImportsReport())
	const topN = 2
	issues := section.TopIssues(topN)
	if len(issues) != topN {
		t.Fatalf("TopIssues(%d) count = %d, want %d", topN, len(issues), topN)
	}
	// Sorted by count desc: fmt(12) first
	if issues[0].Name != "fmt" {
		t.Errorf("issues[0].Name = %q, want %q", issues[0].Name, "fmt")
	}
	if issues[0].Severity != analyze.SeverityInfo {
		t.Errorf("issues[0].Severity = %q, want %q", issues[0].Severity, analyze.SeverityInfo)
	}
}

func TestImportsTopIssues_FromList(t *testing.T) {
	section := NewImportsReportSection(newSimpleImportsReport())
	const topN = 2
	issues := section.TopIssues(topN)
	if len(issues) != topN {
		t.Fatalf("TopIssues(%d) count = %d, want %d", topN, len(issues), topN)
	}
	// Sorted alphabetically: fmt, os
	if issues[0].Name != "fmt" {
		t.Errorf("issues[0].Name = %q, want %q", issues[0].Name, "fmt")
	}
}

func TestImportsAllIssues(t *testing.T) {
	section := NewImportsReportSection(newTestImportsReport())
	const expectedImports = 4
	issues := section.AllIssues()
	if len(issues) != expectedImports {
		t.Errorf("AllIssues() count = %d, want %d", len(issues), expectedImports)
	}
}

func TestImportsTopIssues_Empty(t *testing.T) {
	section := NewImportsReportSection(analyze.Report{})
	const n = 5
	if len(section.TopIssues(n)) != 0 {
		t.Error("TopIssues should be empty for empty report")
	}
}

func TestImportsImplementsInterface(t *testing.T) {
	var _ analyze.ReportSection = (*ImportsReportSection)(nil)
}
