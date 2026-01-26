package analyze //nolint:testpackage // testing internal implementation.

import "testing"

func TestMetric_Fields(t *testing.T) {
	t.Parallel()

	m := Metric{
		Label: "Total Functions",
		Value: "156",
	}

	if m.Label != "Total Functions" {
		t.Errorf("Metric.Label = %q, want %q", m.Label, "Total Functions")
	}

	if m.Value != "156" {
		t.Errorf("Metric.Value = %q, want %q", m.Value, "156")
	}
}

func TestDistributionItem_Fields(t *testing.T) {
	t.Parallel()

	const expectedPercent = 0.68

	const expectedCount = 106

	d := DistributionItem{
		Label:   "Simple (1-5)",
		Percent: expectedPercent,
		Count:   expectedCount,
	}

	if d.Label != "Simple (1-5)" {
		t.Errorf("DistributionItem.Label = %q, want %q", d.Label, "Simple (1-5)")
	}

	if d.Percent != expectedPercent {
		t.Errorf("DistributionItem.Percent = %v, want %v", d.Percent, expectedPercent)
	}

	if d.Count != expectedCount {
		t.Errorf("DistributionItem.Count = %d, want %d", d.Count, expectedCount)
	}
}

func TestIssue_Fields(t *testing.T) {
	t.Parallel()

	i := Issue{
		Name:     "processLargeDataSet",
		Location: "pkg/processor/data.go:142",
		Value:    "12",
		Severity: SeverityPoor,
	}

	if i.Name != "processLargeDataSet" {
		t.Errorf("Issue.Name = %q, want %q", i.Name, "processLargeDataSet")
	}

	if i.Location != "pkg/processor/data.go:142" {
		t.Errorf("Issue.Location = %q, want %q", i.Location, "pkg/processor/data.go:142")
	}

	if i.Value != "12" {
		t.Errorf("Issue.Value = %q, want %q", i.Value, "12")
	}

	if i.Severity != SeverityPoor {
		t.Errorf("Issue.Severity = %q, want %q", i.Severity, SeverityPoor)
	}
}

// mockReportSection is a minimal implementation for testing interface compliance.
type mockReportSection struct{}

func (m *mockReportSection) SectionTitle() string             { return "TEST" }
func (m *mockReportSection) Score() float64                   { return 0.8 }
func (m *mockReportSection) ScoreLabel() string               { return "8/10" }
func (m *mockReportSection) StatusMessage() string            { return "Test message" }
func (m *mockReportSection) KeyMetrics() []Metric             { return nil }
func (m *mockReportSection) Distribution() []DistributionItem { return nil }
func (m *mockReportSection) TopIssues(_ int) []Issue          { return nil }
func (m *mockReportSection) AllIssues() []Issue               { return nil }

func TestReportSection_InterfaceCompliance(t *testing.T) {
	t.Parallel()

	// Compile-time check that mockReportSection implements ReportSection.
	var _ ReportSection = (*mockReportSection)(nil)
}

func TestScoreInfoOnly_Value(t *testing.T) {
	t.Parallel()

	if ScoreInfoOnly >= 0 {
		t.Errorf("ScoreInfoOnly = %v, want negative value", ScoreInfoOnly)
	}
}

func TestBaseReportSection_ImplementsInterface(t *testing.T) {
	t.Parallel()

	// Compile-time check that BaseReportSection implements ReportSection.
	var _ ReportSection = (*BaseReportSection)(nil)
}

func TestBaseReportSection_SectionTitle(t *testing.T) {
	t.Parallel()

	b := &BaseReportSection{Title: "COMPLEXITY"}
	if b.SectionTitle() != "COMPLEXITY" {
		t.Errorf("SectionTitle() = %q, want %q", b.SectionTitle(), "COMPLEXITY")
	}
}

func TestBaseReportSection_Score(t *testing.T) {
	t.Parallel()

	const expectedScore = 0.8

	b := &BaseReportSection{ScoreValue: expectedScore}
	if b.Score() != expectedScore {
		t.Errorf("Score() = %v, want %v", b.Score(), expectedScore)
	}
}

func TestBaseReportSection_ScoreLabel_Numeric(t *testing.T) {
	t.Parallel()

	b := &BaseReportSection{ScoreValue: 0.8}

	result := b.ScoreLabel()
	if result != "8/10" {
		t.Errorf("ScoreLabel() = %q, want %q", result, "8/10")
	}
}

func TestBaseReportSection_ScoreLabel_InfoOnly(t *testing.T) {
	t.Parallel()

	b := &BaseReportSection{ScoreValue: ScoreInfoOnly}

	result := b.ScoreLabel()
	if result != "Info" {
		t.Errorf("ScoreLabel() with ScoreInfoOnly = %q, want %q", result, "Info")
	}
}

func TestBaseReportSection_StatusMessage(t *testing.T) {
	t.Parallel()

	b := &BaseReportSection{Message: "Good - reasonable complexity"}
	if b.StatusMessage() != "Good - reasonable complexity" {
		t.Errorf("StatusMessage() = %q, want %q", b.StatusMessage(), "Good - reasonable complexity")
	}
}

func TestBaseReportSection_KeyMetrics_Default(t *testing.T) {
	t.Parallel()

	b := &BaseReportSection{}
	if b.KeyMetrics() != nil {
		t.Errorf("KeyMetrics() = %v, want nil", b.KeyMetrics())
	}
}

func TestBaseReportSection_Distribution_Default(t *testing.T) {
	t.Parallel()

	b := &BaseReportSection{}
	if b.Distribution() != nil {
		t.Errorf("Distribution() = %v, want nil", b.Distribution())
	}
}

func TestBaseReportSection_TopIssues_Default(t *testing.T) {
	t.Parallel()

	b := &BaseReportSection{}

	const topN = 5
	if b.TopIssues(topN) != nil {
		t.Errorf("TopIssues(%d) = %v, want nil", topN, b.TopIssues(topN))
	}
}

func TestBaseReportSection_AllIssues_Default(t *testing.T) {
	t.Parallel()

	b := &BaseReportSection{}
	if b.AllIssues() != nil {
		t.Errorf("AllIssues() = %v, want nil", b.AllIssues())
	}
}

// mockSectionProvider implements ReportSectionProvider for testing.
type mockSectionProvider struct{}

func (m *mockSectionProvider) CreateReportSection(_ Report) ReportSection {
	return &BaseReportSection{Title: "MOCK", ScoreValue: 0.5, Message: "mock"}
}

func TestReportSectionProvider_InterfaceCompliance(t *testing.T) {
	t.Parallel()

	var _ ReportSectionProvider = (*mockSectionProvider)(nil)
}

func TestReportSectionProvider_CreatesSection(t *testing.T) {
	t.Parallel()

	provider := &mockSectionProvider{}
	section := provider.CreateReportSection(Report{})

	if section.SectionTitle() != "MOCK" {
		t.Errorf("SectionTitle() = %q, want %q", section.SectionTitle(), "MOCK")
	}
}
