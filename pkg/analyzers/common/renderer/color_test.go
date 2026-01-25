package renderer

import (
	"strings"
	"testing"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/common/terminal"
)

const ansiEscape = "\033["

func TestColorForSeverity_Good(t *testing.T) {
	color := ColorForSeverity(analyze.SeverityGood)
	if color != terminal.ColorGreen {
		t.Errorf("ColorForSeverity(%q) = %v, want ColorGreen", analyze.SeverityGood, color)
	}
}

func TestColorForSeverity_Fair(t *testing.T) {
	color := ColorForSeverity(analyze.SeverityFair)
	if color != terminal.ColorYellow {
		t.Errorf("ColorForSeverity(%q) = %v, want ColorYellow", analyze.SeverityFair, color)
	}
}

func TestColorForSeverity_Poor(t *testing.T) {
	color := ColorForSeverity(analyze.SeverityPoor)
	if color != terminal.ColorRed {
		t.Errorf("ColorForSeverity(%q) = %v, want ColorRed", analyze.SeverityPoor, color)
	}
}

func TestColorForSeverity_Info(t *testing.T) {
	color := ColorForSeverity(analyze.SeverityInfo)
	if color != terminal.ColorBlue {
		t.Errorf("ColorForSeverity(%q) = %v, want ColorBlue", analyze.SeverityInfo, color)
	}
}

func TestColorForSeverity_Unknown(t *testing.T) {
	color := ColorForSeverity("unknown")
	if color != terminal.ColorBlue {
		t.Errorf("ColorForSeverity(%q) = %v, want ColorBlue (default)", "unknown", color)
	}
}

// --- Render color tests ---

func TestRender_ColorEnabled_ContainsANSI(t *testing.T) {
	r := NewSectionRenderer(testWidth, false, false) // color enabled
	section := newMockSection()                      // score 0.8 = green

	result := r.Render(section)

	if !strings.Contains(result, ansiEscape) {
		t.Errorf("Render with color enabled should contain ANSI codes, got %q", result)
	}
}

func TestRender_ColorDisabled_NoANSI(t *testing.T) {
	r := NewSectionRenderer(testWidth, false, true) // color disabled
	section := newMockSection()

	result := r.Render(section)

	if strings.Contains(result, ansiEscape) {
		t.Errorf("Render with color disabled should not contain ANSI codes, got %q", result)
	}
}

func TestRender_GoodScore_GreenColor(t *testing.T) {
	r := NewSectionRenderer(testWidth, false, false)
	section := &mockSection{
		BaseReportSection: analyze.BaseReportSection{
			Title:      "TEST",
			Message:    "Good",
			ScoreValue: 0.9,
		},
	}

	result := r.Render(section)

	// Green ANSI code = \033[32m
	if !strings.Contains(result, "\033[32m") {
		t.Errorf("Good score should use green color, got %q", result)
	}
}

func TestRender_FairScore_YellowColor(t *testing.T) {
	r := NewSectionRenderer(testWidth, false, false)
	section := &mockSection{
		BaseReportSection: analyze.BaseReportSection{
			Title:      "TEST",
			Message:    "Fair",
			ScoreValue: 0.6,
		},
	}

	result := r.Render(section)

	// Yellow ANSI code = \033[33m
	if !strings.Contains(result, "\033[33m") {
		t.Errorf("Fair score should use yellow color, got %q", result)
	}
}

func TestRender_PoorScore_RedColor(t *testing.T) {
	r := NewSectionRenderer(testWidth, false, false)
	section := &mockSection{
		BaseReportSection: analyze.BaseReportSection{
			Title:      "TEST",
			Message:    "Poor",
			ScoreValue: 0.3,
		},
	}

	result := r.Render(section)

	// Red ANSI code = \033[31m
	if !strings.Contains(result, "\033[31m") {
		t.Errorf("Poor score should use red color, got %q", result)
	}
}

