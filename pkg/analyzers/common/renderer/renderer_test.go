package renderer //nolint:testpackage // testing internal implementation.

import (
	"strings"
	"testing"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
)

const (
	testWidth     = 80
	testWidthWide = 120
)

func TestNewSectionRenderer_Defaults(t *testing.T) {
	t.Parallel()

	r := NewSectionRenderer(testWidth, false, false)

	if r.config.Width != testWidth {
		t.Errorf("Width = %d, want %d", r.config.Width, testWidth)
	}

	if r.verbose {
		t.Errorf("verbose = %v, want false", r.verbose)
	}

	if r.config.NoColor {
		t.Errorf("NoColor = %v, want false", r.config.NoColor)
	}
}

func TestNewSectionRenderer_Verbose(t *testing.T) {
	t.Parallel()

	r := NewSectionRenderer(testWidth, true, false)

	if !r.verbose {
		t.Errorf("verbose = %v, want true", r.verbose)
	}
}

func TestNewSectionRenderer_NoColor(t *testing.T) {
	t.Parallel()

	r := NewSectionRenderer(testWidth, false, true)

	if !r.config.NoColor {
		t.Errorf("NoColor = %v, want true", r.config.NoColor)
	}
}

// mockSection implements ReportSection for testing.
type mockSection struct {
	analyze.BaseReportSection
}

func newMockSection() *mockSection {
	return &mockSection{
		BaseReportSection: analyze.BaseReportSection{
			Title:      "COMPLEXITY",
			Message:    "Good - reasonable complexity",
			ScoreValue: 0.8,
		},
	}
}

func TestRenderCompact_Format(t *testing.T) {
	t.Parallel()

	r := NewSectionRenderer(testWidth, false, true)
	section := newMockSection()

	result := r.RenderCompact(section)

	// Should contain title.
	if !strings.Contains(result, "COMPLEXITY") {
		t.Errorf("RenderCompact should contain title, got %q", result)
	}
}

func TestRenderCompact_ContainsBar(t *testing.T) {
	t.Parallel()

	r := NewSectionRenderer(testWidth, false, true)
	section := newMockSection()

	result := r.RenderCompact(section)

	// Should contain progress bar characters.
	if !strings.Contains(result, "█") && !strings.Contains(result, "░") {
		t.Errorf("RenderCompact should contain progress bar, got %q", result)
	}
}

func TestRenderCompact_ContainsScore(t *testing.T) {
	t.Parallel()

	r := NewSectionRenderer(testWidth, false, true)
	section := newMockSection()

	result := r.RenderCompact(section)

	// Should contain score.
	if !strings.Contains(result, "8/10") {
		t.Errorf("RenderCompact should contain score, got %q", result)
	}
}

func TestRenderCompact_ContainsMessage(t *testing.T) {
	t.Parallel()

	r := NewSectionRenderer(testWidth, false, true)
	section := newMockSection()

	result := r.RenderCompact(section)

	// Should contain message.
	if !strings.Contains(result, "Good - reasonable complexity") {
		t.Errorf("RenderCompact should contain message, got %q", result)
	}
}

func TestRender_ContainsTitle(t *testing.T) {
	t.Parallel()

	r := NewSectionRenderer(testWidth, false, true)
	section := newMockSection()

	result := r.Render(section)

	if !strings.Contains(result, "COMPLEXITY") {
		t.Errorf("Render should contain title, got %q", result)
	}
}

func TestRender_ContainsScore(t *testing.T) {
	t.Parallel()

	r := NewSectionRenderer(testWidth, false, true)
	section := newMockSection()

	result := r.Render(section)

	if !strings.Contains(result, "8/10") {
		t.Errorf("Render should contain score, got %q", result)
	}
}

func TestRender_ContainsSummary(t *testing.T) {
	t.Parallel()

	r := NewSectionRenderer(testWidth, false, true)
	section := newMockSection()

	result := r.Render(section)

	if !strings.Contains(result, "Good - reasonable complexity") {
		t.Errorf("Render should contain summary message, got %q", result)
	}
}

func TestRender_ContainsHeaderBox(t *testing.T) {
	t.Parallel()

	r := NewSectionRenderer(testWidth, false, true)
	section := newMockSection()

	result := r.Render(section)

	// Should have heavy box characters from DrawHeader.
	if !strings.Contains(result, "┏") || !strings.Contains(result, "┗") {
		t.Errorf("Render should contain header box, got %q", result)
	}
}

// mockSectionWithMetrics adds metrics to the mock section.
type mockSectionWithMetrics struct {
	analyze.BaseReportSection //nolint:embeddedstructfieldcheck // embedded struct field is intentional.
	metrics                   []analyze.Metric
}

