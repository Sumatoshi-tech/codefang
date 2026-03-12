package renderer

import (
	"testing"

	"github.com/stretchr/testify/assert"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
)

// summaryMockSection is a ReportSection for summary tests.
type summaryMockSection struct {
	analyze.BaseReportSection
}

func newSummaryMock(title string, score float64, message string) *summaryMockSection {
	return &summaryMockSection{
		BaseReportSection: analyze.BaseReportSection{
			Title:      title,
			ScoreValue: score,
			Message:    message,
		},
	}
}

func TestNewExecutiveSummary_StoresSections(t *testing.T) {
	t.Parallel()

	s1 := newSummaryMock("COMPLEXITY", 0.8, "Good")
	s2 := newSummaryMock("COMMENTS", 0.6, "Fair")
	sections := []analyze.ReportSection{s1, s2}

	summary := NewExecutiveSummary(sections)

	assert.Len(t, summary.Sections, 2)
	assert.Equal(t, "COMPLEXITY", summary.Sections[0].SectionTitle())
	assert.Equal(t, "COMMENTS", summary.Sections[1].SectionTitle())
}

func TestNewExecutiveSummary_Empty(t *testing.T) {
	t.Parallel()

	summary := NewExecutiveSummary(nil)

	assert.NotNil(t, summary)
	assert.Empty(t, summary.Sections)
}

func TestOverallScore_SingleSection(t *testing.T) {
	t.Parallel()

	s := newSummaryMock("COMPLEXITY", 0.8, "Good")
	summary := NewExecutiveSummary([]analyze.ReportSection{s})

	assert.InDelta(t, 0.8, summary.OverallScore(), 0.001)
}

func TestOverallScore_MultipleSections(t *testing.T) {
	t.Parallel()

	s1 := newSummaryMock("COMPLEXITY", 0.8, "Good")
	s2 := newSummaryMock("COMMENTS", 0.6, "Fair")
	summary := NewExecutiveSummary([]analyze.ReportSection{s1, s2})

	assert.InDelta(t, 0.7, summary.OverallScore(), 0.001)
}

func TestOverallScore_SkipsInfoOnly(t *testing.T) {
	t.Parallel()

	s1 := newSummaryMock("COMPLEXITY", 0.8, "Good")
	s2 := newSummaryMock("IMPORTS", analyze.ScoreInfoOnly, "5 imports")
	summary := NewExecutiveSummary([]analyze.ReportSection{s1, s2})

	assert.InDelta(t, 0.8, summary.OverallScore(), 0.001)
}

func TestOverallScore_AllInfoOnly(t *testing.T) {
	t.Parallel()

	s := newSummaryMock("IMPORTS", analyze.ScoreInfoOnly, "5 imports")
	summary := NewExecutiveSummary([]analyze.ReportSection{s})

	assert.InDelta(t, analyze.ScoreInfoOnly, summary.OverallScore(), 0.001)
}

func TestOverallScore_Empty(t *testing.T) {
	t.Parallel()

	summary := NewExecutiveSummary(nil)

	assert.InDelta(t, analyze.ScoreInfoOnly, summary.OverallScore(), 0.001)
}

func TestOverallScoreLabel_Formatted(t *testing.T) {
	t.Parallel()

	s := newSummaryMock("COMPLEXITY", 0.8, "Good")
	summary := NewExecutiveSummary([]analyze.ReportSection{s})

	assert.Equal(t, "8/10", summary.OverallScoreLabel())
}

func TestOverallScoreLabel_InfoOnly(t *testing.T) {
	t.Parallel()

	s := newSummaryMock("IMPORTS", analyze.ScoreInfoOnly, "5 imports")
	summary := NewExecutiveSummary([]analyze.ReportSection{s})

	assert.Equal(t, analyze.ScoreLabelInfo, summary.OverallScoreLabel())
}

func TestRenderSummary_ContainsTitle(t *testing.T) {
	t.Parallel()

	s := newSummaryMock("COMPLEXITY", 0.8, "Good")
	summary := NewExecutiveSummary([]analyze.ReportSection{s})
	r := NewSectionRenderer(testWidth, false, true)

	output := r.RenderSummary(summary)

	assert.Contains(t, output, SummaryTitle)
}

func TestRenderSummary_ContainsOverallScore(t *testing.T) {
	t.Parallel()

	s := newSummaryMock("COMPLEXITY", 0.8, "Good")
	summary := NewExecutiveSummary([]analyze.ReportSection{s})
	r := NewSectionRenderer(testWidth, false, true)

	output := r.RenderSummary(summary)

	assert.Contains(t, output, "Overall: 8/10")
}

func TestRenderSummary_ContainsColumnHeaders(t *testing.T) {
	t.Parallel()

	s := newSummaryMock("COMPLEXITY", 0.8, "Good")
	summary := NewExecutiveSummary([]analyze.ReportSection{s})
	r := NewSectionRenderer(testWidth, false, true)

	output := r.RenderSummary(summary)

	assert.Contains(t, output, SummaryAnalyzerCol)
	assert.Contains(t, output, SummaryScoreCol)
	assert.Contains(t, output, SummaryStatusCol)
}

func TestRenderSummary_ContainsAllAnalyzers(t *testing.T) {
	t.Parallel()

	s1 := newSummaryMock("COMPLEXITY", 0.8, "Good")
	s2 := newSummaryMock("COMMENTS", 0.6, "Fair")
	s3 := newSummaryMock("IMPORTS", analyze.ScoreInfoOnly, "5 imports")
	summary := NewExecutiveSummary([]analyze.ReportSection{s1, s2, s3})
	r := NewSectionRenderer(testWidth, false, true)

	output := r.RenderSummary(summary)

	assert.Contains(t, output, "COMPLEXITY")
	assert.Contains(t, output, "COMMENTS")
	assert.Contains(t, output, "IMPORTS")
}

func TestRenderSummary_ContainsScores(t *testing.T) {
	t.Parallel()

	s1 := newSummaryMock("COMPLEXITY", 0.8, "Good")
	s2 := newSummaryMock("COMMENTS", 0.6, "Fair")
	summary := NewExecutiveSummary([]analyze.ReportSection{s1, s2})
	r := NewSectionRenderer(testWidth, false, true)

	output := r.RenderSummary(summary)

	assert.Contains(t, output, "8/10")
	assert.Contains(t, output, "6/10")
}

func TestRenderSummary_ContainsMessages(t *testing.T) {
	t.Parallel()

	s := newSummaryMock("COMPLEXITY", 0.8, "Good - reasonable complexity")
	summary := NewExecutiveSummary([]analyze.ReportSection{s})
	r := NewSectionRenderer(testWidth, false, true)

	output := r.RenderSummary(summary)

	assert.Contains(t, output, "Good - reasonable complexity")
}

func TestRenderSummary_InfoOnlySection(t *testing.T) {
	t.Parallel()

	s := newSummaryMock("IMPORTS", analyze.ScoreInfoOnly, "5 imports")
	summary := NewExecutiveSummary([]analyze.ReportSection{s})
	r := NewSectionRenderer(testWidth, false, true)

	output := r.RenderSummary(summary)

	assert.Contains(t, output, "Info")
	assert.Contains(t, output, "5 imports")
}

func TestRenderSummary_EmptySections(t *testing.T) {
	t.Parallel()

	summary := NewExecutiveSummary(nil)
	r := NewSectionRenderer(testWidth, false, true)

	output := r.RenderSummary(summary)

	assert.Contains(t, output, SummaryTitle)
}