func TestRender_IssuesSeverityColored(t *testing.T) {
	r := NewSectionRenderer(testWidth, false, false) // color enabled
	section := newMockSectionWithIssues()            // has poor/fair issues

	result := r.Render(section)

	// Poor severity = red (\033[31m), Fair = yellow (\033[33m)
	if !strings.Contains(result, "\033[31m") {
		t.Errorf("Poor severity issue should contain red ANSI, got %q", result)
	}
	if !strings.Contains(result, "\033[33m") {
		t.Errorf("Fair severity issue should contain yellow ANSI, got %q", result)
	}
}

func TestRender_TitleColoredBlue(t *testing.T) {
	r := NewSectionRenderer(testWidth, false, false)
	section := newMockSection()

	result := r.Render(section)

	// Blue ANSI = \033[34m
	if !strings.Contains(result, "\033[34m") {
		t.Errorf("Section title should be colored blue, got %q", result)
	}
}

// --- RenderCompact color tests ---

func TestRenderCompact_ColorEnabled_ContainsANSI(t *testing.T) {
	r := NewSectionRenderer(testWidth, false, false)
	section := newMockSection()

	result := r.RenderCompact(section)

	if !strings.Contains(result, ansiEscape) {
		t.Errorf("RenderCompact with color should contain ANSI codes, got %q", result)
	}
}

func TestRenderCompact_ColorDisabled_NoANSI(t *testing.T) {
	r := NewSectionRenderer(testWidth, false, true)
	section := newMockSection()

	result := r.RenderCompact(section)

	if strings.Contains(result, ansiEscape) {
		t.Errorf("RenderCompact without color should not contain ANSI codes, got %q", result)
	}
}

// --- RenderSummary color tests ---

func TestRenderSummary_ColorEnabled_ContainsANSI(t *testing.T) {
	r := NewSectionRenderer(testWidth, false, false)
	sections := []analyze.ReportSection{
		newMockSection(),
	}
	summary := NewExecutiveSummary(sections)

	result := r.RenderSummary(summary)

	if !strings.Contains(result, ansiEscape) {
		t.Errorf("RenderSummary with color should contain ANSI codes, got %q", result)
	}
}

func TestRenderSummary_ColorDisabled_NoANSI(t *testing.T) {
	r := NewSectionRenderer(testWidth, false, true)
	sections := []analyze.ReportSection{
		newMockSection(),
	}
	summary := NewExecutiveSummary(sections)

	result := r.RenderSummary(summary)

	if strings.Contains(result, ansiEscape) {
		t.Errorf("RenderSummary without color should not contain ANSI codes, got %q", result)
	}
}

func TestRenderSummary_ScoreRowsColored(t *testing.T) {
	r := NewSectionRenderer(testWidth, false, false)
	goodSection := &mockSection{
		BaseReportSection: analyze.BaseReportSection{
			Title:      "GOOD",
			Message:    "Good",
			ScoreValue: 0.9,
		},
	}
	poorSection := &mockSection{
		BaseReportSection: analyze.BaseReportSection{
			Title:      "POOR",
			Message:    "Poor",
			ScoreValue: 0.3,
		},
	}
	sections := []analyze.ReportSection{goodSection, poorSection}
	summary := NewExecutiveSummary(sections)

	result := r.RenderSummary(summary)

	// Should contain green (for good) and red (for poor)
	if !strings.Contains(result, "\033[32m") {
		t.Errorf("Summary should contain green for good score, got %q", result)
	}
	if !strings.Contains(result, "\033[31m") {
		t.Errorf("Summary should contain red for poor score, got %q", result)
	}
}

// --- Muted elements tests ---

func TestRender_MetricsHeaderMuted(t *testing.T) {
	r := NewSectionRenderer(testWidth, false, false)
	section := newMockSectionWithMetrics()

	result := r.Render(section)

	// Gray ANSI = \033[90m
	if !strings.Contains(result, "\033[90m") {
		t.Errorf("Section headers should use muted gray, got %q", result)
	}
}