func (m *mockSectionWithMetrics) KeyMetrics() []analyze.Metric {
	return m.metrics
}

func newMockSectionWithMetrics() *mockSectionWithMetrics {
	return &mockSectionWithMetrics{
		BaseReportSection: analyze.BaseReportSection{
			Title:      "COMPLEXITY",
			Message:    "Good - reasonable complexity",
			ScoreValue: 0.8,
		},
		metrics: []analyze.Metric{
			{Label: "Total Functions", Value: "156"},
			{Label: "Avg Complexity", Value: "3.2"},
			{Label: "Max Complexity", Value: "12"},
			{Label: "Total Complexity", Value: "499"},
		},
	}
}

func TestRender_ContainsMetricsSection(t *testing.T) {
	t.Parallel()

	r := NewSectionRenderer(testWidth, false, true)
	section := newMockSectionWithMetrics()

	result := r.Render(section)

	if !strings.Contains(result, "Key Metrics") {
		t.Errorf("Render should contain 'Key Metrics' section, got %q", result)
	}
}

func TestRender_ContainsMetricValues(t *testing.T) {
	t.Parallel()

	r := NewSectionRenderer(testWidth, false, true)
	section := newMockSectionWithMetrics()

	result := r.Render(section)

	if !strings.Contains(result, "Total Functions") {
		t.Errorf("Render should contain metric label, got %q", result)
	}

	if !strings.Contains(result, "156") {
		t.Errorf("Render should contain metric value, got %q", result)
	}
}

func TestRender_EmptyMetrics(t *testing.T) {
	t.Parallel()

	r := NewSectionRenderer(testWidth, false, true)
	section := newMockSection() // No metrics.

	result := r.Render(section)

	// Should NOT contain Key Metrics section when empty.
	if strings.Contains(result, "Key Metrics") {
		t.Errorf("Render should not contain 'Key Metrics' when empty, got %q", result)
	}
}

// mockSectionWithDistribution adds distribution to the mock section.
type mockSectionWithDistribution struct {
	analyze.BaseReportSection //nolint:embeddedstructfieldcheck // embedded struct field is intentional.
	distribution              []analyze.DistributionItem
}

func (m *mockSectionWithDistribution) Distribution() []analyze.DistributionItem {
	return m.distribution
}

func newMockSectionWithDistribution() *mockSectionWithDistribution {
	return &mockSectionWithDistribution{
		BaseReportSection: analyze.BaseReportSection{
			Title:      "COMPLEXITY",
			Message:    "Good - reasonable complexity",
			ScoreValue: 0.8,
		},
		distribution: []analyze.DistributionItem{
			{Label: "Simple (1-5)", Percent: 0.68, Count: 106},
			{Label: "Moderate (6-10)", Percent: 0.28, Count: 44},
			{Label: "Complex (11-20)", Percent: 0.04, Count: 6},
		},
	}
}

func TestRender_ContainsDistributionSection(t *testing.T) {
	t.Parallel()

	r := NewSectionRenderer(testWidth, false, true)
	section := newMockSectionWithDistribution()

	result := r.Render(section)

	if !strings.Contains(result, "Distribution") {
		t.Errorf("Render should contain 'Distribution' section, got %q", result)
	}
}

func TestRender_ContainsDistributionBars(t *testing.T) {
	t.Parallel()

	r := NewSectionRenderer(testWidth, false, true)
	section := newMockSectionWithDistribution()

	result := r.Render(section)

	// Should contain progress bar characters.
	if !strings.Contains(result, "█") {
		t.Errorf("Render should contain distribution bars, got %q", result)
	}
	// Should contain percentage.
	if !strings.Contains(result, "68%") {
		t.Errorf("Render should contain percentage, got %q", result)
	}
}

func TestRender_EmptyDistribution(t *testing.T) {
	t.Parallel()

	r := NewSectionRenderer(testWidth, false, true)
	section := newMockSection() // No distribution.

	result := r.Render(section)

	if strings.Contains(result, "Distribution") {
		t.Errorf("Render should not contain 'Distribution' when empty, got %q", result)
	}
}

// mockSectionWithIssues adds issues to the mock section.
type mockSectionWithIssues struct {
	analyze.BaseReportSection //nolint:embeddedstructfieldcheck // embedded struct field is intentional.
	issues                    []analyze.Issue
}

func (m *mockSectionWithIssues) TopIssues(n int) []analyze.Issue {
	if n > len(m.issues) {
		return m.issues
	}

	return m.issues[:n]
}

func (m *mockSectionWithIssues) AllIssues() []analyze.Issue {
	return m.issues
}

func newMockSectionWithIssues() *mockSectionWithIssues {
	return &mockSectionWithIssues{
		BaseReportSection: analyze.BaseReportSection{
			Title:      "COMPLEXITY",
			Message:    "Issues found - high complexity",
			ScoreValue: 0.6,
		},
		issues: []analyze.Issue{
			{Name: "ProcessData", Location: "pkg/data/processor.go:45", Value: "CC=18", Severity: analyze.SeverityPoor},
			{Name: "HandleRequest", Location: "pkg/http/handler.go:120", Value: "CC=15", Severity: analyze.SeverityFair},
			{Name: "ParseConfig", Location: "pkg/config/parser.go:89", Value: "CC=12", Severity: analyze.SeverityFair},
		},
	}
}

func TestRender_ContainsIssuesSection(t *testing.T) {
	t.Parallel()

	r := NewSectionRenderer(testWidth, false, true)
	section := newMockSectionWithIssues()

	result := r.Render(section)

	if !strings.Contains(result, "Top Issues") {
		t.Errorf("Render should contain 'Top Issues' section, got %q", result)
	}
}

func TestRender_ContainsIssueDetails(t *testing.T) {
	t.Parallel()

	r := NewSectionRenderer(testWidth, false, true)
	section := newMockSectionWithIssues()

	result := r.Render(section)

	if !strings.Contains(result, "ProcessData") {
		t.Errorf("Render should contain issue name, got %q", result)
	}

	if !strings.Contains(result, "processor.go") {
		t.Errorf("Render should contain issue location, got %q", result)
	}
}

func TestRender_EmptyIssues(t *testing.T) {
	t.Parallel()

	r := NewSectionRenderer(testWidth, false, true)
	section := newMockSection() // No issues.

	result := r.Render(section)

	if strings.Contains(result, "Top Issues") {
		t.Errorf("Render should not contain 'Top Issues' when empty, got %q", result)
	}
}

func newMockSectionWithManyIssues() *mockSectionWithIssues {
	issues := []analyze.Issue{
		{Name: "Func1", Value: "CC=18", Severity: analyze.SeverityPoor},
		{Name: "Func2", Value: "CC=15", Severity: analyze.SeverityPoor},
		{Name: "Func3", Value: "CC=14", Severity: analyze.SeverityFair},
		{Name: "Func4", Value: "CC=13", Severity: analyze.SeverityFair},
		{Name: "Func5", Value: "CC=12", Severity: analyze.SeverityFair},
		{Name: "Func6", Value: "CC=11", Severity: analyze.SeverityFair},
		{Name: "Func7", Value: "CC=10", Severity: analyze.SeverityFair},
	}

	return &mockSectionWithIssues{
		BaseReportSection: analyze.BaseReportSection{
			Title:      "COMPLEXITY",
			Message:    "Issues found",
			ScoreValue: 0.4,
		},
		issues: issues,
	}
}

func TestRender_NonVerboseShowsTopIssues(t *testing.T) {
	t.Parallel()

	r := NewSectionRenderer(testWidth, false, true)
	section := newMockSectionWithManyIssues()

	result := r.Render(section)

	// TopIssues(5) should show Func1..Func5 but NOT Func6, Func7.
	if !strings.Contains(result, "Func1") {
		t.Errorf("Non-verbose should contain Func1, got %q", result)
	}

	if !strings.Contains(result, "Func5") {
		t.Errorf("Non-verbose should contain Func5, got %q", result)
	}

	if strings.Contains(result, "Func6") {
		t.Errorf("Non-verbose should NOT contain Func6, got %q", result)
	}
}

func TestRender_VerboseShowsAllIssues(t *testing.T) {
	t.Parallel()

	r := NewSectionRenderer(testWidth, true, true)
	section := newMockSectionWithManyIssues()

	result := r.Render(section)

	// AllIssues should show all 7 functions.
	if !strings.Contains(result, "Func1") {
		t.Errorf("Verbose should contain Func1, got %q", result)
	}

	if !strings.Contains(result, "Func7") {
		t.Errorf("Verbose should contain Func7, got %q", result)
	}
}

func TestRender_VerboseChangesLabel(t *testing.T) {
	t.Parallel()

	r := NewSectionRenderer(testWidth, true, true)
	section := newMockSectionWithManyIssues()

	result := r.Render(section)

	// Verbose should show "All Issues" instead of "Top Issues".
	if !strings.Contains(result, "All Issues") {
		t.Errorf("Verbose should show 'All Issues' label, got %q", result)
	}

	if strings.Contains(result, "Top Issues") {
		t.Errorf("Verbose should NOT show 'Top Issues' label, got %q", result)
	}
}
